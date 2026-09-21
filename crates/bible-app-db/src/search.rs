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
    All,
}

impl SearchScope {
    pub const ALL: [SearchScope; 5] = [
        SearchScope::Kjv,
        SearchScope::Commentary,
        SearchScope::Dictionaries,
        SearchScope::Topics,
        SearchScope::All,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Kjv => "KJV",
            Self::Commentary => "Commentary",
            Self::Dictionaries => "Dictionaries",
            Self::Topics => "Topics",
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

/// Turn typed text into an FTS5 MATCH query.
///
/// Multi-word input becomes a phrase so "only begotten" matches that pair,
/// not any verse that happens to contain both words.
pub fn match_query(input: &str) -> Option<String> {
    let tokens: Vec<&str> = input
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '\''))
        .filter(|t| !t.is_empty())
        .filter(|t| {
            !matches!(
                t.to_ascii_uppercase().as_str(),
                "AND" | "OR" | "NOT" | "NEAR"
            )
        })
        .collect();
    if tokens.is_empty() {
        return None;
    }
    if tokens.len() == 1 {
        Some(tokens[0].to_string())
    } else {
        Some(format!("\"{}\"", tokens.join(" ")))
    }
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
    let Some(match_q) = match_query(query) else {
        return Ok(Vec::new());
    };
    let limit = limit.max(1) as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT book, chapter, verse, text,
               snippet(verses_fts, 0, '', '', '…', 16)
        FROM verses_fts
        WHERE verses_fts MATCH ?1
        ORDER BY book, chapter, verse
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(rusqlite::params![match_q, limit], |row| {
        let text: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let snippet = if snippet.is_empty() {
            text.clone()
        } else {
            snippet
        };
        Ok(SearchHit {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            text,
            snippet,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn search_library(
    conn: &Connection,
    query: &str,
    scope: SearchScope,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let limit = limit.max(1);
    match scope {
        SearchScope::Kjv => search_library_verses(conn, query, limit),
        SearchScope::Commentary => search_commentary(conn, query, limit),
        SearchScope::Dictionaries => search_entry_kind(conn, query, "dictionary", limit),
        SearchScope::Topics => search_entry_kind(conn, query, "topic", limit),
        SearchScope::All => search_all(conn, query, limit),
    }
}

fn search_library_verses(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    Ok(search_verses(conn, query, limit)?
        .into_iter()
        .map(LibraryHit::from_verse)
        .collect())
}

fn search_commentary(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let Some(match_q) = match_query(query) else {
        return Ok(Vec::new());
    };
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
        ORDER BY resources_fts.book, resources_fts.chapter, resources_fts.verse,
                 resources_fts.module
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(rusqlite::params![match_q, limit], |row| {
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
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn search_entry_kind(
    conn: &Connection,
    query: &str,
    kind: &str,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let Some(match_q) = match_query(query) else {
        return Ok(Vec::new());
    };
    let library_kind = match kind {
        "topic" => LibraryKind::Topic,
        "lexicon" => LibraryKind::Lexicon,
        _ => LibraryKind::Dictionary,
    };
    let limit = limit.max(1) as i64;
    let exact = query.trim();
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
    let rows = stmt.query_map(rusqlite::params![match_q, kind, limit, exact], |row| {
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

fn search_all(conn: &Connection, query: &str, limit: usize) -> Result<Vec<LibraryHit>, DbError> {
    if match_query(query).is_none() {
        return Ok(Vec::new());
    }
    let groups = vec![
        search_library_verses(conn, query, limit)?,
        search_commentary(conn, query, limit)?,
        search_entry_kind(conn, query, "dictionary", limit)?,
        search_entry_kind(conn, query, "topic", limit)?,
        search_entry_kind(conn, query, "lexicon", limit)?,
    ];
    Ok(take_round_robin(groups, limit))
}

fn take_round_robin(groups: Vec<Vec<LibraryHit>>, limit: usize) -> Vec<LibraryHit> {
    let mut iters: Vec<_> = groups.into_iter().map(|g| g.into_iter()).collect();
    let mut out = Vec::new();
    while out.len() < limit {
        let mut added = false;
        for iter in &mut iters {
            if let Some(hit) = iter.next() {
                out.push(hit);
                added = true;
                if out.len() >= limit {
                    break;
                }
            }
        }
        if !added {
            break;
        }
    }
    out
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
