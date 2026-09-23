use rusqlite::Connection;

use crate::DbError;

pub const DEFAULT_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchScope {
    #[default]
    Kjv,
    Commentary,
    Dictionaries,
    Topics,
    Notes,
    All,
}

impl SearchScope {
    pub const ALL: [SearchScope; 6] = [
        SearchScope::Kjv,
        SearchScope::Commentary,
        SearchScope::Dictionaries,
        SearchScope::Topics,
        SearchScope::Notes,
        SearchScope::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Kjv => "KJV",
            Self::Commentary => "Commentary",
            Self::Dictionaries => "Dictionaries",
            Self::Topics => "Topics",
            Self::Notes => "Notes",
            Self::All => "All",
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or(Self::Kjv)
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|&s| s == self).unwrap_or(0) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryKind {
    Verse,
    Commentary,
    Dictionary,
    Topic,
    Lexicon,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryHit {
    pub kind: LibraryKind,
    pub module: String,
    pub title: String,
    pub book: Option<u8>,
    pub chapter: Option<u8>,
    pub verse: Option<u8>,
    pub headword: Option<String>,
    pub snippet: String,
}

impl LibraryHit {
    fn from_verse(hit: SearchHit) -> Self {
        Self {
            kind: LibraryKind::Verse,
            module: "KJV".into(),
            title: "King James Version".into(),
            book: Some(hit.book),
            chapter: Some(hit.chapter),
            verse: Some(hit.verse),
            headword: None,
            snippet: hit.snippet,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MatchMode {
    #[default]
    Phrase,
    AllWords,
    AnyWord,
}

impl MatchMode {
    pub const ALL: [MatchMode; 3] = [Self::Phrase, Self::AllWords, Self::AnyWord];

    pub fn label(self) -> &'static str {
        match self {
            Self::Phrase => "Phrase",
            Self::AllWords => "All words",
            Self::AnyWord => "Any word",
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(Self::Phrase)
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|&m| m == self).unwrap_or(0) as u32
    }
}

/// Words to emphasize, plus the FTS5 expression that finds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledQuery {
    pub fts: String,
    pub tokens: Vec<String>,
    /// Set when the whole query is a Strong's code such as `H430`.
    pub strongs: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerseFilter {
    pub book: Option<u8>,
    pub chapter: Option<u8>,
    pub book_min: u8,
    pub book_max: u8,
}

impl Default for VerseFilter {
    fn default() -> Self {
        Self {
            book: None,
            chapter: None,
            book_min: 1,
            book_max: 66,
        }
    }
}

impl VerseFilter {
    pub fn all() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookCount {
    pub book: u8,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPage {
    pub hits: Vec<LibraryHit>,
    pub total: i64,
    pub by_book: Vec<BookCount>,
}

/// Turn typed text into an FTS5 MATCH query.
///
/// Multi-word input becomes a phrase so "only begotten" matches that pair,
/// not any verse that happens to contain both words.
pub fn match_query(input: &str) -> Option<String> {
    let compiled = compile_query(input, MatchMode::Phrase)?;
    if compiled.strongs.is_some() {
        return None;
    }
    Some(compiled.fts)
}

/// Compile a search box into an FTS expression or a Strong's code.
///
/// Uppercase `AND`, `OR`, `NOT`, and `NEAR/5` are operators. Lowercase
/// "and" stays a word so a phrase like "bread and wine" still matches.
/// A trailing `*` is a prefix. Apostrophes break tokens, so "God's" searches God.
pub fn compile_query(input: &str, mode: MatchMode) -> Option<CompiledQuery> {
    let trimmed = input.trim();
    if let Some(code) = strongs_query(trimmed) {
        return Some(CompiledQuery {
            fts: String::new(),
            tokens: Vec::new(),
            strongs: Some(code),
        });
    }
    let pieces = tokenize(trimmed);
    let words: Vec<&Piece> = pieces
        .iter()
        .filter(|p| matches!(p, Piece::Word { .. }))
        .collect();
    if words.is_empty() {
        return None;
    }
    let explicit = pieces.iter().any(|p| matches!(p, Piece::Op(_)));
    let fts = if explicit {
        explicit_fts(&pieces)
    } else {
        mode_fts(&pieces, mode)
    };
    let fts = fts?;
    let tokens = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Word { text, .. } => Some(text.clone()),
            Piece::Op(_) => None,
        })
        .collect();
    Some(CompiledQuery {
        fts,
        tokens,
        strongs: None,
    })
}

fn strongs_query(input: &str) -> Option<String> {
    let mut chars = input.chars();
    let lang = chars.next()?.to_ascii_uppercase();
    if lang != 'H' && lang != 'G' {
        return None;
    }
    let rest: String = chars.collect();
    if rest.is_empty() || !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let num: i32 = rest.parse().ok()?;
    if num <= 0 {
        return None;
    }
    Some(format!("{lang}{num}"))
}

enum Piece {
    Word { text: String, prefix: bool },
    Op(String),
}

fn tokenize(input: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    for raw in split_words(&strip_possessives(input)) {
        if let Some(op) = operator(&raw) {
            pieces.push(Piece::Op(op));
            continue;
        }
        let prefix = raw.ends_with('*');
        let text: String = raw
            .trim_end_matches('*')
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        if text.is_empty() {
            continue;
        }
        pieces.push(Piece::Word { text, prefix });
    }
    pieces
}

fn strip_possessives(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let apostrophe = c == '\'' || c == '\u{2019}' || c == '\u{2018}';
        if apostrophe {
            let next = chars.get(i + 1).copied();
            if next.is_some_and(|n| n.eq_ignore_ascii_case(&'s')) {
                let after = chars.get(i + 2).copied();
                if after.is_none_or(|n| !n.is_ascii_alphanumeric()) {
                    i += 2;
                    continue;
                }
            }
            out.push(' ');
            i += 1;
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn split_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    for c in input.chars() {
        if c.is_whitespace() {
            if !cur.is_empty() {
                words.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn operator(raw: &str) -> Option<String> {
    if let Some(rest) = raw
        .strip_prefix("NEAR/")
        .or_else(|| raw.strip_prefix("near/"))
    {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return Some(format!("NEAR/{rest}"));
        }
    }
    match raw {
        "AND" | "OR" | "NOT" | "NEAR" => Some(raw.to_string()),
        _ => None,
    }
}

fn word_fts(text: &str, prefix: bool) -> String {
    if prefix {
        format!("{text}*")
    } else {
        text.to_string()
    }
}

fn mode_fts(pieces: &[Piece], mode: MatchMode) -> Option<String> {
    let words: Vec<&Piece> = pieces
        .iter()
        .filter(|p| matches!(p, Piece::Word { .. }))
        .collect();
    if words.is_empty() {
        return None;
    }
    let any_prefix = words
        .iter()
        .any(|p| matches!(p, Piece::Word { prefix: true, .. }));
    if mode == MatchMode::Phrase && words.len() > 1 && !any_prefix {
        let inner = words
            .iter()
            .filter_map(|p| match p {
                Piece::Word { text, .. } => Some(text.as_str()),
                Piece::Op(_) => None,
            })
            .collect::<Vec<_>>()
            .join(" ");
        return Some(format!("\"{inner}\""));
    }
    let joiner = match mode {
        MatchMode::AnyWord => " OR ",
        MatchMode::Phrase | MatchMode::AllWords => " AND ",
    };
    Some(
        words
            .iter()
            .filter_map(|p| match p {
                Piece::Word { text, prefix } => Some(word_fts(text, *prefix)),
                Piece::Op(_) => None,
            })
            .collect::<Vec<_>>()
            .join(joiner),
    )
}

fn explicit_fts(pieces: &[Piece]) -> Option<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in pieces {
        match piece {
            Piece::Op(op) => {
                if out.is_empty() && op != "NOT" {
                    continue;
                }
                if out
                    .last()
                    .is_some_and(|prev| prev == "AND" || prev == "OR" || prev.starts_with("NEAR"))
                {
                    continue;
                }
                out.push(op.clone());
            }
            Piece::Word { text, prefix } => {
                if out.last().is_some_and(|prev| !is_op(prev)) && !out.is_empty() {
                    out.push("AND".into());
                }
                out.push(word_fts(text, *prefix));
            }
        }
    }
    while out.last().is_some_and(|p| is_op(p)) {
        out.pop();
    }
    if out.iter().all(|p| is_op(p)) || out.is_empty() {
        return None;
    }
    Some(out.join(" "))
}

fn is_op(token: &str) -> bool {
    token == "AND"
        || token == "OR"
        || token == "NOT"
        || token == "NEAR"
        || token.starts_with("NEAR/")
}

pub fn rebuild_verses_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
            text,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        DELETE FROM verses_fts;
        INSERT INTO verses_fts (text, book, chapter, verse)
        SELECT text, book, chapter, verse FROM verses;
        "#,
    )?;
    Ok(())
}

pub fn ensure_verses_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
            text,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        "#,
    )?;
    let verses: i64 = conn.query_row("SELECT COUNT(*) FROM verses", [], |r| r.get(0))?;
    let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM verses_fts", [], |r| r.get(0))?;
    if verses != indexed {
        rebuild_verses_fts(conn)?;
    }
    Ok(())
}

pub fn rebuild_resources_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS resources_fts USING fts5(
            text,
            module UNINDEXED,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        DELETE FROM resources_fts;
        INSERT INTO resources_fts (text, module, book, chapter, verse)
        SELECT text, module, book, chapter, verse FROM resources;
        "#,
    )?;
    Ok(())
}

pub fn ensure_resources_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS resources_fts USING fts5(
            text,
            module UNINDEXED,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        "#,
    )?;
    let rows: i64 = conn.query_row("SELECT COUNT(*) FROM resources", [], |r| r.get(0))?;
    let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM resources_fts", [], |r| r.get(0))?;
    if rows != indexed {
        rebuild_resources_fts(conn)?;
    }
    Ok(())
}

pub fn rebuild_entries_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
            headword,
            text,
            module UNINDEXED,
            i UNINDEXED,
            tokenize = 'unicode61'
        );
        DELETE FROM entries_fts;
        INSERT INTO entries_fts (headword, text, module, i)
        SELECT e.headword, e.text, e.module, e.i
        FROM entries e
        JOIN modules m ON m.id = e.module
        WHERE m.kind != 'lexicon';
        "#,
    )?;
    Ok(())
}

fn indexed_entry_count(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM entries e
         JOIN modules m ON m.id = e.module
         WHERE m.kind != 'lexicon'",
        [],
        |r| r.get(0),
    )?)
}

pub fn ensure_entries_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
            headword,
            text,
            module UNINDEXED,
            i UNINDEXED,
            tokenize = 'unicode61'
        );
        "#,
    )?;
    let rows = indexed_entry_count(conn)?;
    let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM entries_fts", [], |r| r.get(0))?;
    if rows != indexed {
        rebuild_entries_fts(conn)?;
    }
    Ok(())
}

pub fn search_verses(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, DbError> {
    let Some(compiled) = compile_query(query, MatchMode::Phrase) else {
        return Ok(Vec::new());
    };
    if compiled.strongs.is_some() {
        return Ok(Vec::new());
    }
    Ok(verse_hits(conn, &compiled, VerseFilter::all(), None, limit)?.0)
}

/// Verse hits in biblical order, with a real count and a count per book.
///
/// `chip` narrows the hit list. `by_book` ignores `chip` so the book row can
/// still show the rest of the range. `after` is the last reference already shown.
pub fn search_verse_page(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    chip: Option<u8>,
    after: Option<(u8, u8, u8)>,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    if compiled.strongs.is_some() || compiled.fts.is_empty() {
        return Ok(LibraryPage {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
        });
    }
    let by_book = verse_book_counts(conn, &compiled.fts, filter)?;
    let mut narrowed = filter;
    if let Some(book) = chip {
        narrowed.book = Some(book);
    }
    let (hits, total) = verse_hits(conn, compiled, narrowed, after, limit)?;
    Ok(LibraryPage {
        hits: hits.into_iter().map(LibraryHit::from_verse).collect(),
        total,
        by_book,
    })
}

fn verse_hits(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    after: Option<(u8, u8, u8)>,
    limit: usize,
) -> Result<(Vec<SearchHit>, i64), DbError> {
    let (book, chapter, book_min, book_max) = filter_params(filter);
    let (after_book, after_chapter, after_verse) = after_params(after);
    let total: i64 = conn.query_row(
        r#"
        SELECT count(*) FROM verses_fts
        WHERE verses_fts MATCH ?1
          AND book BETWEEN ?2 AND ?3
          AND (?4 = 0 OR book = ?4)
          AND (?5 = 0 OR chapter = ?5)
        "#,
        rusqlite::params![compiled.fts, book_min, book_max, book, chapter],
        |row| row.get(0),
    )?;
    let limit = limit.max(1) as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT book, chapter, verse, text
        FROM verses_fts
        WHERE verses_fts MATCH ?1
          AND book BETWEEN ?2 AND ?3
          AND (?4 = 0 OR book = ?4)
          AND (?5 = 0 OR chapter = ?5)
          AND (
            ?6 = 0
            OR book > ?6
            OR (book = ?6 AND chapter > ?7)
            OR (book = ?6 AND chapter = ?7 AND verse > ?8)
          )
        ORDER BY book, chapter, verse
        LIMIT ?9
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            compiled.fts,
            book_min,
            book_max,
            book,
            chapter,
            after_book,
            after_chapter,
            after_verse,
            limit
        ],
        |row| {
            let text: String = row.get(3)?;
            Ok(SearchHit {
                book: row.get(0)?,
                chapter: row.get(1)?,
                verse: row.get(2)?,
                snippet: plain_line(&strip_trailing_notes(&text)),
                text,
            })
        },
    )?;
    Ok((rows.collect::<Result<Vec<_>, _>>()?, total))
}

fn verse_book_counts(
    conn: &Connection,
    fts: &str,
    filter: VerseFilter,
) -> Result<Vec<BookCount>, DbError> {
    let chapter = i64::from(filter.chapter.unwrap_or(0));
    let mut stmt = conn.prepare(
        r#"
        SELECT book, count(*)
        FROM verses_fts
        WHERE verses_fts MATCH ?1
          AND book BETWEEN ?2 AND ?3
          AND (?4 = 0 OR chapter = ?4)
        GROUP BY book
        ORDER BY book
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![fts, filter.book_min, filter.book_max, chapter],
        |row| {
            Ok(BookCount {
                book: row.get(0)?,
                count: row.get(1)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn filter_params(filter: VerseFilter) -> (i64, i64, i64, i64) {
    (
        i64::from(filter.book.unwrap_or(0)),
        i64::from(filter.chapter.unwrap_or(0)),
        i64::from(filter.book_min),
        i64::from(filter.book_max),
    )
}

fn after_params(after: Option<(u8, u8, u8)>) -> (i64, i64, i64) {
    match after {
        Some((book, chapter, verse)) => (i64::from(book), i64::from(chapter), i64::from(verse)),
        None => (0, 0, 0),
    }
}

fn strip_trailing_notes(text: &str) -> String {
    let mut end = text.trim_end().len();
    loop {
        let current = text[..end].trim_end();
        end = current.len();
        if !current.ends_with('}') {
            break;
        }
        let Some(open) = current.rfind('{') else {
            break;
        };
        if current[open + 1..end.saturating_sub(1)].contains('{') {
            break;
        }
        end = open;
    }
    text[..end].trim().to_string()
}

fn plain_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn window_snippet(text: &str, tokens: &[String], width: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return String::new();
    }
    let lower = flat.to_lowercase();
    let mut at = 0usize;
    for token in tokens {
        let needle = token.to_lowercase();
        if needle.is_empty() {
            continue;
        }
        if let Some(pos) = find_token(&lower, &needle) {
            at = pos;
            break;
        }
    }
    let start = at.saturating_sub(width / 3);
    let start = floor_char(&flat, start);
    let end = ceil_char(&flat, (start + width).min(flat.len()));
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(flat[start..end].trim());
    if end < flat.len() {
        out.push('…');
    }
    out
}

fn find_token(haystack: &str, needle: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(needle) {
        let pos = from + rel;
        let before = haystack[..pos].chars().next_back();
        let after = haystack[pos + needle.len()..].chars().next();
        let left_ok = before.is_none_or(|c| !c.is_ascii_alphanumeric());
        let right_ok = after.is_none_or(|c| !c.is_ascii_alphanumeric());
        if left_ok && right_ok {
            return Some(pos);
        }
        from = pos + needle.len();
    }
    None
}

fn floor_char(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    let mut i = byte;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(text: &str, byte: usize) -> usize {
    if byte >= text.len() {
        return text.len();
    }
    let mut i = byte;
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

pub fn search_library(
    conn: &Connection,
    query: &str,
    scope: SearchScope,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let Some(compiled) = compile_query(query, MatchMode::Phrase) else {
        return Ok(Vec::new());
    };
    if compiled.strongs.is_some() {
        return Ok(Vec::new());
    }
    Ok(search_filtered(
        conn,
        &compiled,
        scope,
        VerseFilter::all(),
        None,
        None,
        limit,
    )?
    .hits)
}

/// Search one scope with a compiled query, a testament or book window, and paging.
///
/// `chip` narrows verse and commentary hits. Book counts cover the window
/// before that narrowing. Notes live in the user database and return an empty page.
pub fn search_filtered(
    conn: &Connection,
    compiled: &CompiledQuery,
    scope: SearchScope,
    filter: VerseFilter,
    chip: Option<u8>,
    after: Option<(u8, u8, u8)>,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    if compiled.strongs.is_some() || compiled.fts.is_empty() {
        return Ok(empty_page());
    }
    let limit = limit.max(1);
    match scope {
        SearchScope::Kjv => search_verse_page(conn, compiled, filter, chip, after, limit),
        SearchScope::Commentary => {
            search_commentary_page(conn, compiled, filter, chip, after, limit)
        }
        SearchScope::Dictionaries => search_entry_page(conn, compiled, "dictionary", limit),
        SearchScope::Topics => search_entry_page(conn, compiled, "topic", limit),
        SearchScope::Notes => Ok(empty_page()),
        SearchScope::All => search_all_page(conn, compiled, filter, chip, limit),
    }
}

fn empty_page() -> LibraryPage {
    LibraryPage {
        hits: Vec::new(),
        total: 0,
        by_book: Vec::new(),
    }
}

fn search_commentary_page(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    chip: Option<u8>,
    after: Option<(u8, u8, u8)>,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    let by_book = commentary_book_counts(conn, &compiled.fts, filter)?;
    let mut narrowed = filter;
    if let Some(book) = chip {
        narrowed.book = Some(book);
    }
    let hits = commentary_hits(conn, compiled, narrowed, after, limit)?;
    let (book, chapter, book_min, book_max) = filter_params(narrowed);
    let total: i64 = conn.query_row(
        r#"
        SELECT count(*)
        FROM resources_fts
        JOIN modules m ON m.id = resources_fts.module
        WHERE resources_fts MATCH ?1 AND m.kind = 'commentary'
          AND resources_fts.book BETWEEN ?2 AND ?3
          AND (?4 = 0 OR resources_fts.book = ?4)
          AND (?5 = 0 OR resources_fts.chapter = ?5)
        "#,
        rusqlite::params![compiled.fts, book_min, book_max, book, chapter],
        |row| row.get(0),
    )?;
    Ok(LibraryPage {
        hits,
        total,
        by_book,
    })
}

fn commentary_hits(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    after: Option<(u8, u8, u8)>,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let (book, chapter, book_min, book_max) = filter_params(filter);
    let (after_book, after_chapter, after_verse) = after_params(after);
    let limit = limit.max(1) as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT resources_fts.module, m.title,
               resources_fts.book, resources_fts.chapter, resources_fts.verse,
               resources_fts.text,
               snippet(resources_fts, 0, '', '', '…', 16)
        FROM resources_fts
        JOIN modules m ON m.id = resources_fts.module
        WHERE resources_fts MATCH ?1 AND m.kind = 'commentary'
          AND resources_fts.book BETWEEN ?2 AND ?3
          AND (?4 = 0 OR resources_fts.book = ?4)
          AND (?5 = 0 OR resources_fts.chapter = ?5)
          AND (
            ?6 = 0
            OR resources_fts.book > ?6
            OR (resources_fts.book = ?6 AND resources_fts.chapter > ?7)
            OR (resources_fts.book = ?6 AND resources_fts.chapter = ?7
                AND resources_fts.verse > ?8)
          )
        ORDER BY resources_fts.book, resources_fts.chapter, resources_fts.verse,
                 resources_fts.module
        LIMIT ?9
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            compiled.fts,
            book_min,
            book_max,
            book,
            chapter,
            after_book,
            after_chapter,
            after_verse,
            limit
        ],
        |row| {
            let text: String = row.get(5)?;
            let snippet: String = row.get(6)?;
            Ok(LibraryHit {
                kind: LibraryKind::Commentary,
                module: row.get(0)?,
                title: row.get(1)?,
                book: Some(row.get(2)?),
                chapter: Some(row.get(3)?),
                verse: Some(row.get(4)?),
                headword: None,
                snippet: coalesce_snippet(snippet, &text),
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn commentary_book_counts(
    conn: &Connection,
    fts: &str,
    filter: VerseFilter,
) -> Result<Vec<BookCount>, DbError> {
    let chapter = i64::from(filter.chapter.unwrap_or(0));
    let mut stmt = conn.prepare(
        r#"
        SELECT resources_fts.book, count(*)
        FROM resources_fts
        JOIN modules m ON m.id = resources_fts.module
        WHERE resources_fts MATCH ?1 AND m.kind = 'commentary'
          AND resources_fts.book BETWEEN ?2 AND ?3
          AND (?4 = 0 OR resources_fts.chapter = ?4)
        GROUP BY resources_fts.book
        ORDER BY resources_fts.book
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![fts, filter.book_min, filter.book_max, chapter],
        |row| {
            Ok(BookCount {
                book: row.get(0)?,
                count: row.get(1)?,
            })
        },
    )?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn search_entry_page(
    conn: &Connection,
    compiled: &CompiledQuery,
    kind: &str,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    let hits = search_entry_kind(conn, compiled, kind, limit)?;
    let total: i64 = conn.query_row(
        r#"
        SELECT count(*)
        FROM entries_fts
        JOIN modules m ON m.id = entries_fts.module
        WHERE entries_fts MATCH ?1 AND m.kind = ?2
        "#,
        rusqlite::params![compiled.fts, kind],
        |row| row.get(0),
    )?;
    Ok(LibraryPage {
        hits,
        total,
        by_book: Vec::new(),
    })
}

fn search_entry_kind(
    conn: &Connection,
    compiled: &CompiledQuery,
    kind: &str,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let library_kind = match kind {
        "topic" => LibraryKind::Topic,
        "lexicon" => LibraryKind::Lexicon,
        _ => LibraryKind::Dictionary,
    };
    let limit = limit.max(1) as i64;
    let exact = compiled.tokens.first().cloned().unwrap_or_default();
    let mut stmt = conn.prepare(
        r#"
        SELECT entries_fts.module, m.title, entries_fts.headword, entries_fts.text,
               snippet(entries_fts, 1, '', '', '…', 16)
        FROM entries_fts
        JOIN modules m ON m.id = entries_fts.module
        WHERE entries_fts MATCH ?1 AND m.kind = ?2
        ORDER BY CASE WHEN entries_fts.headword = ?4 COLLATE NOCASE THEN 0 ELSE 1 END,
                 bm25(entries_fts, 5.0, 1.0),
                 entries_fts.headword COLLATE NOCASE,
                 entries_fts.i
        LIMIT ?3
        "#,
    )?;
    let rows = stmt.query_map(rusqlite::params![compiled.fts, kind, limit, exact], |row| {
        let headword: String = row.get(2)?;
        let text: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let snippet = coalesce_snippet(snippet, if text.is_empty() { &headword } else { &text });
        Ok(LibraryHit {
            kind: library_kind,
            module: row.get(0)?,
            title: row.get(1)?,
            book: None,
            chapter: None,
            verse: None,
            headword: Some(headword),
            snippet,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn search_all_page(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    chip: Option<u8>,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    let per = (limit / 4).max(5);
    let verses = search_verse_page(conn, compiled, filter, chip, None, per)?;
    let comments = search_commentary_page(conn, compiled, filter, chip, None, per)?;
    let dicts = search_entry_kind(conn, compiled, "dictionary", per)?;
    let topics = search_entry_kind(conn, compiled, "topic", per)?;
    let mut hits = verses.hits;
    hits.extend(comments.hits);
    hits.extend(dicts);
    hits.extend(topics);
    hits.truncate(limit);
    Ok(LibraryPage {
        hits,
        total: verses.total + comments.total,
        by_book: verses.by_book,
    })
}

fn coalesce_snippet(snippet: String, fallback: &str) -> String {
    if snippet.is_empty() {
        preview(fallback)
    } else {
        snippet
    }
}

fn preview(text: &str) -> String {
    const MAX: usize = 160;
    let mut chars = text.chars();
    let out: String = chars.by_ref().take(MAX).collect();
    if chars.next().is_some() {
        format!("{out}…")
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_memory;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES
                (1, 'Ge', 'Genesis'),
                (43, 'Joh', 'John'),
                (58, 'Heb', 'Hebrews');
            INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 1, 1, 'In the beginning God created the heaven and the earth.', 1),
                (43, 3, 16, 'For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.', 0),
                (43, 1, 14, 'And the Word was made flesh, and dwelt among us, (and we beheld his glory, the glory as of the only begotten of the Father,) full of grace and truth.', 0),
                (58, 11, 17, 'By faith Abraham, when he was tried, offered up Isaac: and he that had received the promises offered up his only begotten son,', 0);
            "#,
        )
        .unwrap();
        rebuild_verses_fts(conn).unwrap();
    }

    fn seed_library(conn: &Connection) {
        seed(conn);
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license) VALUES
                ('MHC', 'commentary', 'Matthew Henry''s Commentary on the Whole Bible', 'public-domain'),
                ('TSK', 'commentary', 'The Treasury of Scripture Knowledge', 'public-domain'),
                ('Easton', 'dictionary', 'Easton''s Bible Dictionary', 'public-domain'),
                ('Nave', 'topic', 'Nave''s Topical Bible', 'public-domain');
            INSERT INTO resources (module, book, chapter, verse, text) VALUES
                ('MHC', 1, 1, 1, 'The first verse declares that God created. Distinctive mhcword appears here.'),
                ('TSK', 1, 1, 1, '* beginning. Proverbs 8:22–24');
            INSERT INTO entries (module, i, headword, text) VALUES
                ('Easton', 0, 'God', 'the true and living God; distinctive eastonbodyterm'),
                ('Easton', 1, 'Water-god', 'a deity that presides over the water'),
                ('Nave', 0, 'Faith', 'confidence toward God; distinctive navebodyterm');
            "#,
        )
        .unwrap();
        rebuild_resources_fts(conn).unwrap();
        rebuild_entries_fts(conn).unwrap();
    }

    fn shipped_sqlite() -> Option<std::path::PathBuf> {
        std::env::var_os("BIBLE_APP_DB")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| {
                let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../data/bible-app.sqlite");
                p.is_file().then_some(p)
            })
    }

    #[test]
    fn phrase_only_begotten_includes_john_3_16() {
        let conn = open_memory().unwrap();
        seed(&conn);
        let hits = search_verses(&conn, "only begotten", 20).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 43 && h.chapter == 3 && h.verse == 16),
            "hits: {hits:?}"
        );
        assert!(hits
            .iter()
            .all(|h| h.text.to_lowercase().contains("only begotten")));
        let keys: Vec<_> = hits.iter().map(|h| (h.book, h.chapter, h.verse)).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "hits should be in book/chapter/verse order");
    }

    #[test]
    fn compile_keeps_phrase_and_lowercase_and() {
        let phrase = compile_query("only begotten", MatchMode::Phrase).unwrap();
        assert_eq!(phrase.fts, "\"only begotten\"");
        assert_eq!(phrase.tokens, ["only", "begotten"]);
        assert!(phrase.strongs.is_none());
        let with_and = compile_query("bread and wine", MatchMode::Phrase).unwrap();
        assert_eq!(with_and.fts, "\"bread and wine\"");
        let explicit = compile_query("faith AND works", MatchMode::Phrase).unwrap();
        assert_eq!(explicit.fts, "faith AND works");
        let any = compile_query("faith works", MatchMode::AnyWord).unwrap();
        assert_eq!(any.fts, "faith OR works");
        let all = compile_query("faith works", MatchMode::AllWords).unwrap();
        assert_eq!(all.fts, "faith AND works");
        let prefix = compile_query("lov*", MatchMode::Phrase).unwrap();
        assert_eq!(prefix.fts, "lov*");
        assert_eq!(prefix.tokens, ["lov"]);
        let possessive = compile_query("God's", MatchMode::Phrase).unwrap();
        assert_eq!(possessive.fts, "God");
        let code = compile_query("h430", MatchMode::Phrase).unwrap();
        assert_eq!(code.strongs.as_deref(), Some("H430"));
        assert!(compile_query("AND OR", MatchMode::Phrase).is_none());
        assert!(compile_query("   ", MatchMode::Phrase).is_none());
    }

    #[test]
    fn all_words_finds_apart_and_book_filter_counts() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute(
            "INSERT INTO verses (book, chapter, verse, text, para_break) VALUES (43, 2, 1, 'faith without works is dead', 0)",
            [],
        )
        .unwrap();
        rebuild_verses_fts(&conn).unwrap();
        let phrase = compile_query("faith works", MatchMode::Phrase).unwrap();
        let phrase_hits =
            search_verse_page(&conn, &phrase, VerseFilter::all(), None, None, 20).unwrap();
        assert!(phrase_hits.hits.is_empty(), "{phrase_hits:?}");
        let all = compile_query("faith works", MatchMode::AllWords).unwrap();
        let hits = search_verse_page(&conn, &all, VerseFilter::all(), None, None, 20).unwrap();
        assert_eq!(hits.total, 1);
        assert_eq!(hits.hits[0].book, Some(43));
        assert!(hits.by_book.iter().any(|b| b.book == 43 && b.count == 1));
        let john = VerseFilter {
            book: Some(43),
            ..VerseFilter::all()
        };
        let only = search_verse_page(&conn, &all, john, None, None, 20).unwrap();
        assert_eq!(only.total, 1);
        let genesis = VerseFilter {
            book: Some(1),
            ..VerseFilter::all()
        };
        assert_eq!(
            search_verse_page(&conn, &all, genesis, None, None, 20)
                .unwrap()
                .total,
            0
        );
        let snippet = &hits.hits[0].snippet;
        assert!(!snippet.contains('{'));
        assert!(snippet.to_lowercase().contains("faith"));
    }

    #[test]
    fn snippet_drops_trailing_translator_note() {
        let text = "And Moses said unto Joshua {Joshua: called Jesus}";
        let snippet = window_snippet(&super::strip_trailing_notes(text), &["Jesus".into()], 80);
        assert!(!snippet.contains('}'));
        assert!(snippet.contains("Joshua"));
    }

    #[test]
    fn empty_or_operator_only_query_is_empty() {
        let conn = open_memory().unwrap();
        seed(&conn);
        assert!(search_verses(&conn, "   ", 10).unwrap().is_empty());
        assert!(search_verses(&conn, "AND OR", 10).unwrap().is_empty());
        assert_eq!(
            match_query("only begotten").as_deref(),
            Some("\"only begotten\"")
        );
        assert_eq!(match_query("begotten").as_deref(), Some("begotten"));
    }

    #[test]
    fn real_sqlite_only_begotten_includes_john_3_16() {
        let Some(path) = shipped_sqlite() else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = crate::open(&path).unwrap();
        let hits = search_verses(&conn, "only begotten", 50).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 43 && h.chapter == 3 && h.verse == 16),
            "John 3:16 missing from {hits:?}"
        );
    }

    #[test]
    fn ensure_backfills_when_index_missing() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (43, 'Joh', 'John');
            INSERT INTO verses (book, chapter, verse, text, para_break)
            VALUES (43, 3, 16, 'his only begotten Son', 0);
            DELETE FROM verses_fts;
            "#,
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM verses_fts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        ensure_verses_fts(&conn).unwrap();
        let hits = search_verses(&conn, "only begotten", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].book, hits[0].chapter, hits[0].verse), (43, 3, 16));
    }

    #[test]
    fn commentary_search_finds_mhc_and_tsk() {
        let conn = open_memory().unwrap();
        seed_library(&conn);
        let hits = search_library(&conn, "beginning", SearchScope::Commentary, 20).unwrap();
        assert!(
            hits.iter().any(|h| h.module == "TSK"
                && h.book == Some(1)
                && h.chapter == Some(1)
                && h.verse == Some(1)),
            "TSK missing from {hits:?}"
        );
        assert!(hits.iter().all(|h| h.kind == LibraryKind::Commentary));
        assert!(hits.iter().all(|h| !h.snippet.is_empty()));
        let mhc = search_library(&conn, "mhcword", SearchScope::Commentary, 20).unwrap();
        assert_eq!(mhc.len(), 1);
        assert_eq!(mhc[0].module, "MHC");
        assert!(!mhc[0].snippet.is_empty());
        assert!(search_library(&conn, "   ", SearchScope::Commentary, 10)
            .unwrap()
            .is_empty());
        assert!(search_library(&conn, "AND OR", SearchScope::Commentary, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn dictionaries_search_headword_and_body() {
        let conn = open_memory().unwrap();
        seed_library(&conn);
        let by_head = search_library(&conn, "God", SearchScope::Dictionaries, 20).unwrap();
        assert!(
            by_head.iter().any(|h| h.module == "Easton"
                && h.headword.as_deref() == Some("God")
                && h.kind == LibraryKind::Dictionary),
            "Easton God missing from {by_head:?}"
        );
        assert_eq!(
            by_head[0].headword.as_deref(),
            Some("God"),
            "exact headword should rank first: {by_head:?}"
        );
        assert!(by_head.iter().all(|h| !h.snippet.is_empty()));
        let by_body =
            search_library(&conn, "eastonbodyterm", SearchScope::Dictionaries, 20).unwrap();
        assert_eq!(by_body.len(), 1);
        assert_eq!(by_body[0].headword.as_deref(), Some("God"));
        assert!(!by_body[0].snippet.is_empty());
    }

    #[test]
    fn topics_search_finds_nave() {
        let conn = open_memory().unwrap();
        seed_library(&conn);
        let hits = search_library(&conn, "Faith", SearchScope::Topics, 20).unwrap();
        assert!(
            hits.iter().any(|h| h.module == "Nave"
                && h.headword.as_deref() == Some("Faith")
                && h.kind == LibraryKind::Topic),
            "Nave Faith missing from {hits:?}"
        );
        let by_body = search_library(&conn, "navebodyterm", SearchScope::Topics, 20).unwrap();
        assert_eq!(by_body.len(), 1);
        assert_eq!(by_body[0].module, "Nave");
        assert!(!by_body[0].snippet.is_empty());
        assert!(search_library(&conn, "AND OR", SearchScope::Topics, 10)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn all_scope_labels_each_source() {
        let conn = open_memory().unwrap();
        seed_library(&conn);
        let hits = search_library(&conn, "God", SearchScope::All, 20).unwrap();
        let modules: Vec<&str> = hits.iter().map(|h| h.module.as_str()).collect();
        assert!(modules.contains(&"KJV"), "{modules:?}");
        assert!(modules.contains(&"MHC"), "{modules:?}");
        assert!(modules.contains(&"Easton"), "{modules:?}");
        assert!(modules.contains(&"Nave"), "{modules:?}");
        assert!(hits.iter().any(|h| h.kind == LibraryKind::Verse));
        assert!(hits.iter().any(|h| h.kind == LibraryKind::Commentary));
        assert!(hits.iter().any(|h| h.kind == LibraryKind::Dictionary));
        assert!(hits.iter().any(|h| h.kind == LibraryKind::Topic));
    }

    #[test]
    fn library_kjv_keeps_phrase_search() {
        let conn = open_memory().unwrap();
        seed_library(&conn);
        let hits = search_library(&conn, "only begotten", SearchScope::Kjv, 20).unwrap();
        assert!(hits
            .iter()
            .any(|h| h.book == Some(43) && h.chapter == Some(3) && h.verse == Some(16)));
        assert!(hits.iter().all(|h| h.module == "KJV"));
    }

    #[test]
    fn ensure_resources_fts_backfills_when_index_missing() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis');
            INSERT INTO modules (id, kind, title, license)
            VALUES ('MHC', 'commentary', 'Matthew Henry', 'public-domain');
            INSERT INTO resources (module, book, chapter, verse, text)
            VALUES ('MHC', 1, 1, 1, 'comment containing mhcbackfill');
            DELETE FROM resources_fts;
            "#,
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM resources_fts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        ensure_resources_fts(&conn).unwrap();
        let hits = search_library(&conn, "mhcbackfill", SearchScope::Commentary, 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].module, "MHC");
        assert_eq!(
            (hits[0].book, hits[0].chapter, hits[0].verse),
            (Some(1), Some(1), Some(1))
        );
        assert!(!hits[0].snippet.is_empty());
    }

    #[test]
    fn ensure_entries_fts_backfills_when_index_missing() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license)
            VALUES ('Easton', 'dictionary', 'Easton''s Bible Dictionary', 'public-domain');
            INSERT INTO entries (module, i, headword, text)
            VALUES ('Easton', 0, 'God', 'the true God eastonbackfill');
            DELETE FROM entries_fts;
            "#,
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM entries_fts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        ensure_entries_fts(&conn).unwrap();
        let hits = search_library(&conn, "eastonbackfill", SearchScope::Dictionaries, 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].headword.as_deref(), Some("God"));
        assert!(!hits[0].snippet.is_empty());
    }

    #[test]
    fn real_sqlite_library_search() {
        let Some(path) = shipped_sqlite() else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = crate::open(&path).unwrap();
        let comments = search_library(&conn, "beginning", SearchScope::Commentary, 20).unwrap();
        assert!(
            comments
                .iter()
                .any(|h| h.module == "MHC" || h.module == "TSK"),
            "commentary missing from {comments:?}"
        );
        assert!(comments.iter().all(|h| !h.snippet.is_empty()));
        let dicts = search_library(&conn, "God", SearchScope::Dictionaries, 50).unwrap();
        assert!(
            dicts.iter().any(|h| h.module == "Easton"
                && h.headword
                    .as_deref()
                    .is_some_and(|w| w.eq_ignore_ascii_case("God"))),
            "Easton God missing from {dicts:?}"
        );
        let topics = search_library(&conn, "Faith", SearchScope::Topics, 20).unwrap();
        assert!(
            topics.iter().any(|h| h.module == "Nave"),
            "Nave missing from {topics:?}"
        );
    }
}
