use crate::layout;
use crate::nav::{self, Ref};
use bible_app_db::{
    Book, BookCount, CompiledQuery, LibraryHit, LibraryKind, LibraryPage, MatchMode, SearchScope,
    VerseFilter,
};
use gtk::prelude::*;
use relm4::gtk;
use rusqlite::Connection;
use std::path::Path;

pub const PAGE: usize = 80;
const OT_LAST: u8 = 39;

pub fn placeholder(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::Kjv => "Search the KJV",
        SearchScope::Commentary => "Search MHC and TSK",
        SearchScope::Dictionaries => "Search dictionaries",
        SearchScope::Topics => "Search topics",
        SearchScope::Notes => "Search your notes",
        SearchScope::All => "Search the library",
    }
}

pub fn status(query: &str, shown: usize, total: i64, scope: SearchScope) -> String {
    let q = query.trim();
    let (one, many) = nouns(scope);
    if q.is_empty() {
        empty_prompt(scope).into()
    } else if shown == 0 || total <= 0 {
        format!("No {many} match \"{q}\".")
    } else if total == 1 {
        format!("1 {one}")
    } else if shown < total as usize {
        format!("{shown} of {total} {many}")
    } else {
        format!("{total} {many}")
    }
}

pub fn short_status() -> &'static str {
    "Type at least two letters, a reference, or a Strong's code."
}

fn nouns(scope: SearchScope) -> (&'static str, &'static str) {
    match scope {
        SearchScope::Kjv => ("verse", "verses"),
        SearchScope::Commentary => ("comment", "comments"),
        SearchScope::Dictionaries => ("entry", "entries"),
        SearchScope::Topics => ("topic", "topics"),
        SearchScope::Notes => ("note", "notes"),
        SearchScope::All => ("result", "results"),
    }
}

fn empty_prompt(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::Kjv => "Type a word or phrase from the King James Version.",
        SearchScope::Commentary => "Type a word or phrase from Matthew Henry or TSK.",
        SearchScope::Dictionaries => "Type a word or phrase from the dictionaries.",
        SearchScope::Topics => "Type a word or phrase from the topical works.",
        SearchScope::Notes => "Type a word or phrase from your notes.",
        SearchScope::All => "Type a word or phrase to search the library.",
    }
}

pub fn empty_description(query: &str, scope: SearchScope) -> Option<&'static str> {
    if !query.trim().is_empty() {
        return None;
    }
    Some(match scope {
        SearchScope::Kjv => "Try a short phrase, for example only begotten.",
        SearchScope::Commentary => "Try a heading or phrase, for example beginning.",
        SearchScope::Dictionaries => "Try a headword or a word from the definition.",
        SearchScope::Topics => "Try a topic name or a word from the entry.",
        SearchScope::Notes => "Try a word from a note you have written.",
        SearchScope::All => "Try a short phrase, for example only begotten.",
    })
}

pub fn source_label(module: &str) -> &str {
    match module {
        "KJV" => "KJV",
        "MHC" => "Matthew Henry",
        "TSK" => "TSK",
        "Easton" => "Easton's",
        "Smith" => "Smith's",
        "Names" => "Hitchcock",
        "ATSD" => "ATS",
        "Webster" => "Webster",
        "Nave" => "Nave",
        "Torrey" => "Torrey",
        "BDB" => "BDB",
        "Thayer" => "Thayer",
        other => other,
    }
}

pub fn hit_ref(hit: &LibraryHit) -> Option<Ref> {
    Some(Ref {
        book: hit.book?,
        chapter: hit.chapter?,
        verse: hit.verse?,
    })
}

pub fn caption(hit: &LibraryHit, books: &[Book]) -> String {
    let source = source_label(&hit.module);
    match hit.kind {
        LibraryKind::Verse | LibraryKind::Commentary => match hit_ref(hit) {
            Some(at) => format!("{source} · {}", nav::format_ref(books, at)),
            None => source.to_string(),
        },
        LibraryKind::Note => match hit_ref(hit) {
            Some(at) => format!("Note · {}", nav::format_ref(books, at)),
            None => "Note".into(),
        },
        LibraryKind::Dictionary | LibraryKind::Topic | LibraryKind::Lexicon => {
            match hit.headword.as_deref() {
                Some(head) if !head.is_empty() => format!("{source} · {head}"),
                _ => source.to_string(),
            }
        }
    }
}

pub fn group_label(hit: &LibraryHit, books: &[Book], scope: SearchScope) -> String {
    if scope == SearchScope::All {
        return match hit.kind {
            LibraryKind::Verse => "Verses".into(),
            LibraryKind::Commentary => "Commentary".into(),
            LibraryKind::Dictionary => "Dictionaries".into(),
            LibraryKind::Topic => "Topics".into(),
            LibraryKind::Lexicon => "Lexicon".into(),
            LibraryKind::Note => "Notes".into(),
        };
    }
    match hit.kind {
        LibraryKind::Verse | LibraryKind::Commentary | LibraryKind::Note => hit
            .book
            .and_then(|id| books.iter().find(|b| b.id == id))
            .map(|b| b.name.clone())
            .unwrap_or_else(|| source_label(&hit.module).to_string()),
        LibraryKind::Dictionary | LibraryKind::Topic | LibraryKind::Lexicon => {
            source_label(&hit.module).to_string()
        }
    }
}

pub fn reference_text(hit: &LibraryHit, books: &[Book], scope: SearchScope) -> String {
    if hit.module == "goto" {
        return "Go to".into();
    }
    match hit.kind {
        LibraryKind::Dictionary | LibraryKind::Topic | LibraryKind::Lexicon => hit
            .headword
            .clone()
            .unwrap_or_else(|| source_label(&hit.module).to_string()),
        LibraryKind::Verse | LibraryKind::Commentary | LibraryKind::Note => {
            let Some(at) = hit_ref(hit) else {
                return source_label(&hit.module).to_string();
            };
            let cv = format!("{}:{}", at.chapter, at.verse);
            if scope == SearchScope::All {
                return nav::format_ref(books, at);
            }
            if hit.kind == LibraryKind::Commentary {
                format!("{} {cv}", source_label(&hit.module))
            } else {
                cv
            }
        }
    }
}

pub fn emphasize(text: &str, tokens: &[String]) -> String {
    if text.is_empty() {
        return String::new();
    }
    let lower = text.to_lowercase();
    if lower.len() != text.len() {
        return escape_markup(text);
    }
    let mut marks: Vec<(usize, usize)> = Vec::new();
    for token in tokens {
        let needle = token.to_lowercase();
        if needle.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(rel) = lower[from..].find(&needle) {
            let pos = from + rel;
            let before = lower[..pos].chars().next_back();
            let after = lower[pos + needle.len()..].chars().next();
            let left_ok = before.is_none_or(|c| !c.is_ascii_alphanumeric());
            let right_ok = after.is_none_or(|c| !c.is_ascii_alphanumeric());
            if left_ok && right_ok {
                marks.push((pos, pos + needle.len()));
            }
            from = pos + needle.len().max(1);
        }
    }
    marks.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in marks {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    let mut out = String::new();
    let mut cursor = 0;
    for (start, end) in merged {
        out.push_str(&escape_markup(&text[cursor..start]));
        out.push_str("<b>");
        out.push_str(&escape_markup(&text[start..end]));
        out.push_str("</b>");
        cursor = end;
    }
    out.push_str(&escape_markup(&text[cursor..]));
    out
}

fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn row(
    hit: &LibraryHit,
    books: &[Book],
    scope: SearchScope,
    tokens: &[String],
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    box_.set_margin_top(4);
    box_.set_margin_bottom(4);

    let reference = reference_text(hit, books, scope);
    let title = gtk::Label::new(Some(&reference));
    title.set_xalign(0.0);
    title.set_width_chars(14);
    title.add_css_class("caption");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let snippet = gtk::Label::new(None);
    snippet.set_markup(&emphasize(&hit.snippet, tokens));
    snippet.set_xalign(0.0);
    snippet.set_hexpand(true);
    snippet.set_ellipsize(gtk::pango::EllipsizeMode::End);
    snippet.set_single_line_mode(true);

    box_.append(&title);
    box_.append(&snippet);
    row.set_child(Some(&box_));
    row.set_activatable(true);
    row.set_tooltip_text(Some(&caption(hit, books)));
    row
}

pub fn refill_list(
    list: &gtk::ListBox,
    hits: &[LibraryHit],
    books: &[Book],
    scope: SearchScope,
    tokens: &[String],
    groups: &std::cell::RefCell<Vec<String>>,
    append: bool,
) {
    if !append {
        while let Some(child) = list.row_at_index(0) {
            list.remove(&child);
        }
        groups.borrow_mut().clear();
    }
    for hit in hits {
        groups.borrow_mut().push(group_label(hit, books, scope));
        list.append(&row(hit, books, scope, tokens));
    }
    if !append {
        if let Some(first) = list.row_at_index(0) {
            list.select_row(Some(&first));
        }
    }
}

pub fn show_chips(scope: SearchScope, strongs: bool) -> bool {
    strongs
        || matches!(
            scope,
            SearchScope::Kjv | SearchScope::Commentary | SearchScope::Notes
        )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchRange {
    #[default]
    All,
    ThisBook,
    ThisChapter,
    Ot,
    Nt,
}

impl SearchRange {
    pub const ALL: [SearchRange; 5] = [
        Self::All,
        Self::ThisBook,
        Self::ThisChapter,
        Self::Ot,
        Self::Nt,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "Whole Bible",
            Self::ThisBook => "This book",
            Self::ThisChapter => "This chapter",
            Self::Ot => "Old Testament",
            Self::Nt => "New Testament",
        }
    }

    pub fn from_index(index: u32) -> Self {
        Self::ALL.get(index as usize).copied().unwrap_or(Self::All)
    }

    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|&r| r == self).unwrap_or(0) as u32
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookChip {
    Auto,
    AllBooks,
    Book(u8),
}

#[derive(Debug, Clone)]
pub enum Plan {
    Idle,
    Short,
    Goto(Ref),
    Ready(Prepared),
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub compiled: CompiledQuery,
    pub scope: SearchScope,
    pub filter: VerseFilter,
    pub chip: BookChip,
    pub current_book: u8,
    pub db_path: std::path::PathBuf,
    pub user_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub hits: Vec<LibraryHit>,
    pub total: i64,
    pub by_book: Vec<BookCount>,
    pub tokens: Vec<String>,
    pub strongs: Option<String>,
    pub chosen_book: Option<u8>,
    pub append: bool,
}

pub fn plan(
    query: &str,
    books: &[Book],
    current: Ref,
    mode: MatchMode,
    range: SearchRange,
    chip: BookChip,
    scope: SearchScope,
) -> Plan {
    let query = query.trim();
    if query.is_empty() {
        return Plan::Idle;
    }
    let (slash_book, slash_chapter, text) = slash_target(query, books, current);
    let text = text.as_str();
    if let Some(compiled) = bible_app_db::compile_query(text, mode) {
        if compiled.strongs.is_some() {
            return Plan::Ready(Prepared {
                compiled,
                scope,
                filter: bounds(range, current, slash_book, slash_chapter),
                chip,
                current_book: current.book,
                db_path: std::path::PathBuf::new(),
                user_path: None,
            });
        }
    }
    if slash_book.is_none() && query.chars().any(|c| c.is_ascii_digit()) {
        if let Some(at) = nav::parse_ref(query, books, current) {
            return Plan::Goto(at);
        }
    }
    let letters = text.chars().filter(|c| c.is_ascii_alphanumeric()).count();
    if letters < 2 {
        return Plan::Short;
    }
    let Some(compiled) = bible_app_db::compile_query(text, mode) else {
        return Plan::Idle;
    };
    Plan::Ready(Prepared {
        compiled,
        scope,
        filter: bounds(range, current, slash_book, slash_chapter),
        chip,
        current_book: current.book,
        db_path: std::path::PathBuf::new(),
        user_path: None,
    })
}

fn slash_target(query: &str, books: &[Book], current: Ref) -> (Option<u8>, Option<u8>, String) {
    let Some((left, right)) = query.split_once('/') else {
        return (None, None, query.to_string());
    };
    let left = left.trim();
    let right = right.trim();
    if left.is_empty() || right.is_empty() {
        return (None, None, query.to_string());
    }
    let Some(at) = nav::parse_ref(left, books, current) else {
        return (None, None, query.to_string());
    };
    if left.chars().any(|c| c.is_ascii_digit()) {
        (Some(at.book), Some(at.chapter), right.to_string())
    } else {
        (Some(at.book), None, right.to_string())
    }
}

fn bounds(
    range: SearchRange,
    current: Ref,
    slash_book: Option<u8>,
    slash_chapter: Option<u8>,
) -> VerseFilter {
    let mut filter = match range {
        SearchRange::All => VerseFilter::all(),
        SearchRange::ThisBook => VerseFilter {
            book: Some(current.book),
            book_min: current.book,
            book_max: current.book,
            ..VerseFilter::all()
        },
        SearchRange::ThisChapter => VerseFilter {
            book: Some(current.book),
            chapter: Some(current.chapter),
            book_min: current.book,
            book_max: current.book,
        },
        SearchRange::Ot => VerseFilter {
            book_min: 1,
            book_max: OT_LAST,
            ..VerseFilter::all()
        },
        SearchRange::Nt => VerseFilter {
            book_min: OT_LAST + 1,
            book_max: 66,
            ..VerseFilter::all()
        },
    };
    if let Some(book) = slash_book {
        filter.book = Some(book);
        filter.book_min = book;
        filter.book_max = book;
        filter.chapter = slash_chapter;
    }
    filter
}

pub fn execute(prepared: &Prepared, after: Option<(u8, u8, u8)>, append: bool) -> Outcome {
    let empty = Outcome {
        hits: Vec::new(),
        total: 0,
        by_book: Vec::new(),
        tokens: prepared.compiled.tokens.clone(),
        strongs: prepared.compiled.strongs.clone(),
        chosen_book: None,
        append,
    };
    let Ok(conn) = open_reader(&prepared.db_path) else {
        return empty;
    };
    if let Some(code) = prepared.compiled.strongs.clone() {
        return strongs_outcome(&conn, prepared, &code, after, append);
    }
    if prepared.scope == SearchScope::Notes {
        return notes_outcome(prepared, after, append);
    }
    let chip = chip_arg(prepared.chip);
    let mut page = match bible_app_db::search_filtered(
        &conn,
        &prepared.compiled,
        prepared.scope,
        prepared.filter,
        chip,
        after,
        PAGE,
    ) {
        Ok(page) => page,
        Err(_) => return empty,
    };
    let mut chosen = chip;
    if prepared.chip == BookChip::Auto
        && prepared.filter.book.is_none()
        && prepared.filter.chapter.is_none()
        && page
            .by_book
            .iter()
            .any(|b| b.book == prepared.current_book && b.count > 0)
    {
        chosen = Some(prepared.current_book);
        if let Ok(narrowed) = bible_app_db::search_filtered(
            &conn,
            &prepared.compiled,
            prepared.scope,
            prepared.filter,
            chosen,
            after,
            PAGE,
        ) {
            page.hits = narrowed.hits;
            page.total = narrowed.total;
        }
    }
    if prepared.scope == SearchScope::All {
        append_notes(prepared, &mut page);
    }
    finish(page, prepared, chosen, append)
}

fn strongs_outcome(
    conn: &Connection,
    prepared: &Prepared,
    code: &str,
    after: Option<(u8, u8, u8)>,
    append: bool,
) -> Outcome {
    let chip = match prepared.chip {
        BookChip::Book(id) => Some(id),
        BookChip::AllBooks => None,
        BookChip::Auto => None,
    };
    let Ok((hits, total, by_book)) = bible_app_db::strongs_page(
        conn,
        code,
        bible_app_db::StrongsWindow {
            book_min: prepared.filter.book_min,
            book_max: prepared.filter.book_max,
            book: prepared.filter.book.or(chip),
            chapter: prepared.filter.chapter,
            after,
            limit: PAGE,
        },
    ) else {
        return Outcome {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
            tokens: Vec::new(),
            strongs: Some(code.to_string()),
            chosen_book: prepared.filter.book.or(chip),
            append,
        };
    };
    let mut chosen = prepared.filter.book.or(chip);
    let (hits, total, by_book) = if prepared.chip == BookChip::Auto
        && prepared.filter.book.is_none()
        && prepared.filter.chapter.is_none()
        && by_book
            .iter()
            .any(|b| b.book == prepared.current_book && b.count > 0)
    {
        chosen = Some(prepared.current_book);
        bible_app_db::strongs_page(
            conn,
            code,
            bible_app_db::StrongsWindow {
                book_min: prepared.filter.book_min,
                book_max: prepared.filter.book_max,
                book: Some(prepared.current_book),
                chapter: None,
                after,
                limit: PAGE,
            },
        )
        .unwrap_or((hits, total, by_book))
    } else {
        (hits, total, by_book)
    };
    let hits = hits
        .into_iter()
        .map(|hit| {
            let (body, _) = layout::split_notes(&hit.snippet);
            LibraryHit {
                kind: LibraryKind::Verse,
                module: "KJV".into(),
                title: "King James Version".into(),
                book: Some(hit.book),
                chapter: Some(hit.chapter),
                verse: Some(hit.verse),
                headword: None,
                snippet: bible_app_db::window_snippet(&body, &[], 96),
            }
        })
        .collect();
    Outcome {
        hits,
        total,
        by_book,
        tokens: Vec::new(),
        strongs: Some(code.to_string()),
        chosen_book: chosen,
        append,
    }
}

fn notes_outcome(prepared: &Prepared, _after: Option<(u8, u8, u8)>, append: bool) -> Outcome {
    let Some(path) = prepared.user_path.as_deref() else {
        return Outcome {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
            tokens: prepared.compiled.tokens.clone(),
            strongs: None,
            chosen_book: None,
            append,
        };
    };
    let Ok(conn) = open_reader(path) else {
        return Outcome {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
            tokens: prepared.compiled.tokens.clone(),
            strongs: None,
            chosen_book: None,
            append,
        };
    };
    let chip = chip_arg(prepared.chip);
    let book = prepared.filter.book.or(chip);
    let Ok(page) = crate::user_db::search_notes(
        &conn,
        &prepared.compiled.fts,
        prepared.filter.book_min,
        prepared.filter.book_max,
        book,
        prepared.filter.chapter,
        PAGE,
    ) else {
        return Outcome {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
            tokens: prepared.compiled.tokens.clone(),
            strongs: None,
            chosen_book: book,
            append,
        };
    };
    let hits = page.hits;
    let mut total = page.total;
    let by_book = page
        .by_book
        .iter()
        .map(|(id, count)| BookCount {
            book: *id,
            count: *count,
        })
        .collect::<Vec<_>>();
    let mut chosen = book;
    let hits = if prepared.chip == BookChip::Auto
        && prepared.filter.book.is_none()
        && prepared.filter.chapter.is_none()
        && by_book
            .iter()
            .any(|b| b.book == prepared.current_book && b.count > 0)
    {
        chosen = Some(prepared.current_book);
        crate::user_db::search_notes(
            &conn,
            &prepared.compiled.fts,
            prepared.filter.book_min,
            prepared.filter.book_max,
            Some(prepared.current_book),
            None,
            PAGE,
        )
        .map(|page| {
            total = page.total;
            page.hits
        })
        .unwrap_or(hits)
    } else {
        hits
    };
    Outcome {
        hits: note_hits(&hits, &prepared.compiled.tokens),
        total,
        by_book,
        tokens: prepared.compiled.tokens.clone(),
        strongs: None,
        chosen_book: chosen,
        append,
    }
}

fn append_notes(prepared: &Prepared, page: &mut LibraryPage) {
    let Some(path) = prepared.user_path.as_deref() else {
        return;
    };
    let Ok(conn) = open_reader(path) else {
        return;
    };
    let Ok(found) = crate::user_db::search_notes(
        &conn,
        &prepared.compiled.fts,
        prepared.filter.book_min,
        prepared.filter.book_max,
        prepared.filter.book,
        prepared.filter.chapter,
        8,
    ) else {
        return;
    };
    page.hits
        .extend(note_hits(&found.hits, &prepared.compiled.tokens));
}

fn note_hits(hits: &[crate::user_db::NoteHit], tokens: &[String]) -> Vec<LibraryHit> {
    hits.iter()
        .map(|hit| LibraryHit {
            kind: LibraryKind::Note,
            module: "Notes".into(),
            title: "Notes".into(),
            book: Some(hit.book),
            chapter: Some(hit.chapter),
            verse: Some(hit.verse),
            headword: None,
            snippet: bible_app_db::window_snippet(&hit.text, tokens, 96),
        })
        .collect()
}

fn finish(page: LibraryPage, prepared: &Prepared, chosen: Option<u8>, append: bool) -> Outcome {
    Outcome {
        hits: page.hits,
        total: page.total,
        by_book: page.by_book,
        tokens: prepared.compiled.tokens.clone(),
        strongs: None,
        chosen_book: chosen.or(prepared.filter.book),
        append,
    }
}

fn chip_arg(chip: BookChip) -> Option<u8> {
    match chip {
        BookChip::Book(id) => Some(id),
        BookChip::Auto | BookChip::AllBooks => None,
    }
}

fn open_reader(path: &Path) -> rusqlite::Result<Connection> {
    let flags =
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    conn.pragma_update(None, "query_only", "ON")?;
    Ok(conn)
}

pub fn chip_text(books: &[Book], count: &BookCount) -> String {
    let name = books
        .iter()
        .find(|b| b.id == count.book)
        .map(|b| b.abbrev.as_str())
        .unwrap_or("?");
    format!("{name} {}", count.count)
}

pub fn goto_hit(at: Ref, books: &[Book]) -> LibraryHit {
    LibraryHit {
        kind: LibraryKind::Verse,
        module: "goto".into(),
        title: "Go to".into(),
        book: Some(at.book),
        chapter: Some(at.chapter),
        verse: Some(at.verse),
        headword: None,
        snippet: nav::format_ref(books, at),
    }
}

pub fn lexicon_hit(code: &str, module: &str) -> LibraryHit {
    LibraryHit {
        kind: LibraryKind::Lexicon,
        module: module.to_string(),
        title: module.to_string(),
        book: None,
        chapter: None,
        verse: None,
        headword: Some(code.to_string()),
        snippet: format!("Open {code}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bible_app_db::LibraryHit;

    fn books() -> Vec<Book> {
        vec![Book {
            id: 1,
            abbrev: "Ge".into(),
            name: "Genesis".into(),
        }]
    }

    #[test]
    fn status_empty_and_counts() {
        assert!(status("  ", 0, 0, SearchScope::Kjv).contains("Type a word"));
        assert!(status("foo", 0, 0, SearchScope::Kjv).contains("No verses"));
        assert_eq!(status("foo", 1, 1, SearchScope::Kjv), "1 verse");
        assert_eq!(status("foo", 3, 3, SearchScope::Kjv), "3 verses");
        assert_eq!(status("foo", 80, 973, SearchScope::Kjv), "80 of 973 verses");
        assert!(status("foo", 0, 0, SearchScope::Commentary).contains("No comments"));
        assert_eq!(status("foo", 1, 1, SearchScope::Dictionaries), "1 entry");
        assert_eq!(status("foo", 2, 2, SearchScope::Topics), "2 topics");
        assert_eq!(status("foo", 3, 3, SearchScope::All), "3 results");
        assert_eq!(status("foo", 2, 2, SearchScope::Notes), "2 notes");
    }

    #[test]
    fn empty_description_prompt_only_for_blank_query() {
        assert_eq!(
            empty_description("", SearchScope::Kjv),
            Some("Try a short phrase, for example only begotten.")
        );
        assert_eq!(
            empty_description("  ", SearchScope::Kjv),
            Some("Try a short phrase, for example only begotten.")
        );
        assert_eq!(empty_description("foo", SearchScope::Kjv), None);
        assert!(empty_description("", SearchScope::Commentary)
            .unwrap()
            .contains("beginning"));
    }

    #[test]
    fn placeholder_follows_scope() {
        assert_eq!(placeholder(SearchScope::Kjv), "Search the KJV");
        assert_eq!(placeholder(SearchScope::All), "Search the library");
        assert_eq!(placeholder(SearchScope::Notes), "Search your notes");
    }

    #[test]
    fn plan_routes_reference_strongs_and_book_slash() {
        let books = vec![
            Book {
                id: 43,
                abbrev: "Joh".into(),
                name: "John".into(),
            },
            Book {
                id: 1,
                abbrev: "Ge".into(),
                name: "Genesis".into(),
            },
        ];
        let current = Ref {
            book: 1,
            chapter: 1,
            verse: 1,
        };
        match plan(
            "John 3:16",
            &books,
            current,
            MatchMode::Phrase,
            SearchRange::All,
            BookChip::Auto,
            SearchScope::Kjv,
        ) {
            Plan::Goto(at) => assert_eq!((at.book, at.chapter, at.verse), (43, 3, 16)),
            other => panic!("expected goto, got {other:?}"),
        }
        match plan(
            "H430",
            &books,
            current,
            MatchMode::Phrase,
            SearchRange::All,
            BookChip::Auto,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => assert_eq!(p.compiled.strongs.as_deref(), Some("H430")),
            other => panic!("expected strongs, got {other:?}"),
        }
        match plan(
            "John/light",
            &books,
            current,
            MatchMode::Phrase,
            SearchRange::All,
            BookChip::Auto,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => {
                assert_eq!(p.compiled.fts, "light");
                assert_eq!(p.filter.book, Some(43));
                assert_eq!(p.filter.chapter, None);
            }
            other => panic!("expected book search, got {other:?}"),
        }
        match plan(
            "John:3/born",
            &books,
            current,
            MatchMode::Phrase,
            SearchRange::All,
            BookChip::Auto,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => {
                assert_eq!(p.filter.book, Some(43));
                assert_eq!(p.filter.chapter, Some(3));
                assert_eq!(p.compiled.fts, "born");
            }
            other => panic!("expected chapter search, got {other:?}"),
        }
        assert!(matches!(
            plan(
                "a",
                &books,
                current,
                MatchMode::Phrase,
                SearchRange::All,
                BookChip::Auto,
                SearchScope::Kjv,
            ),
            Plan::Short
        ));
    }

    #[test]
    fn emphasize_bolds_the_match_and_escapes() {
        assert_eq!(
            emphasize("the only begotten Son", &["begotten".into()]),
            "the only <b>begotten</b> Son"
        );
        assert!(emphasize("a < b & c", &["b".into()]).contains("&lt;"));
    }

    #[test]
    fn verse_reference_is_chapter_and_verse_under_a_book_group() {
        let books = books();
        let verse = LibraryHit {
            kind: LibraryKind::Verse,
            module: "KJV".into(),
            title: "King James Version".into(),
            book: Some(1),
            chapter: Some(1),
            verse: Some(1),
            headword: None,
            snippet: "In the beginning".into(),
        };
        assert_eq!(reference_text(&verse, &books, SearchScope::Kjv), "1:1");
        assert_eq!(group_label(&verse, &books, SearchScope::Kjv), "Genesis");
    }

    #[test]
    fn caption_includes_source() {
        let books = books();
        let verse = LibraryHit {
            kind: LibraryKind::Verse,
            module: "KJV".into(),
            title: "King James Version".into(),
            book: Some(1),
            chapter: Some(1),
            verse: Some(1),
            headword: None,
            snippet: "In the beginning".into(),
        };
        assert_eq!(caption(&verse, &books), "KJV · Genesis 1:1");
        let mhc = LibraryHit {
            kind: LibraryKind::Commentary,
            module: "MHC".into(),
            title: "Matthew Henry".into(),
            book: Some(1),
            chapter: Some(1),
            verse: Some(1),
            headword: None,
            snippet: "comment".into(),
        };
        assert_eq!(caption(&mhc, &books), "Matthew Henry · Genesis 1:1");
        let easton = LibraryHit {
            kind: LibraryKind::Dictionary,
            module: "Easton".into(),
            title: "Easton's Bible Dictionary".into(),
            book: None,
            chapter: None,
            verse: None,
            headword: Some("God".into()),
            snippet: "the true God".into(),
        };
        assert_eq!(caption(&easton, &books), "Easton's · God");
    }
}
