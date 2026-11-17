use rusqlite::{functions::FunctionFlags, params_from_iter, Connection, OptionalExtension};
use std::collections::HashMap;
use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

use crate::DbError;

pub const DEFAULT_LIMIT: usize = 200;

/// Tokens this long or longer become FTS5 prefixes, so `eart` matches earth.
pub const SEARCH_PREFIX_MIN: usize = 3;

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
    ExactWord,
}

impl MatchMode {
    pub const ALL: [MatchMode; 4] = [Self::Phrase, Self::AllWords, Self::AnyWord, Self::ExactWord];

    pub fn label(self) -> &'static str {
        match self {
            Self::Phrase => "Contains",
            Self::AllWords => "All words",
            Self::AnyWord => "Any word",
            Self::ExactWord => "Exact word",
        }
    }

    pub fn word_match(self) -> WordMatch {
        match self {
            Self::ExactWord => WordMatch::Exact,
            _ => WordMatch::Prefix,
        }
    }

    fn combine(self) -> Self {
        match self {
            Self::ExactWord => Self::Phrase,
            other => other,
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(Self::Phrase)
    }

    /// Restore the match menu from state.toml. An older Exact word setting
    /// (`search_words = 1`) becomes Exact word in this menu.
    pub fn from_saved(mode: u32, words: u32) -> Self {
        if words == 1 {
            Self::ExactWord
        } else {
            Self::from_index(mode)
        }
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|&m| m == self).unwrap_or(0) as u32
    }
}

/// How each token matches a word in the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordMatch {
    #[default]
    Prefix,
    Exact,
}

impl WordMatch {
    /// Prefix mode matches the token anywhere inside a word (`ear` finds hear).
    pub fn partial(self) -> bool {
        self == Self::Prefix
    }
}

/// Words to emphasize, plus the FTS5 expression that finds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledQuery {
    pub fts: String,
    /// Substring predicate with `{c}` for the text column. Exact word and
    /// explicit operators keep FTS MATCH.
    pub substring: Option<String>,
    pub tokens: Vec<String>,
    /// Set when the whole query is a Strong's code such as `H430`.
    pub strongs: Option<String>,
    pub match_case: bool,
    case_matcher: Option<CaseExpr>,
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

/// Each source advances independently because library rows have different
/// ordering keys (and several sources may refer to the same verse).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SearchCursor {
    pub verses: usize,
    pub commentary: usize,
    pub dictionaries: usize,
    pub topics: usize,
    pub notes: usize,
    pub strongs_after: Option<(u8, u8, u8)>,
    pub round: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryPage {
    pub hits: Vec<LibraryHit>,
    pub total: i64,
    pub by_book: Vec<BookCount>,
    pub next: SearchCursor,
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
/// Contains, all words, and any word match a token anywhere in a word (`ear`
/// finds earth and hear). Exact word keeps whole tokens. A trailing `*` still
/// prefixes a shorter stem in Exact word. Apostrophes break tokens, so "God's"
/// searches God.
pub fn compile_query(input: &str, mode: MatchMode) -> Option<CompiledQuery> {
    compile_query_with_case(input, mode, false)
}

/// Compile a query, optionally preserving case in text and Strong's matches.
pub fn compile_query_with_case(
    input: &str,
    mode: MatchMode,
    match_case: bool,
) -> Option<CompiledQuery> {
    let word_match = mode.word_match();
    let mode = mode.combine();
    let trimmed = input.trim();
    if let Some(code) = strongs_query(trimmed) {
        let code = if match_case {
            format!("{}{}", trimmed.chars().next()?, &code[1..])
        } else {
            code
        };
        return Some(CompiledQuery {
            fts: String::new(),
            substring: None,
            tokens: Vec::new(),
            strongs: Some(code),
            match_case,
            case_matcher: None,
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
        explicit_fts(&pieces, word_match)
    } else {
        mode_fts(&pieces, mode, word_match)
    };
    let mut fts = fts?;
    let tokens: Vec<String> = pieces
        .iter()
        .filter_map(|p| match p {
            Piece::Word { text, .. } => Some(text.clone()),
            Piece::Op(_) => None,
        })
        .collect();
    let substring = if word_match.partial() && !explicit {
        substring_pred(&tokens, mode, match_case)
    } else {
        None
    };
    let case_matcher = if match_case && substring.is_none() {
        let matcher = case_expression(&pieces, mode, word_match, explicit)?;
        // Case-insensitive NOT can discard a valid case-sensitive match. Keep
        // the positive side as the FTS candidate set, then check the full
        // expression before counts and pagination.
        if explicit
            && pieces
                .iter()
                .any(|p| matches!(p, Piece::Op(op) if op == "NOT"))
        {
            fts = matcher.positive_fts();
        }
        Some(matcher)
    } else {
        None
    };
    Some(CompiledQuery {
        fts,
        substring,
        tokens,
        strongs: None,
        match_case,
        case_matcher,
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

#[derive(Debug, Clone)]
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
            .filter(|c| c.is_alphanumeric() || is_combining_mark(*c))
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

fn word_fts(text: &str, prefix: bool, word_match: WordMatch) -> String {
    if prefix || (word_match.partial() && is_prefix_token(text)) {
        format!("{text}*")
    } else {
        text.to_string()
    }
}

fn substring_pred(tokens: &[String], mode: MatchMode, match_case: bool) -> Option<String> {
    if tokens.is_empty()
        || tokens.iter().any(|t| {
            t.is_empty()
                || !t
                    .chars()
                    .all(|c| c.is_alphanumeric() || is_combining_mark(c))
        })
    {
        return None;
    }
    let like = |t: &str| {
        if match_case {
            format!("instr({{c}}, '{t}') > 0")
        } else {
            format!("{{c}} LIKE '%{t}%' COLLATE NOCASE")
        }
    };
    Some(match mode {
        MatchMode::Phrase | MatchMode::ExactWord => like(&tokens.join(" ")),
        MatchMode::AllWords => tokens
            .iter()
            .map(|t| like(t))
            .collect::<Vec<_>>()
            .join(" AND "),
        MatchMode::AnyWord => format!(
            "({})",
            tokens
                .iter()
                .map(|t| like(t))
                .collect::<Vec<_>>()
                .join(" OR ")
        ),
    })
}

pub fn text_pred(compiled: &CompiledQuery, fts_table: &str, text_col: &str) -> String {
    let pred = match &compiled.substring {
        Some(sql) => sql.replace("{c}", text_col),
        None => format!("{fts_table} MATCH ?1"),
    };
    if compiled.case_matcher.is_some() {
        format!("({pred}) AND search_match_case('{fts_table}', {fts_table}.rowid, {text_col})")
    } else {
        pred
    }
}

fn entry_pred(compiled: &CompiledQuery) -> String {
    let pred = match &compiled.substring {
        Some(sql) => {
            let text = sql.replace("{c}", "entries_fts.text");
            let head = sql.replace("{c}", "entries_fts.headword");
            format!("({text} OR {head})")
        }
        None => "entries_fts MATCH ?1".into(),
    };
    if compiled.case_matcher.is_some() {
        format!("({pred}) AND search_match_case_entry(entries_fts.rowid, entries_fts.headword, entries_fts.text)")
    } else {
        pred
    }
}

fn bind_params<'a>(
    compiled: &'a CompiledQuery,
    rest: &'a [&'a dyn rusqlite::ToSql],
) -> Vec<&'a dyn rusqlite::ToSql> {
    let mut out = Vec::with_capacity(rest.len() + 1);
    if compiled.substring.is_none() {
        out.push(&compiled.fts as &dyn rusqlite::ToSql);
    }
    out.extend_from_slice(rest);
    out
}

fn param_base(compiled: &CompiledQuery) -> i32 {
    if compiled.substring.is_some() {
        0
    } else {
        1
    }
}

pub fn is_prefix_token(token: &str) -> bool {
    token.len() >= SEARCH_PREFIX_MIN
}

fn mode_fts(pieces: &[Piece], mode: MatchMode, word_match: WordMatch) -> Option<String> {
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
        let parts: Vec<&str> = words
            .iter()
            .filter_map(|p| match p {
                Piece::Word { text, .. } => Some(text.as_str()),
                Piece::Op(_) => None,
            })
            .collect();
        let inner = parts.join(" ");
        if parts
            .last()
            .is_some_and(|last| word_match.partial() && is_prefix_token(last))
        {
            return Some(format!("\"{inner}\"*"));
        }
        return Some(format!("\"{inner}\""));
    }
    let joiner = match mode {
        MatchMode::AnyWord => " OR ",
        MatchMode::Phrase | MatchMode::AllWords | MatchMode::ExactWord => " AND ",
    };
    Some(
        words
            .iter()
            .filter_map(|p| match p {
                Piece::Word { text, prefix } => Some(word_fts(text, *prefix, word_match)),
                Piece::Op(_) => None,
            })
            .collect::<Vec<_>>()
            .join(joiner),
    )
}

fn explicit_tokens(pieces: &[Piece], word_match: WordMatch) -> Vec<String> {
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
                out.push(word_fts(text, *prefix, word_match));
            }
        }
    }
    while out.last().is_some_and(|p| is_op(p)) {
        out.pop();
    }
    if out.iter().all(|p| is_op(p)) || out.is_empty() {
        return Vec::new();
    }
    out
}

fn explicit_fts(pieces: &[Piece], word_match: WordMatch) -> Option<String> {
    let tokens = explicit_tokens(pieces, word_match);
    let mut parser = CaseParser {
        tokens: &tokens,
        pos: 0,
    };
    let expression = parser.parse_or()?;
    (parser.pos == tokens.len()).then(|| expression.fts(0, false))
}

fn is_op(token: &str) -> bool {
    token == "AND"
        || token == "OR"
        || token == "NOT"
        || token == "NEAR"
        || token.starts_with("NEAR/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CaseTerm {
    text: String,
    prefix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaseExpr {
    Term(CaseTerm),
    Phrase(Vec<CaseTerm>),
    Near(CaseTerm, CaseTerm, usize),
    And(Box<CaseExpr>, Box<CaseExpr>),
    Or(Box<CaseExpr>, Box<CaseExpr>),
    Not(Box<CaseExpr>, Box<CaseExpr>),
}

impl CaseTerm {
    fn new(text: &str, prefix: bool) -> Self {
        Self {
            text: text.nfd().filter(|c| !is_combining_mark(*c)).collect(),
            prefix,
        }
    }

    fn matches(&self, word: &str) -> bool {
        if word.is_ascii() && self.text.is_ascii() {
            return if self.prefix {
                word.starts_with(&self.text)
            } else {
                word == self.text
            };
        }
        let mut normalized = word.nfd().filter(|c| !is_combining_mark(*c));
        for expected in self.text.chars() {
            if normalized.next() != Some(expected) {
                return false;
            }
        }
        self.prefix || normalized.next().is_none()
    }

    fn occurs_in(&self, text: &str) -> bool {
        // The common ASCII path uses the standard library's optimized
        // substring search instead of tokenizing entire commentary articles.
        for (pos, _) in text.match_indices(&self.text) {
            let before = text[..pos].chars().next_back();
            let after = text[pos + self.text.len()..].chars().next();
            if before.is_none_or(|c| !is_word_char(c))
                && (self.prefix || after.is_none_or(|c| !is_word_char(c)))
            {
                return true;
            }
        }
        // FTS unicode61 ignores diacritics. Skip ASCII runs directly rather
        // than tokenizing long articles just to inspect their accented words.
        !text.is_ascii() && self.accented_word_occurs(text)
    }

    fn accented_word_occurs(&self, text: &str) -> bool {
        let mut from = 0;
        while let Some(relative) = text.as_bytes()[from..]
            .iter()
            .position(|byte| !byte.is_ascii())
        {
            let position = from + relative;
            let ch = text[position..].chars().next().expect("UTF-8 boundary");
            if !is_word_char(ch) {
                from = position + ch.len_utf8();
                continue;
            }
            let start = text[..position]
                .char_indices()
                .rev()
                .find(|(_, ch)| !is_word_char(*ch))
                .map_or(0, |(index, ch)| index + ch.len_utf8());
            let end = text[position..]
                .char_indices()
                .find(|(_, ch)| !is_word_char(*ch))
                .map_or(text.len(), |(index, _)| position + index);
            if self.matches(&text[start..end]) {
                return true;
            }
            from = end;
        }
        false
    }

    fn fts(&self) -> String {
        if self.prefix {
            format!("{}*", self.text)
        } else {
            self.text.clone()
        }
    }
}

impl CaseExpr {
    fn matches(&self, columns: &[&str]) -> bool {
        match self {
            Self::Term(term) => columns.iter().any(|column| term.occurs_in(column)),
            Self::Phrase(phrase) => columns.iter().any(|column| {
                let mut words = case_words(column);
                while let Some(word) = words.next() {
                    let mut rest = words.clone();
                    if phrase[0].matches(word)
                        && phrase[1..]
                            .iter()
                            .all(|term| rest.next().is_some_and(|word| term.matches(word)))
                    {
                        return true;
                    }
                }
                false
            }),
            Self::Near(left, right, distance) => columns.iter().any(|column| {
                let mut left_at = None;
                let mut right_at = None;
                for (index, word) in case_words(column).enumerate() {
                    let left_match = left.matches(word);
                    let right_match = right.matches(word);
                    if left_match && right_match {
                        return true;
                    }
                    if left_match && right_at.is_some_and(|at| index - at - 1 <= *distance) {
                        return true;
                    }
                    if right_match && left_at.is_some_and(|at| index - at - 1 <= *distance) {
                        return true;
                    }
                    if left_match {
                        left_at = Some(index);
                    }
                    if right_match {
                        right_at = Some(index);
                    }
                }
                false
            }),
            Self::And(left, right) => left.matches(columns) && right.matches(columns),
            Self::Or(left, right) => left.matches(columns) || right.matches(columns),
            Self::Not(left, right) => left.matches(columns) && !right.matches(columns),
        }
    }

    fn precedence(&self) -> u8 {
        match self {
            Self::Or(..) => 1,
            Self::And(..) => 2,
            Self::Not(..) => 3,
            _ => 4,
        }
    }

    fn fts(&self, parent: u8, positive_only: bool) -> String {
        if positive_only {
            if let Self::Not(left, _) = self {
                return left.fts(parent, true);
            }
        }
        let precedence = self.precedence();
        let body = match self {
            Self::Term(term) => term.fts(),
            Self::Phrase(terms) => format!(
                "\"{}\"",
                terms
                    .iter()
                    .map(|term| term.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            Self::Near(left, right, distance) => {
                format!("NEAR({} {}, {distance})", left.fts(), right.fts())
            }
            Self::And(left, right) | Self::Or(left, right) | Self::Not(left, right) => {
                let op = match self {
                    Self::And(..) => "AND",
                    Self::Or(..) => "OR",
                    Self::Not(..) => "NOT",
                    _ => unreachable!(),
                };
                format!(
                    "{} {op} {}",
                    left.fts(precedence, positive_only),
                    right.fts(precedence + 1, positive_only)
                )
            }
        };
        if precedence < parent {
            format!("({body})")
        } else {
            body
        }
    }

    fn positive_fts(&self) -> String {
        self.fts(0, true)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_combining_mark(c)
}

fn case_words(text: &str) -> impl Iterator<Item = &str> + Clone {
    text.split(|c: char| !is_word_char(c))
        .filter(|word| !word.is_empty())
}

/// Byte ranges of case-sensitive matches, ignoring diacritics like unicode61.
/// Used for rendering; the search predicate uses the faster streaming matcher.
pub fn case_match_ranges(text: &str, token: &str, partial: bool) -> Vec<(usize, usize)> {
    let needle: String = token.nfd().filter(|c| !is_combining_mark(*c)).collect();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut normalized = String::new();
    let mut offsets: Vec<(usize, usize)> = Vec::new();
    for (start, ch) in text.char_indices() {
        let end = start + ch.len_utf8();
        if is_combining_mark(ch) {
            if let Some(last) = offsets.last_mut() {
                last.1 = end;
            }
            continue;
        }
        let mut bytes = [0; 4];
        for base in ch
            .encode_utf8(&mut bytes)
            .nfd()
            .filter(|c| !is_combining_mark(*c))
        {
            normalized.push(base);
            offsets.extend(std::iter::repeat_n((start, end), base.len_utf8()));
        }
    }
    normalized
        .match_indices(&needle)
        .filter_map(|(start, _)| {
            let end = start + needle.len();
            let before = normalized[..start].chars().next_back();
            let after = normalized[end..].chars().next();
            if !partial && (before.is_some_and(is_word_char) || after.is_some_and(is_word_char)) {
                return None;
            }
            Some((offsets[start].0, offsets[end - 1].1))
        })
        .collect()
}

fn case_expression(
    pieces: &[Piece],
    mode: MatchMode,
    word_match: WordMatch,
    explicit: bool,
) -> Option<CaseExpr> {
    if explicit {
        let tokens = explicit_tokens(pieces, word_match);
        let mut parser = CaseParser {
            tokens: &tokens,
            pos: 0,
        };
        let expr = parser.parse_or()?;
        return (parser.pos == parser.tokens.len()).then_some(expr);
    }
    let terms: Vec<CaseTerm> = pieces
        .iter()
        .filter_map(|piece| match piece {
            Piece::Word { text, prefix } => Some(CaseTerm::new(
                text,
                *prefix || (word_match.partial() && is_prefix_token(text)),
            )),
            Piece::Op(_) => None,
        })
        .collect();
    if mode == MatchMode::Phrase && terms.len() > 1 && !terms.iter().any(|term| term.prefix) {
        return Some(CaseExpr::Phrase(terms));
    }
    let mut terms = terms.into_iter().map(CaseExpr::Term);
    let first = terms.next()?;
    Some(terms.fold(first, |left, right| {
        CaseExpr::And(Box::new(left), Box::new(right))
    }))
}

struct CaseParser<'a> {
    tokens: &'a [String],
    pos: usize,
}

impl CaseParser<'_> {
    fn take(&mut self, token: &str) -> bool {
        if self
            .tokens
            .get(self.pos)
            .is_some_and(|value| value == token)
        {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn parse_or(&mut self) -> Option<CaseExpr> {
        let mut left = self.parse_and()?;
        while self.take("OR") {
            left = CaseExpr::Or(Box::new(left), Box::new(self.parse_and()?));
        }
        Some(left)
    }

    fn parse_and(&mut self) -> Option<CaseExpr> {
        let mut left = self.parse_not()?;
        while self.take("AND") {
            left = CaseExpr::And(Box::new(left), Box::new(self.parse_not()?));
        }
        Some(left)
    }

    fn parse_not(&mut self) -> Option<CaseExpr> {
        let mut left = self.parse_near()?;
        while self.take("NOT") {
            left = CaseExpr::Not(Box::new(left), Box::new(self.parse_near()?));
        }
        Some(left)
    }

    fn parse_near(&mut self) -> Option<CaseExpr> {
        let CaseExpr::Term(mut previous) = self.parse_term()? else {
            return None;
        };
        let mut result = None;
        while let Some(op) = self.tokens.get(self.pos) {
            let distance = if op == "NEAR" {
                10
            } else if let Some(value) = op.strip_prefix("NEAR/") {
                value.parse().ok()?
            } else {
                break;
            };
            self.pos += 1;
            let CaseExpr::Term(next) = self.parse_term()? else {
                return None;
            };
            let pair = CaseExpr::Near(previous, next.clone(), distance);
            result = Some(match result {
                Some(left) => CaseExpr::And(Box::new(left), Box::new(pair)),
                None => pair,
            });
            previous = next;
        }
        Some(result.unwrap_or(CaseExpr::Term(previous)))
    }

    fn parse_term(&mut self) -> Option<CaseExpr> {
        let token = self.tokens.get(self.pos)?;
        if is_op(token) {
            return None;
        }
        self.pos += 1;
        Some(CaseExpr::Term(CaseTerm::new(
            token.trim_end_matches('*'),
            token.ends_with('*'),
        )))
    }
}

/// Register per-query case checks on a connection before executing its SQL.
pub fn register_case_matcher(conn: &Connection, compiled: &CompiledQuery) -> rusqlite::Result<()> {
    let Some(matcher) = compiled.case_matcher.clone() else {
        return Ok(());
    };
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    let entry_matcher = matcher.clone();
    let mut text_cache: HashMap<String, HashMap<i64, bool>> = HashMap::new();
    let mut entry_cache = HashMap::new();
    conn.create_scalar_function("search_match_case", 3, flags, move |ctx| {
        let source = ctx.get_raw(0).as_str()?;
        let rowid = ctx.get::<i64>(1)?;
        let text = ctx.get_raw(2).as_str()?;
        if !text_cache.contains_key(source) {
            text_cache.insert(source.to_string(), HashMap::new());
        }
        let rows = text_cache.get_mut(source).expect("source was inserted");
        Ok(cached_case_match(rows, rowid, || matcher.matches(&[text])))
    })?;
    conn.create_scalar_function("search_match_case_entry", 3, flags, move |ctx| {
        let rowid = ctx.get::<i64>(0)?;
        let headword = ctx.get_raw(1).as_str()?;
        let text = ctx.get_raw(2).as_str()?;
        Ok(cached_case_match(&mut entry_cache, rowid, || {
            entry_matcher.matches(&[headword, text])
        }))
    })?;
    Ok(())
}

fn cached_case_match(
    cache: &mut HashMap<i64, bool>,
    rowid: i64,
    evaluate: impl FnOnce() -> bool,
) -> bool {
    *cache.entry(rowid).or_insert_with(evaluate)
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
        "#,
    )?;
    let verses: Vec<(String, i64, i64, i64)> = {
        let mut sel = conn.prepare("SELECT text, book, chapter, verse FROM verses")?;
        let rows = sel.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    conn.execute("BEGIN IMMEDIATE", [])?;
    let result = (|| -> Result<(), DbError> {
        conn.execute("DELETE FROM verses_fts", [])?;
        let mut ins = conn.prepare(
            "INSERT INTO verses_fts (text, book, chapter, verse) VALUES (?1, ?2, ?3, ?4)",
        )?;
        for (text, book, chapter, verse) in verses {
            ins.execute(rusqlite::params![
                strip_trailing_notes(&text),
                book,
                chapter,
                verse
            ])?;
        }
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('verses_fts_plain', '1')",
            [],
        )?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            conn.execute("COMMIT", [])?;
            Ok(())
        }
        Err(err) => {
            let _ = conn.execute("ROLLBACK", []);
            Err(err)
        }
    }
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
    let plain = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'verses_fts_plain'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if verses != indexed || plain.as_deref() != Some("1") {
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
    verse_hits(conn, &compiled, VerseFilter::all(), None, 0, limit)
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
    search_verse_page_offset(
        conn,
        compiled,
        filter,
        chip,
        after,
        SearchCursor::default(),
        limit,
    )
}

fn search_verse_page_offset(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    chip: Option<u8>,
    after: Option<(u8, u8, u8)>,
    mut cursor: SearchCursor,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    register_case_matcher(conn, compiled)?;
    if compiled.strongs.is_some() || compiled.fts.is_empty() {
        return Ok(LibraryPage {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
            next: cursor,
        });
    }
    let by_book = verse_book_counts(conn, compiled, filter)?;
    let mut narrowed = filter;
    if let Some(book) = chip {
        narrowed.book = Some(book);
    }
    let total = filtered_total(&by_book, narrowed.book);
    let hits = if limit == 0 || cursor.verses as i64 >= total {
        Vec::new()
    } else {
        verse_hits(conn, compiled, narrowed, after, cursor.verses, limit)?
    };
    cursor.verses += hits.len();
    Ok(LibraryPage {
        hits: hits.into_iter().map(LibraryHit::from_verse).collect(),
        total,
        by_book,
        next: cursor,
    })
}

fn filtered_total(counts: &[BookCount], book: Option<u8>) -> i64 {
    match book {
        Some(book) => counts
            .iter()
            .find(|count| count.book == book)
            .map_or(0, |count| count.count),
        None => counts.iter().map(|count| count.count).sum(),
    }
}

fn verse_hits(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    after: Option<(u8, u8, u8)>,
    offset: usize,
    limit: usize,
) -> Result<Vec<SearchHit>, DbError> {
    let (book, chapter, book_min, book_max) = filter_params(filter);
    let (after_book, after_chapter, after_verse) = after_params(after);
    let o = param_base(compiled);
    let pred = text_pred(compiled, "verses_fts", "text");
    let limit = limit.max(1) as i64;
    let offset = offset as i64;
    let list_sql = format!(
        "
        SELECT book, chapter, verse, text
        FROM verses_fts
        WHERE {pred}
          AND book BETWEEN ?{bmin} AND ?{bmax}
          AND (?{book} = 0 OR book = ?{book})
          AND (?{chapter} = 0 OR chapter = ?{chapter})
          AND (
            ?{after_book} = 0
            OR book > ?{after_book}
            OR (book = ?{after_book} AND chapter > ?{after_chapter})
            OR (book = ?{after_book} AND chapter = ?{after_chapter} AND verse > ?{after_verse})
          )
        ORDER BY book, chapter, verse
        LIMIT ?{limit} OFFSET ?{offset}
        ",
        bmin = o + 1,
        bmax = o + 2,
        book = o + 3,
        chapter = o + 4,
        after_book = o + 5,
        after_chapter = o + 6,
        after_verse = o + 7,
        limit = o + 8,
        offset = o + 9,
    );
    let mut stmt = conn.prepare(&list_sql)?;
    let rest_list: [&dyn rusqlite::ToSql; 9] = [
        &book_min,
        &book_max,
        &book,
        &chapter,
        &after_book,
        &after_chapter,
        &after_verse,
        &limit,
        &offset,
    ];
    let rows = stmt.query_map(params_from_iter(bind_params(compiled, &rest_list)), |row| {
        let text: String = row.get(3)?;
        Ok(SearchHit {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            snippet: plain_line(&strip_trailing_notes(&text)),
            text,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn verse_book_counts(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
) -> Result<Vec<BookCount>, DbError> {
    let chapter = i64::from(filter.chapter.unwrap_or(0));
    let book_min = i64::from(filter.book_min);
    let book_max = i64::from(filter.book_max);
    let o = param_base(compiled);
    let pred = text_pred(compiled, "verses_fts", "text");
    let sql = format!(
        "
        SELECT book, count(*)
        FROM verses_fts
        WHERE {pred}
          AND book BETWEEN ?{bmin} AND ?{bmax}
          AND (?{chapter} = 0 OR chapter = ?{chapter})
        GROUP BY book
        ORDER BY book
        ",
        bmin = o + 1,
        bmax = o + 2,
        chapter = o + 3,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rest: [&dyn rusqlite::ToSql; 3] = [&book_min, &book_max, &chapter];
    let rows = stmt.query_map(params_from_iter(bind_params(compiled, &rest)), |row| {
        Ok(BookCount {
            book: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
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
    window_snippet_with_case(text, tokens, width, false)
}

fn window_snippet_with_case(
    text: &str,
    tokens: &[String],
    width: usize,
    match_case: bool,
) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.is_empty() {
        return String::new();
    }
    let haystack = if match_case {
        flat.clone()
    } else {
        flat.to_lowercase()
    };
    let mut at = 0usize;
    for token in tokens {
        let needle = if match_case {
            token.clone()
        } else {
            token.to_lowercase()
        };
        if needle.is_empty() {
            continue;
        }
        let position = find_token(&haystack, &needle).or_else(|| {
            match_case
                .then(|| {
                    case_match_ranges(&flat, token, true)
                        .first()
                        .map(|(start, _)| *start)
                })
                .flatten()
        });
        if let Some(pos) = position {
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
    haystack.find(needle)
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
/// before that narrowing. Notes live in the user database and return an empty
/// page here. Pass `LibraryPage::next` to continue all sources independently.
pub fn search_filtered(
    conn: &Connection,
    compiled: &CompiledQuery,
    scope: SearchScope,
    filter: VerseFilter,
    chip: Option<u8>,
    cursor: Option<SearchCursor>,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    register_case_matcher(conn, compiled)?;
    if compiled.strongs.is_some() || compiled.fts.is_empty() {
        return Ok(empty_page());
    }
    let cursor = cursor.unwrap_or_default();
    let limit = limit.max(1);
    match scope {
        SearchScope::Kjv => {
            search_verse_page_offset(conn, compiled, filter, chip, None, cursor, limit)
        }
        SearchScope::Commentary => {
            search_commentary_page(conn, compiled, filter, chip, cursor, limit)
        }
        SearchScope::Dictionaries => search_entry_page(conn, compiled, "dictionary", cursor, limit),
        SearchScope::Topics => search_entry_page(conn, compiled, "topic", cursor, limit),
        SearchScope::Notes => Ok(empty_page()),
        SearchScope::All => search_all_page(conn, compiled, filter, chip, cursor, limit),
    }
}

fn empty_page() -> LibraryPage {
    LibraryPage {
        hits: Vec::new(),
        total: 0,
        by_book: Vec::new(),
        next: SearchCursor::default(),
    }
}

fn search_commentary_page(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    chip: Option<u8>,
    mut cursor: SearchCursor,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    let by_book = commentary_book_counts(conn, compiled, filter)?;
    let mut narrowed = filter;
    if let Some(book) = chip {
        narrowed.book = Some(book);
    }
    let total = filtered_total(&by_book, narrowed.book);
    let hits = if limit == 0 || cursor.commentary as i64 >= total {
        Vec::new()
    } else {
        commentary_hits(conn, compiled, narrowed, cursor.commentary, limit)?
    };
    cursor.commentary += hits.len();
    Ok(LibraryPage {
        hits,
        total,
        by_book,
        next: cursor,
    })
}

fn commentary_hits(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
    offset: usize,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let (book, chapter, book_min, book_max) = filter_params(filter);
    let limit = limit.max(1) as i64;
    let offset = offset as i64;
    let o = param_base(compiled);
    let pred = text_pred(compiled, "resources_fts", "resources_fts.text");
    let snippet_col = if compiled.substring.is_some() || compiled.match_case {
        "resources_fts.text"
    } else {
        "snippet(resources_fts, 0, '', '', '…', 16)"
    };
    let sql = format!(
        "
        SELECT resources_fts.module, m.title,
               resources_fts.book, resources_fts.chapter, resources_fts.verse,
               resources_fts.text,
               {snippet_col}
        FROM resources_fts
        JOIN modules m ON m.id = resources_fts.module
        WHERE {pred} AND m.kind = 'commentary'
          AND resources_fts.book BETWEEN ?{bmin} AND ?{bmax}
          AND (?{book} = 0 OR resources_fts.book = ?{book})
          AND (?{chapter} = 0 OR resources_fts.chapter = ?{chapter})
        ORDER BY resources_fts.book, resources_fts.chapter, resources_fts.verse,
                 resources_fts.module
        LIMIT ?{limit} OFFSET ?{offset}
        ",
        bmin = o + 1,
        bmax = o + 2,
        book = o + 3,
        chapter = o + 4,
        limit = o + 5,
        offset = o + 6,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rest: [&dyn rusqlite::ToSql; 6] = [&book_min, &book_max, &book, &chapter, &limit, &offset];
    let tokens = compiled.tokens.clone();
    let custom_snippet = compiled.substring.is_some() || compiled.match_case;
    let match_case = compiled.match_case;
    let rows = stmt.query_map(params_from_iter(bind_params(compiled, &rest)), move |row| {
        let text: String = row.get(5)?;
        let snippet: String = row.get(6)?;
        let snippet = if custom_snippet {
            window_snippet_with_case(&text, &tokens, 80, match_case)
        } else {
            coalesce_snippet(snippet, &text)
        };
        Ok(LibraryHit {
            kind: LibraryKind::Commentary,
            module: row.get(0)?,
            title: row.get(1)?,
            book: Some(row.get(2)?),
            chapter: Some(row.get(3)?),
            verse: Some(row.get(4)?),
            headword: None,
            snippet,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn commentary_book_counts(
    conn: &Connection,
    compiled: &CompiledQuery,
    filter: VerseFilter,
) -> Result<Vec<BookCount>, DbError> {
    let chapter = i64::from(filter.chapter.unwrap_or(0));
    let book_min = i64::from(filter.book_min);
    let book_max = i64::from(filter.book_max);
    let o = param_base(compiled);
    let pred = text_pred(compiled, "resources_fts", "resources_fts.text");
    let sql = format!(
        "
        SELECT resources_fts.book, count(*)
        FROM resources_fts
        JOIN modules m ON m.id = resources_fts.module
        WHERE {pred} AND m.kind = 'commentary'
          AND resources_fts.book BETWEEN ?{bmin} AND ?{bmax}
          AND (?{chapter} = 0 OR resources_fts.chapter = ?{chapter})
        GROUP BY resources_fts.book
        ORDER BY resources_fts.book
        ",
        bmin = o + 1,
        bmax = o + 2,
        chapter = o + 3,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rest: [&dyn rusqlite::ToSql; 3] = [&book_min, &book_max, &chapter];
    let rows = stmt.query_map(params_from_iter(bind_params(compiled, &rest)), |row| {
        Ok(BookCount {
            book: row.get(0)?,
            count: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn search_entry_page(
    conn: &Connection,
    compiled: &CompiledQuery,
    kind: &str,
    mut cursor: SearchCursor,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    let o = param_base(compiled);
    let pred = entry_pred(compiled);
    let total_sql = format!(
        "
        SELECT count(*)
        FROM entries_fts
        JOIN modules m ON m.id = entries_fts.module
        WHERE {pred} AND m.kind = ?{kind}
        ",
        kind = o + 1,
    );
    let rest: [&dyn rusqlite::ToSql; 1] = [&kind];
    let total: i64 = conn.query_row(
        &total_sql,
        params_from_iter(bind_params(compiled, &rest)),
        |row| row.get(0),
    )?;
    let offset = if kind == "topic" {
        cursor.topics
    } else {
        cursor.dictionaries
    };
    let hits = if limit == 0 || offset as i64 >= total {
        Vec::new()
    } else {
        search_entry_kind(conn, compiled, kind, offset, limit)?
    };
    if kind == "topic" {
        cursor.topics += hits.len();
    } else {
        cursor.dictionaries += hits.len();
    }
    Ok(LibraryPage {
        hits,
        total,
        by_book: Vec::new(),
        next: cursor,
    })
}

fn search_entry_kind(
    conn: &Connection,
    compiled: &CompiledQuery,
    kind: &str,
    offset: usize,
    limit: usize,
) -> Result<Vec<LibraryHit>, DbError> {
    let library_kind = match kind {
        "topic" => LibraryKind::Topic,
        "lexicon" => LibraryKind::Lexicon,
        _ => LibraryKind::Dictionary,
    };
    let limit = limit.max(1) as i64;
    let offset = offset as i64;
    let exact = compiled.tokens.first().cloned().unwrap_or_default();
    let o = param_base(compiled);
    let pred = entry_pred(compiled);
    let snippet_col = if compiled.substring.is_some() || compiled.match_case {
        "entries_fts.text"
    } else {
        "snippet(entries_fts, 1, '', '', '…', 16)"
    };
    let order = if compiled.substring.is_some() {
        format!(
            "CASE WHEN entries_fts.headword = ?{exact} COLLATE NOCASE THEN 0 ELSE 1 END,
                 entries_fts.headword COLLATE NOCASE,
                 entries_fts.module,
                 entries_fts.i",
            exact = o + 3,
        )
    } else {
        format!(
            "CASE WHEN entries_fts.headword = ?{exact} COLLATE NOCASE THEN 0 ELSE 1 END,
                 bm25(entries_fts, 5.0, 1.0),
                 entries_fts.headword COLLATE NOCASE,
                 entries_fts.module,
                 entries_fts.i",
            exact = o + 3,
        )
    };
    let sql = format!(
        "
        SELECT entries_fts.module, m.title, entries_fts.headword, entries_fts.text,
               {snippet_col}
        FROM entries_fts
        JOIN modules m ON m.id = entries_fts.module
        WHERE {pred} AND m.kind = ?{kind}
        ORDER BY {order}
        LIMIT ?{limit} OFFSET ?{offset}
        ",
        kind = o + 1,
        limit = o + 2,
        offset = o + 4,
    );
    let mut stmt = conn.prepare(&sql)?;
    let rest: [&dyn rusqlite::ToSql; 4] = [&kind, &limit, &exact, &offset];
    let tokens = compiled.tokens.clone();
    let custom_snippet = compiled.substring.is_some() || compiled.match_case;
    let match_case = compiled.match_case;
    let rows = stmt.query_map(params_from_iter(bind_params(compiled, &rest)), move |row| {
        let headword: String = row.get(2)?;
        let text: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let fallback = if text.is_empty() { &headword } else { &text };
        let snippet = if custom_snippet {
            window_snippet_with_case(fallback, &tokens, 80, match_case)
        } else {
            coalesce_snippet(snippet, fallback)
        };
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
    cursor: SearchCursor,
    limit: usize,
) -> Result<LibraryPage, DbError> {
    // Distribute small pages fairly as well: rotate any remainder so no
    // source is permanently starved when the limit is below four.
    let mut quota = [limit / 4; 4];
    for i in 0..limit % 4 {
        quota[(cursor.round + i) % 4] += 1;
    }
    let verses = search_verse_page_offset(conn, compiled, filter, chip, None, cursor, quota[0])?;
    let comments = search_commentary_page(conn, compiled, filter, chip, verses.next, quota[1])?;
    let (dicts, topics) = if chip.is_some() {
        (empty_page(), empty_page())
    } else {
        let dicts = search_entry_page(conn, compiled, "dictionary", comments.next, quota[2])?;
        let topics = search_entry_page(conn, compiled, "topic", dicts.next, quota[3])?;
        (dicts, topics)
    };
    let total = verses.total + comments.total + dicts.total + topics.total;
    let mut next = if chip.is_some() {
        comments.next
    } else {
        topics.next
    };
    next.round = cursor.round.wrapping_add(1) % 4;
    let source_totals = [verses.total, comments.total, dicts.total, topics.total];
    let by_book = verses.by_book;
    let mut hits = verses.hits;
    hits.extend(comments.hits);
    hits.extend(dicts.hits);
    hits.extend(topics.hits);
    let mut narrowed = filter;
    if let Some(book) = chip {
        narrowed.book = Some(book);
    }
    // Reuse spare quota from exhausted sources, so even tiny pages cannot
    // appear empty while another source still has results.
    for step in 0..4 {
        let remaining = limit.saturating_sub(hits.len());
        if remaining == 0 {
            break;
        }
        let source = (cursor.round + step) % 4;
        let offset = match source {
            0 => next.verses,
            1 => next.commentary,
            2 => next.dictionaries,
            _ => next.topics,
        };
        if offset as i64 >= source_totals[source] {
            continue;
        }
        let additional = match source {
            0 => verse_hits(conn, compiled, narrowed, None, offset, remaining)?
                .into_iter()
                .map(LibraryHit::from_verse)
                .collect::<Vec<_>>(),
            1 => commentary_hits(conn, compiled, narrowed, offset, remaining)?,
            2 => search_entry_kind(conn, compiled, "dictionary", offset, remaining)?,
            _ => search_entry_kind(conn, compiled, "topic", offset, remaining)?,
        };
        match source {
            0 => next.verses += additional.len(),
            1 => next.commentary += additional.len(),
            2 => next.dictionaries += additional.len(),
            _ => next.topics += additional.len(),
        }
        hits.extend(additional);
    }
    Ok(LibraryPage {
        hits,
        total,
        by_book,
        next,
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
        assert_eq!(phrase.fts, "\"only begotten\"*");
        assert_eq!(phrase.tokens, ["only", "begotten"]);
        assert!(phrase.strongs.is_none());
        let with_and = compile_query("bread and wine", MatchMode::Phrase).unwrap();
        assert_eq!(with_and.fts, "\"bread and wine\"*");
        let explicit = compile_query("faith AND works", MatchMode::Phrase).unwrap();
        assert_eq!(explicit.fts, "faith* AND works*");
        let any = compile_query("faith works", MatchMode::AnyWord).unwrap();
        assert_eq!(any.fts, "faith* OR works*");
        let all = compile_query("faith works", MatchMode::AllWords).unwrap();
        assert_eq!(all.fts, "faith* AND works*");
        let prefix = compile_query("lov*", MatchMode::Phrase).unwrap();
        assert_eq!(prefix.fts, "lov*");
        assert_eq!(prefix.tokens, ["lov"]);
        let partial = compile_query("eart", MatchMode::Phrase).unwrap();
        assert_eq!(partial.fts, "eart*");
        assert_eq!(
            partial.substring.as_deref(),
            Some("{c} LIKE '%eart%' COLLATE NOCASE")
        );
        assert_eq!(partial.tokens, ["eart"]);
        let short = compile_query("in", MatchMode::Phrase).unwrap();
        assert_eq!(short.fts, "in");
        let exact = compile_query("eart", MatchMode::ExactWord).unwrap();
        assert_eq!(exact.fts, "eart");
        let exact_phrase = compile_query("only begotten", MatchMode::ExactWord).unwrap();
        assert_eq!(exact_phrase.fts, "\"only begotten\"");
        let starred = compile_query("ear*", MatchMode::ExactWord).unwrap();
        assert_eq!(starred.fts, "ear*");
        let possessive = compile_query("God's", MatchMode::Phrase).unwrap();
        assert_eq!(possessive.fts, "God*");
        let code = compile_query("h430", MatchMode::Phrase).unwrap();
        assert_eq!(code.strongs.as_deref(), Some("H430"));
        assert!(compile_query("AND OR", MatchMode::Phrase).is_none());
        assert!(compile_query("   ", MatchMode::Phrase).is_none());
        assert_eq!(MatchMode::from_saved(0, 0), MatchMode::Phrase);
        assert_eq!(MatchMode::from_saved(1, 0), MatchMode::AllWords);
        assert_eq!(MatchMode::from_saved(0, 1), MatchMode::ExactWord);
        assert_eq!(MatchMode::from_saved(1, 1), MatchMode::ExactWord);
        assert_eq!(MatchMode::from_saved(3, 0), MatchMode::ExactWord);
        assert_eq!(MatchMode::Phrase.label(), "Contains");
        assert_eq!(MatchMode::ExactWord.label(), "Exact word");
        assert_eq!(MatchMode::Phrase.word_match(), WordMatch::Prefix);
        assert_eq!(MatchMode::ExactWord.word_match(), WordMatch::Exact);
    }

    #[test]
    fn match_case_keeps_exact_boundaries_phrases_prefixes_and_operators() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis');
            INSERT INTO verses (book, chapter, verse, text) VALUES
                (1, 1, 1, 'holy one'),
                (1, 1, 2, 'Holy One'),
                (1, 1, 3, 'Holy [One]'),
                (1, 1, 4, 'Holy one'),
                (1, 1, 5, 'Holiness One');
            "#,
        )
        .unwrap();
        rebuild_verses_fts(&conn).unwrap();
        let verses = |query: &str, mode: MatchMode, match_case: bool| {
            let compiled = compile_query_with_case(query, mode, match_case).unwrap();
            let page =
                search_verse_page(&conn, &compiled, VerseFilter::all(), None, None, 20).unwrap();
            page.hits
                .into_iter()
                .map(|hit| hit.verse.unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(verses("Holy One", MatchMode::ExactWord, true), vec![2, 3]);
        assert_eq!(verses("holy one", MatchMode::ExactWord, true), vec![1]);
        assert_eq!(
            verses("Holy One", MatchMode::ExactWord, false),
            vec![1, 2, 3, 4]
        );
        assert_eq!(verses("Hol*", MatchMode::ExactWord, true), vec![2, 3, 4, 5]);
        assert_eq!(verses("Holy NOT One", MatchMode::ExactWord, true), vec![4]);
        assert_eq!(
            verses("Holy OR one", MatchMode::ExactWord, true),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn near_queries_observe_distance_case_and_boolean_operators() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis');
            INSERT INTO verses (book, chapter, verse, text) VALUES
                (1, 1, 1, 'Holy One'),
                (1, 1, 2, 'Holy blessed and mighty One'),
                (1, 1, 3, 'holy One'),
                (1, 1, 4, 'Holy one a b c d e f g h i j holy One'),
                (1, 1, 5, 'One Holy'),
                (1, 1, 6, 'Lord');
            "#,
        )
        .unwrap();
        rebuild_verses_fts(&conn).unwrap();
        let verses = |query: &str, mode: MatchMode, match_case: bool| {
            let compiled = compile_query_with_case(query, mode, match_case)
                .unwrap_or_else(|| panic!("failed to compile {query:?}"));
            let page =
                search_verse_page(&conn, &compiled, VerseFilter::all(), None, None, 20).unwrap();
            page.hits
                .iter()
                .map(|hit| hit.verse.unwrap())
                .collect::<Vec<_>>()
        };
        for mode in MatchMode::ALL {
            assert_eq!(verses("Holy NEAR/0 One", mode, true), vec![1, 5]);
            assert_eq!(verses("Holy NEAR/3 One", mode, true), vec![1, 2, 5]);
            assert_eq!(verses("Holy NEAR One", mode, true), vec![1, 2, 5]);
            assert_eq!(verses("Holy NEAR/0 One", mode, false), vec![1, 3, 4, 5]);
            assert_eq!(verses("Holy NEAR/0 One OR Lord", mode, true), vec![1, 5, 6]);
            assert_eq!(verses("Holy NEAR/0 Holy", mode, true), vec![1, 2, 4, 5]);
        }
    }

    #[test]
    fn match_case_ignores_diacritics_but_not_letter_case() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis');
            INSERT INTO verses (book, chapter, verse, text) VALUES
                (1, 1, 1, 'Elōhim'),
                (1, 1, 2, 'elōhim'),
                (1, 1, 3, 'Elohim'),
                (1, 1, 4, 'Elohimic');
            "#,
        )
        .unwrap();
        rebuild_verses_fts(&conn).unwrap();
        for query in ["Elohim", "Elōhim", "Elo\u{304}him"] {
            let compiled = compile_query_with_case(query, MatchMode::ExactWord, true).unwrap();
            let page =
                search_verse_page(&conn, &compiled, VerseFilter::all(), None, None, 20).unwrap();
            let verses: Vec<_> = page.hits.iter().map(|hit| hit.verse.unwrap()).collect();
            assert_eq!(verses, vec![1, 3], "{query}");
        }
        let lowercase = compile_query_with_case("elohim", MatchMode::ExactWord, true).unwrap();
        let page =
            search_verse_page(&conn, &lowercase, VerseFilter::all(), None, None, 20).unwrap();
        assert_eq!(page.hits.len(), 1);
        assert_eq!(page.hits[0].verse, Some(2));
    }

    #[test]
    fn case_not_prefilter_keeps_only_positive_candidates() {
        for (query, expected) in [
            ("God NOT Lord", "God"),
            ("God NOT Lord OR Christ", "God OR Christ"),
            ("God AND mercy NOT Lord", "God AND mercy"),
        ] {
            let compiled = compile_query_with_case(query, MatchMode::ExactWord, true).unwrap();
            assert_eq!(compiled.fts, expected, "{query}");
        }
        let term = CaseTerm::new("God", false);
        let long_ascii = format!("{} God {}", "word ".repeat(50_000), "word ".repeat(50_000));
        assert!(term.occurs_in(&long_ascii));
        assert!(!term.occurs_in(&long_ascii.replace("God", "god")));
        assert!(term.occurs_in("— Ǵod —"));
        assert!(!term.occurs_in("— ǵod —"));
    }

    #[test]
    fn case_checks_are_cached_per_row_within_a_search() {
        let mut cache = HashMap::new();
        let evaluations = std::cell::Cell::new(0);
        let evaluate = || {
            evaluations.set(evaluations.get() + 1);
            true
        };
        assert!(cached_case_match(&mut cache, 7, evaluate));
        assert!(cached_case_match(&mut cache, 7, evaluate));
        assert_eq!(evaluations.get(), 1);
        assert!(cached_case_match(&mut cache, 8, evaluate));
        assert_eq!(evaluations.get(), 2);
    }

    #[test]
    fn match_case_evaluates_entry_columns_without_joining_phrases() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license)
                VALUES ('Easton', 'dictionary', 'Easton', 'public-domain');
            INSERT INTO entries (module, i, headword, text) VALUES
                ('Easton', 0, 'Holy', 'One followed by holy one'),
                ('Easton', 1, 'other', 'Holy One');
            "#,
        )
        .unwrap();
        rebuild_entries_fts(&conn).unwrap();
        let phrase = compile_query_with_case("Holy One", MatchMode::ExactWord, true).unwrap();
        let page = search_filtered(
            &conn,
            &phrase,
            SearchScope::Dictionaries,
            VerseFilter::all(),
            None,
            None,
            20,
        )
        .unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.hits[0].headword.as_deref(), Some("other"));

        let and = compile_query_with_case("Holy AND One", MatchMode::ExactWord, true).unwrap();
        let page = search_filtered(
            &conn,
            &and,
            SearchScope::Dictionaries,
            VerseFilter::all(),
            None,
            None,
            20,
        )
        .unwrap();
        assert_eq!(page.total, 2);
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
    fn verses_fts_ignores_translator_notes() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 2, 15, 'And the LORD God took the man, and put him into the garden of Eden to dress it and to keep it. {the man: or, Adam}', 0),
                (1, 2, 19, 'And out of the ground the LORD God formed every beast of the field, and brought them unto Adam.', 0);
            "#,
        )
        .unwrap();
        conn.execute_batch(
            "DELETE FROM verses_fts;
             INSERT INTO verses_fts (text, book, chapter, verse)
             SELECT text, book, chapter, verse FROM verses;
             DELETE FROM meta WHERE key = 'verses_fts_plain';",
        )
        .unwrap();
        let before = search_verses(&conn, "adam", 20).unwrap();
        assert!(
            before
                .iter()
                .any(|h| h.book == 1 && h.chapter == 2 && h.verse == 15),
            "note-indexed FTS should still match 2:15: {before:?}"
        );
        ensure_verses_fts(&conn).unwrap();
        let hits = search_verses(&conn, "adam", 20).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 1 && h.chapter == 2 && h.verse == 19),
            "body match missing: {hits:?}"
        );
        assert!(
            hits.iter()
                .all(|h| !(h.book == 1 && h.chapter == 2 && h.verse == 15)),
            "translator note must not match: {hits:?}"
        );
    }

    #[test]
    fn empty_or_operator_only_query_is_empty() {
        let conn = open_memory().unwrap();
        seed(&conn);
        assert!(search_verses(&conn, "   ", 10).unwrap().is_empty());
        assert!(search_verses(&conn, "AND OR", 10).unwrap().is_empty());
        assert_eq!(
            match_query("only begotten").as_deref(),
            Some("\"only begotten\"*")
        );
        assert_eq!(match_query("begotten").as_deref(), Some("begotten*"));
        assert_eq!(match_query("eart").as_deref(), Some("eart*"));
        assert_eq!(match_query("in").as_deref(), Some("in"));
    }

    #[test]
    fn partial_word_matches_earth() {
        let conn = open_memory().unwrap();
        seed(&conn);
        let hits = search_verses(&conn, "eart", 20).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 1 && h.chapter == 1 && h.verse == 1),
            "hits: {hits:?}"
        );
        assert!(hits.iter().all(|h| h.text.to_lowercase().contains("eart")));
        let snippet = window_snippet(
            "In the beginning God created the heaven and the earth.",
            &["eart".into()],
            80,
        );
        assert!(snippet.to_lowercase().contains("earth"), "{snippet}");
    }

    #[test]
    fn exact_word_skips_longer_stems() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            "INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 2, 2, 'So that thou incline thine ear unto wisdom', 0),
                (1, 3, 1, 'hear thou in heaven', 0);",
        )
        .unwrap();
        rebuild_verses_fts(&conn).unwrap();
        let exact = compile_query("ear", MatchMode::ExactWord).unwrap();
        assert_eq!(exact.fts, "ear");
        assert!(exact.substring.is_none());
        let hits = search_verse_page(&conn, &exact, VerseFilter::all(), None, None, 20).unwrap();
        assert_eq!(hits.total, 1, "{hits:?}");
        assert_eq!(hits.hits[0].verse, Some(2));
        let prefix = compile_query("ear", MatchMode::Phrase).unwrap();
        assert_eq!(
            prefix.substring.as_deref(),
            Some("{c} LIKE '%ear%' COLLATE NOCASE")
        );
        let wider = search_verse_page(&conn, &prefix, VerseFilter::all(), None, None, 20).unwrap();
        assert!(wider.total >= 3, "{wider:?}");
        assert!(wider.hits.iter().any(|h| h.verse == Some(1)));
        assert!(wider.hits.iter().any(|h| h.verse == Some(2)));
        assert!(wider
            .hits
            .iter()
            .any(|h| h.chapter == Some(3) && h.verse == Some(1)));
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
    fn small_all_scope_pages_do_not_starve_later_sources() {
        let conn = open_memory().unwrap();
        seed_library(&conn);
        let query = compile_query("eastonbodyterm", MatchMode::ExactWord).unwrap();
        let first = search_filtered(
            &conn,
            &query,
            SearchScope::All,
            VerseFilter::all(),
            None,
            None,
            1,
        )
        .unwrap();
        assert_eq!(first.total, 1);
        assert_eq!(first.hits.len(), 1);
        assert_eq!(first.hits[0].kind, LibraryKind::Dictionary);
        let next = search_filtered(
            &conn,
            &query,
            SearchScope::All,
            VerseFilter::all(),
            None,
            Some(first.next),
            1,
        )
        .unwrap();
        assert!(next.hits.is_empty());
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
