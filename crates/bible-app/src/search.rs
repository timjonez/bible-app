use crate::layout;
use crate::nav::{self, Ref};
use crate::workspace::TabId;
use bible_app_db::{
    Book, BookCount, CompiledQuery, LibraryHit, LibraryKind, LibraryPage, MatchMode, SearchCursor,
    SearchScope, StoredSpan, VerseFilter, WordMatch,
};
use gtk::gio;
use gtk::prelude::*;
use relm4::gtk;
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

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

#[cfg(test)]
pub fn emphasize(text: &str, tokens: &[String], word_match: WordMatch, match_case: bool) -> String {
    paint_bold(text, &query_ranges(text, tokens, word_match, match_case))
}

fn query_ranges(
    text: &str,
    tokens: &[String],
    word_match: WordMatch,
    match_case: bool,
) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return Vec::new();
    }
    if match_case && word_match == WordMatch::Exact {
        return merge_ranges(
            tokens
                .iter()
                .flat_map(|token| bible_app_db::case_match_ranges(text, token, false))
                .collect(),
        );
    }
    let haystack = if match_case {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    if haystack.len() != text.len() {
        return Vec::new();
    }
    let mut marks: Vec<(usize, usize)> = Vec::new();
    for token in tokens {
        let needle = if match_case {
            token.clone()
        } else {
            token.to_lowercase()
        };
        if needle.is_empty() {
            continue;
        }
        let partial = word_match.partial();
        let mut from = 0;
        while let Some(rel) = haystack[from..].find(&needle) {
            let pos = from + rel;
            let before = haystack[..pos].chars().next_back();
            let after = haystack[pos + needle.len()..].chars().next();
            let left_ok = before.is_none_or(|c| !c.is_ascii_alphanumeric());
            let right_ok = after.is_none_or(|c| !c.is_ascii_alphanumeric());
            if partial || (left_ok && right_ok) {
                let start = pos;
                let end = pos + needle.len();
                marks.push((start, end));
                from = end.max(pos + 1);
                continue;
            }
            from = pos + needle.len().max(1);
        }
    }
    merge_ranges(marks)
}

fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
fn paint_bold(text: &str, ranges: &[(usize, usize)]) -> String {
    paint(text, ranges, false)
}

/// Bold `ranges`. With `supplied`, KJV `[words]` lose their brackets and
/// are set in italics, as in the reader.
fn paint(text: &str, ranges: &[(usize, usize)], supplied: bool) -> String {
    let mut bold = vec![false; text.len()];
    let mut cursor = 0;
    for &(start, end) in ranges {
        if start >= end || end > text.len() || start < cursor {
            continue;
        }
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            continue;
        }
        bold[start..end].fill(true);
        cursor = end;
    }

    let mut out = String::new();
    let mut run = String::new();
    let mut state = (false, false);
    let mut italic = false;
    let flush = |out: &mut String, run: &mut String, (b, i): (bool, bool)| {
        if run.is_empty() {
            return;
        }
        let text = escape_markup(run);
        out.push_str(&match (b, i) {
            (true, true) => format!("<b><i>{text}</i></b>"),
            (true, false) => format!("<b>{text}</b>"),
            (false, true) => format!("<i>{text}</i>"),
            (false, false) => text,
        });
        run.clear();
    };
    for (i, ch) in text.char_indices() {
        if supplied && (ch == '[' || ch == ']') {
            italic = ch == '[';
            continue;
        }
        let now = (bold[i], italic);
        if now != state {
            flush(&mut out, &mut run, state);
            state = now;
        }
        run.push(ch);
    }
    flush(&mut out, &mut run, state);
    out
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut() {
            if start <= last.1 {
                last.1 = last.1.max(end);
                continue;
            }
        }
        merged.push((start, end));
    }
    merged
}

/// Collapsed list line and the only bold ranges that index it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkedLine {
    text: String,
    ranges: Vec<(usize, usize)>,
}

impl MarkedLine {
    /// `verse` is stored KJV text. `spans` are character offsets into that
    /// same string.
    pub fn from_verse(verse: &str, spans: &[StoredSpan]) -> Self {
        let (body, _) = layout::split_notes(verse);
        let mut text = String::new();
        let mut char_out = Vec::new();
        let mut chars = body.chars().peekable();
        while let Some(ch) = chars.next() {
            if !ch.is_whitespace() {
                let start = text.len();
                text.push(ch);
                char_out.push((start, text.len()));
                continue;
            }
            let mut run = 1;
            while chars.peek().is_some_and(|next| next.is_whitespace()) {
                chars.next();
                run += 1;
            }
            let drop_run = text.is_empty() || chars.peek().is_none();
            let start = text.len();
            if !drop_run {
                text.push(' ');
            }
            let end = text.len();
            char_out.extend(std::iter::repeat_n((start, end), run));
        }

        let body_len = char_out.len();
        let mut ranges = Vec::new();
        for span in spans {
            let start = span.start() as usize;
            let end = (span.end() as usize).min(body_len);
            if start >= body_len || start >= end {
                continue;
            }
            let mut lo = usize::MAX;
            let mut hi = 0;
            for &(out_start, out_end) in &char_out[start..end] {
                if out_start < out_end {
                    lo = lo.min(out_start);
                    hi = hi.max(out_end);
                }
            }
            if lo < hi {
                ranges.push(tighten(&text, lo, hi));
            }
        }
        Self {
            text,
            ranges: merge_ranges(ranges),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub fn markup(&self) -> String {
        paint_bold(&self.text, &self.ranges)
    }
}

/// Stored spans can carry the punctuation between words (`": and God"`).
/// Brackets mark supplied words and stay with them.
fn tighten(text: &str, lo: usize, hi: usize) -> (usize, usize) {
    let keep = |c: char| c.is_alphanumeric() || matches!(c, '[' | ']' | '\'');
    let span = &text[lo..hi];
    let Some(first) = span.find(keep) else {
        return (lo, hi);
    };
    let last = span.rfind(keep).unwrap_or(first);
    let last_len = span[last..].chars().next().map_or(0, char::len_utf8);
    (lo + first, lo + last + last_len)
}

#[derive(Debug, Clone)]
enum RowMarks {
    Query,
    Alignment(MarkedLine),
}

#[derive(Debug, Clone)]
pub struct ListedHit {
    library: LibraryHit,
    marks: RowMarks,
}

impl std::ops::Deref for ListedHit {
    type Target = LibraryHit;

    fn deref(&self) -> &LibraryHit {
        &self.library
    }
}

impl ListedHit {
    pub fn plain(hit: LibraryHit) -> Self {
        Self {
            library: hit,
            marks: RowMarks::Query,
        }
    }

    pub fn strongs(mut hit: LibraryHit, line: MarkedLine) -> Self {
        hit.snippet = line.text().to_string();
        Self {
            library: hit,
            marks: RowMarks::Alignment(line),
        }
    }

    pub fn line_markup(
        &self,
        tokens: &[String],
        word_match: WordMatch,
        match_case: bool,
    ) -> String {
        let supplied = self.library.kind == LibraryKind::Verse;
        match &self.marks {
            RowMarks::Query => {
                let text = &self.library.snippet;
                paint(
                    text,
                    &query_ranges(text, tokens, word_match, match_case),
                    supplied,
                )
            }
            RowMarks::Alignment(line) => paint(&line.text, &line.ranges, supplied),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn row(
    hit: &ListedHit,
    books: &[Book],
    scope: SearchScope,
    tokens: &[String],
    word_match: WordMatch,
    match_case: bool,
    id: TabId,
    sender: relm4::Sender<crate::app::Msg>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    box_.set_hexpand(true);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    box_.set_margin_top(6);
    box_.set_margin_bottom(6);

    let reference = reference_text(hit, books, scope);
    let title = gtk::Label::new(Some(&reference));
    title.set_xalign(0.0);
    title.set_yalign(0.0);
    title.set_width_chars(8);
    title.add_css_class("caption");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let snippet = gtk::Label::new(None);
    snippet.set_markup(&hit.line_markup(tokens, word_match, match_case));
    snippet.set_xalign(0.0);
    snippet.set_yalign(0.0);
    snippet.set_hexpand(true);
    snippet.set_wrap(true);
    snippet.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    snippet.set_lines(3);
    snippet.set_ellipsize(gtk::pango::EllipsizeMode::End);

    box_.append(&title);
    box_.append(&snippet);
    row.set_child(Some(&box_));
    row.set_activatable(true);
    row.set_tooltip_text(Some(&caption(hit, books)));
    if let Some(at) = hit_ref(hit) {
        attach_hit_menu(&row, id, at, sender);
    }
    row
}

fn attach_hit_menu(
    row: &gtk::ListBoxRow,
    id: TabId,
    at: Ref,
    sender: relm4::Sender<crate::app::Msg>,
) {
    let model = gio::Menu::new();
    model.append(Some("Open in a new tab"), Some("search.tab"));
    let menu = gtk::PopoverMenu::from_model(Some(&model));
    menu.set_parent(row);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);

    let group = gio::SimpleActionGroup::new();
    let tab = gio::SimpleAction::new("tab", None);
    tab.connect_activate(move |_, _| {
        sender.emit(crate::app::Msg::OpenHitTab(id, at));
    });
    group.add_action(&tab);
    menu.insert_action_group("search", Some(&group));

    let right = gtk::GestureClick::new();
    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.set_propagation_phase(gtk::PropagationPhase::Capture);
    let menu_click = menu.clone();
    right.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        menu_click.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        menu_click.popup();
    });
    row.add_controller(right);
    row.connect_destroy(move |_| {
        menu.unparent();
    });
}

pub fn refill_list(pane: &Pane, hits: &[ListedHit], books: &[Book], append: bool) {
    if !append {
        while let Some(child) = pane.list.row_at_index(0) {
            pane.list.remove(&child);
        }
        pane.groups.borrow_mut().clear();
    }
    for hit in hits {
        pane.groups
            .borrow_mut()
            .push(group_label(hit, books, pane.scope));
        pane.list.append(&row(
            hit,
            books,
            pane.scope,
            &pane.tokens,
            pane.words,
            pane.match_case,
            pane.tab,
            pane.sender.clone(),
        ));
    }
    if !append {
        if let Some(first) = pane.list.row_at_index(0) {
            pane.list.select_row(Some(&first));
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

    /// Passage range is a verse-search control. Commentary is not the
    /// current chapter, and dictionaries and topics are not passages.
    pub fn applies(scope: SearchScope) -> bool {
        matches!(
            scope,
            SearchScope::Kjv | SearchScope::Notes | SearchScope::All
        )
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
    AllBooks,
    Book(u8),
}

#[derive(Debug, Clone)]
pub enum Plan {
    Idle,
    Short,
    Goto(Ref),
    Ready(Box<Prepared>),
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub compiled: CompiledQuery,
    pub scope: SearchScope,
    pub filter: VerseFilter,
    pub chip: BookChip,
    pub db_path: std::path::PathBuf,
    pub user_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub hits: Vec<ListedHit>,
    pub total: i64,
    pub by_book: Vec<BookCount>,
    pub tokens: Vec<String>,
    pub strongs: Option<String>,
    pub chosen_book: Option<u8>,
    pub append: bool,
    pub next: SearchCursor,
}

#[allow(clippy::too_many_arguments)]
pub fn plan(
    query: &str,
    books: &[Book],
    current: Ref,
    mode: MatchMode,
    match_case: bool,
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
    if let Some(compiled) = bible_app_db::compile_query_with_case(text, mode, match_case) {
        if compiled.strongs.is_some() {
            return Plan::Ready(Box::new(Prepared {
                compiled,
                scope,
                filter: bounds(
                    effective_range(range, scope),
                    current,
                    slash_book,
                    slash_chapter,
                ),
                chip,
                db_path: std::path::PathBuf::new(),
                user_path: None,
            }));
        }
    }
    if slash_book.is_none() && query.chars().any(|c| c.is_ascii_digit()) {
        if let Some(at) = nav::parse_ref(query, books, current) {
            return Plan::Goto(at);
        }
    }
    let letters = text.chars().filter(|c| c.is_alphanumeric()).count();
    if letters < 2 {
        return Plan::Short;
    }
    let Some(compiled) = bible_app_db::compile_query_with_case(text, mode, match_case) else {
        return Plan::Idle;
    };
    Plan::Ready(Box::new(Prepared {
        compiled,
        scope,
        filter: bounds(
            effective_range(range, scope),
            current,
            slash_book,
            slash_chapter,
        ),
        chip,
        db_path: std::path::PathBuf::new(),
        user_path: None,
    }))
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

fn effective_range(range: SearchRange, scope: SearchScope) -> SearchRange {
    if SearchRange::applies(scope) {
        range
    } else {
        SearchRange::All
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

pub fn execute(prepared: &Prepared, cursor: Option<SearchCursor>, append: bool) -> Outcome {
    let cursor = cursor.unwrap_or_default();
    let empty = Outcome {
        hits: Vec::new(),
        total: 0,
        by_book: Vec::new(),
        tokens: prepared.compiled.tokens.clone(),
        strongs: prepared.compiled.strongs.clone(),
        chosen_book: None,
        append,
        next: cursor,
    };
    let Ok(conn) = open_reader(&prepared.db_path) else {
        return empty;
    };
    if let Some(code) = prepared.compiled.strongs.clone() {
        return strongs_outcome(&conn, prepared, &code, cursor, append);
    }
    if prepared.scope == SearchScope::Notes {
        return notes_outcome(prepared, cursor, append);
    }
    let chip = chip_arg(prepared.chip);
    let mut page = match bible_app_db::search_filtered(
        &conn,
        &prepared.compiled,
        prepared.scope,
        prepared.filter,
        chip,
        Some(cursor),
        if prepared.scope == SearchScope::All && prepared.user_path.is_some() {
            PAGE - PAGE / 5
        } else {
            PAGE
        },
    ) {
        Ok(page) => page,
        Err(_) => return empty,
    };
    if prepared.scope == SearchScope::All {
        append_notes(prepared, &mut page);
    }
    finish(page, prepared, chip, append)
}

fn strongs_outcome(
    conn: &Connection,
    prepared: &Prepared,
    code: &str,
    mut cursor: SearchCursor,
    append: bool,
) -> Outcome {
    let chip = chip_arg(prepared.chip);
    let Ok((hits, total, by_book)) = bible_app_db::strongs_page(
        conn,
        code,
        bible_app_db::StrongsWindow {
            book_min: prepared.filter.book_min,
            book_max: prepared.filter.book_max,
            book: prepared.filter.book.or(chip),
            chapter: prepared.filter.chapter,
            after: cursor.strongs_after,
            limit: PAGE,
            match_case: prepared.compiled.match_case,
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
            next: cursor,
        };
    };
    if let Some(last) = hits.last() {
        cursor.strongs_after = Some((last.book, last.chapter, last.verse));
    }
    let hits = hits
        .into_iter()
        .map(|hit| {
            let line = MarkedLine::from_verse(&hit.text, &hit.spans);
            ListedHit::strongs(
                LibraryHit {
                    kind: LibraryKind::Verse,
                    module: "KJV".into(),
                    title: "King James Version".into(),
                    book: Some(hit.book),
                    chapter: Some(hit.chapter),
                    verse: Some(hit.verse),
                    headword: None,
                    snippet: String::new(),
                },
                line,
            )
        })
        .collect();
    Outcome {
        hits,
        total,
        by_book,
        tokens: Vec::new(),
        strongs: Some(code.to_string()),
        chosen_book: prepared.filter.book.or(chip),
        append,
        next: cursor,
    }
}

fn notes_outcome(prepared: &Prepared, mut cursor: SearchCursor, append: bool) -> Outcome {
    let Some(path) = prepared.user_path.as_deref() else {
        return Outcome {
            hits: Vec::new(),
            total: 0,
            by_book: Vec::new(),
            tokens: prepared.compiled.tokens.clone(),
            strongs: None,
            chosen_book: None,
            append,
            next: cursor,
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
            next: cursor,
        };
    };
    let chip = chip_arg(prepared.chip);
    let book = prepared.filter.book.or(chip);
    let Ok(page) = crate::user_db::search_notes(
        &conn,
        &prepared.compiled,
        prepared.filter.book_min,
        prepared.filter.book_max,
        book,
        prepared.filter.chapter,
        cursor.notes,
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
            next: cursor,
        };
    };
    let by_book = page
        .by_book
        .iter()
        .map(|(id, count)| BookCount {
            book: *id,
            count: *count,
        })
        .collect::<Vec<_>>();
    cursor.notes += page.hits.len();
    Outcome {
        hits: note_hits(&page.hits)
            .into_iter()
            .map(ListedHit::plain)
            .collect(),
        total: page.total,
        by_book,
        tokens: prepared.compiled.tokens.clone(),
        strongs: None,
        chosen_book: book,
        append,
        next: cursor,
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
        &prepared.compiled,
        prepared.filter.book_min,
        prepared.filter.book_max,
        chip_arg(prepared.chip).or(prepared.filter.book),
        prepared.filter.chapter,
        page.next.notes,
        PAGE.saturating_sub(page.hits.len()),
    ) else {
        return;
    };
    page.total += found.total;
    page.next.notes += found.hits.len();
    page.hits.extend(note_hits(&found.hits));
}

fn note_hits(hits: &[crate::user_db::NoteHit]) -> Vec<LibraryHit> {
    hits.iter()
        .map(|hit| LibraryHit {
            kind: LibraryKind::Note,
            module: "Notes".into(),
            title: "Notes".into(),
            book: Some(hit.book),
            chapter: Some(hit.chapter),
            verse: Some(hit.verse),
            headword: None,
            snippet: hit.text.split_whitespace().collect::<Vec<_>>().join(" "),
        })
        .collect()
}

fn finish(page: LibraryPage, prepared: &Prepared, chosen: Option<u8>, append: bool) -> Outcome {
    Outcome {
        hits: page.hits.into_iter().map(ListedHit::plain).collect(),
        total: page.total,
        by_book: page.by_book,
        tokens: prepared.compiled.tokens.clone(),
        strongs: None,
        chosen_book: chosen.or(prepared.filter.book),
        append,
        next: page.next,
    }
}

fn chip_arg(chip: BookChip) -> Option<u8> {
    match chip {
        BookChip::Book(id) => Some(id),
        BookChip::AllBooks => None,
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

/// One window's search tab: the list, and the query it is showing.
pub struct Pane {
    pub root: gtk::Box,
    pub entry: gtk::SearchEntry,
    pub list: gtk::ListBox,
    pub chips: gtk::Box,
    pub scope_dd: gtk::DropDown,
    pub mode_dd: gtk::DropDown,
    pub case_check: gtk::CheckButton,
    pub range_dd: gtk::DropDown,
    pub status: gtk::Label,
    pub empty: gtk::Label,
    pub query: String,
    pub scope: SearchScope,
    pub mode: MatchMode,
    pub match_case: bool,
    pub words: WordMatch,
    pub range: SearchRange,
    pub chip: BookChip,
    pub hits: Vec<ListedHit>,
    pub total: i64,
    pub tokens: Vec<String>,
    pub strongs: Option<String>,
    pub chosen: Option<u8>,
    pub counts: Vec<BookCount>,
    pub cursor: SearchCursor,
    pub origin: Option<Ref>,
    pub gen: Rc<Cell<u64>>,
    pub hold: Rc<Cell<bool>>,
    pub groups: Rc<RefCell<Vec<String>>>,
    tab: TabId,
    sender: relm4::Sender<crate::app::Msg>,
}

/// A plain mouse wheel only reports vertical motion, which a horizontal-only
/// strip ignores.
fn scroll_sideways_with_wheel(scroll: &gtk::ScrolledWindow) {
    const WHEEL_STEP: f64 = 60.0;
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    let target = scroll.clone();
    wheel.connect_scroll(move |controller, dx, dy| {
        if dx != 0.0 || dy == 0.0 {
            return gtk::glib::Propagation::Proceed;
        }
        let step = match controller.unit() {
            gtk::gdk::ScrollUnit::Wheel => WHEEL_STEP,
            _ => 1.0,
        };
        let adj = target.hadjustment();
        adj.set_value(adj.value() + dy * step);
        gtk::glib::Propagation::Stop
    });
    scroll.add_controller(wheel);
}

pub fn build_pane(
    mode: MatchMode,
    match_case: bool,
    id: TabId,
    sender: relm4::Sender<crate::app::Msg>,
) -> Pane {
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some(placeholder(SearchScope::Kjv)));
    entry.set_tooltip_text(Some(
        "Click a hit, or press Enter, to open the verse beside Search. Right-click opens a new tab.",
    ));
    entry.set_hexpand(true);

    let scope_dd = gtk::DropDown::from_strings(&SearchScope::ALL.map(SearchScope::label));
    scope_dd.set_selected(SearchScope::Kjv.index());
    scope_dd.set_enable_search(false);
    scope_dd.set_tooltip_text(Some("Search in"));
    scope_dd.update_property(&[gtk::accessible::Property::Label("Search in")]);

    let mode_dd = gtk::DropDown::from_strings(&MatchMode::ALL.map(MatchMode::label));
    mode_dd.set_selected(mode.index());
    mode_dd.set_enable_search(false);
    mode_dd.set_tooltip_text(Some("Contains, all words, any word, or exact word"));
    mode_dd.update_property(&[gtk::accessible::Property::Label("Match")]);

    let case_check = gtk::CheckButton::with_label("Match case");
    case_check.set_active(match_case);
    case_check.set_tooltip_text(Some("Match uppercase and lowercase exactly"));

    let range_dd = gtk::DropDown::from_strings(&SearchRange::ALL.map(SearchRange::label));
    range_dd.set_selected(SearchRange::All.index());
    range_dd.set_enable_search(false);
    range_dd.set_tooltip_text(Some("Range"));
    range_dd.update_property(&[gtk::accessible::Property::Label("Range")]);

    let filters = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    filters.append(&entry);
    filters.append(&scope_dd);
    let modes = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    mode_dd.set_hexpand(true);
    range_dd.set_hexpand(true);
    modes.append(&mode_dd);
    modes.append(&case_check);
    modes.append(&range_dd);

    let status = gtk::Label::new(Some(&status("", 0, 0, SearchScope::Kjv)));
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.add_css_class("dim-label");

    let empty = gtk::Label::new(Some(empty_description("", SearchScope::Kjv).unwrap_or("")));
    empty.set_xalign(0.0);
    empty.set_wrap(true);
    empty.add_css_class("dim-label");

    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let chip_scroll = gtk::ScrolledWindow::new();
    chip_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    chip_scroll.set_propagate_natural_height(true);
    chip_scroll.add_css_class("chip-strip");
    chip_scroll.set_child(Some(&chips));
    scroll_sideways_with_wheel(&chip_scroll);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.set_accessible_role(gtk::AccessibleRole::List);
    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_hexpand(true);
    list_scroll.set_vexpand(true);
    list_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    list_scroll.set_child(Some(&list));

    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_start(12);
    root.set_margin_end(12);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.add_css_class("search-pane");
    root.append(&filters);
    root.append(&modes);
    root.append(&status);
    root.append(&empty);
    root.append(&chip_scroll);
    root.append(&list_scroll);

    Pane {
        root,
        entry,
        list,
        chips,
        scope_dd,
        mode_dd,
        case_check,
        range_dd,
        status,
        empty,
        query: String::new(),
        scope: SearchScope::Kjv,
        mode,
        match_case,
        words: mode.word_match(),
        range: SearchRange::All,
        chip: BookChip::AllBooks,
        hits: Vec::new(),
        total: 0,
        tokens: Vec::new(),
        strongs: None,
        chosen: None,
        counts: Vec::new(),
        cursor: SearchCursor::default(),
        origin: None,
        gen: Rc::new(Cell::new(0)),
        hold: Rc::new(Cell::new(false)),
        groups: Rc::new(RefCell::new(Vec::new())),
        tab: id,
        sender,
    }
}

impl Pane {
    pub fn show_status(&self, text: &str) {
        self.status.set_label(text);
    }

    pub fn sync_empty(&self) {
        match empty_description(&self.query, self.scope) {
            Some(text) => {
                self.empty.set_label(text);
                self.empty.set_visible(true);
            }
            None => self.empty.set_visible(false),
        }
    }

    pub fn sync_placeholder(&self) {
        self.entry
            .set_placeholder_text(Some(placeholder(self.scope)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bible_app_db::{LibraryHit, StoredSpan};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct SearchFixture {
        dir: std::path::PathBuf,
        db_path: std::path::PathBuf,
        user_path: std::path::PathBuf,
        books: Vec<Book>,
    }

    impl SearchFixture {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "bible-app-search-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&dir).unwrap();
            let db_path = dir.join("library.sqlite");
            let user_path = dir.join("user.sqlite");
            let conn = bible_app_db::open(&db_path).unwrap();
            conn.execute_batch(
                r#"
                INSERT INTO books (id, abbrev, name) VALUES
                    (5, 'De', 'Deuteronomy'), (23, 'Isa', 'Isaiah'),
                    (40, 'Mt', 'Matthew'), (44, 'Ac', 'Acts');
                INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                    (5, 33, 8, 'with thy holy one', 0),
                    (23, 1, 4, 'they have provoked the Holy One', 0),
                    (40, 4, 5, 'holy city', 0),
                    (44, 2, 27, 'Holy One to see corruption', 0),
                    (44, 3, 14, 'Holy One and the Just', 0),
                    (44, 13, 35, 'Holy One to see corruption', 0);
                INSERT INTO verse_words (book, chapter, verse, i, start, end_pos, strongs) VALUES
                    (40, 4, 5, 0, 0, 4, 'G40'),
                    (44, 2, 27, 0, 0, 4, 'G40'),
                    (44, 3, 14, 0, 0, 4, 'G40');
                INSERT INTO modules (id, kind, title, license) VALUES
                    ('MHC', 'commentary', 'Matthew Henry', 'public-domain'),
                    ('Easton', 'dictionary', 'Easton', 'public-domain'),
                    ('Nave', 'topic', 'Nave', 'public-domain');
                INSERT INTO resources (module, book, chapter, verse, text) VALUES
                    ('MHC', 5, 33, 8, 'the holy one in Deuteronomy'),
                    ('MHC', 44, 2, 27, 'the Holy One in Acts');
                INSERT INTO entries (module, i, headword, text) VALUES
                    ('Easton', 0, 'Holy One', 'a dictionary entry'),
                    ('Easton', 1, 'holy one', 'another dictionary entry'),
                    ('Nave', 0, 'Holy One', 'a topic'),
                    ('Nave', 1, 'holy one', 'another topic');
                "#,
            )
            .unwrap();
            bible_app_db::rebuild_verses_fts(&conn).unwrap();
            bible_app_db::rebuild_resources_fts(&conn).unwrap();
            bible_app_db::rebuild_entries_fts(&conn).unwrap();
            let books = bible_app_db::books(&conn).unwrap();
            drop(conn);

            let user = crate::user_db::open(&user_path).unwrap();
            crate::user_db::upsert_note(&user, at(5, 33, 8), "the holy one").unwrap();
            crate::user_db::upsert_note(&user, at(44, 2, 27), "the Holy One").unwrap();
            drop(user);
            Self {
                dir,
                db_path,
                user_path,
                books,
            }
        }

        fn prepare(
            &self,
            query: &str,
            origin: Ref,
            mode: MatchMode,
            scope: SearchScope,
            range: SearchRange,
            chip: BookChip,
        ) -> Prepared {
            self.prepare_with_case(query, origin, mode, false, scope, range, chip)
        }

        #[allow(clippy::too_many_arguments)]
        fn prepare_with_case(
            &self,
            query: &str,
            origin: Ref,
            mode: MatchMode,
            match_case: bool,
            scope: SearchScope,
            range: SearchRange,
            chip: BookChip,
        ) -> Prepared {
            let Plan::Ready(mut prepared) = plan(
                query,
                &self.books,
                origin,
                mode,
                match_case,
                range,
                chip,
                scope,
            ) else {
                panic!("expected a search for {query:?}");
            };
            prepared.db_path = self.db_path.clone();
            prepared.user_path = Some(self.user_path.clone());
            *prepared
        }

        fn search(
            &self,
            query: &str,
            scope: SearchScope,
            range: SearchRange,
            chip: BookChip,
        ) -> Outcome {
            let prepared = self.prepare(
                query,
                at(44, 2, 27),
                MatchMode::ExactWord,
                scope,
                range,
                chip,
            );
            execute(&prepared, None, false)
        }
    }

    impl Drop for SearchFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.db_path);
            let _ = std::fs::remove_file(&self.user_path);
            let _ = std::fs::remove_dir(&self.dir);
        }
    }

    fn at(book: u8, chapter: u8, verse: u8) -> Ref {
        Ref {
            book,
            chapter,
            verse,
        }
    }

    fn references(outcome: &Outcome) -> Vec<(u8, u8, u8)> {
        outcome
            .hits
            .iter()
            .filter_map(|hit| hit_ref(hit).map(|at| (at.book, at.chapter, at.verse)))
            .collect()
    }

    #[test]
    fn whole_bible_search_is_case_insensitive_and_not_limited_to_the_open_book() {
        let fixture = SearchFixture::new();
        let expected = vec![
            (5, 33, 8),
            (23, 1, 4),
            (44, 2, 27),
            (44, 3, 14),
            (44, 13, 35),
        ];
        for origin in [at(44, 2, 27), at(5, 33, 8)] {
            for mode in [MatchMode::ExactWord, MatchMode::Phrase] {
                for query in ["Holy One", "holy one", "HOLY ONE"] {
                    let prepared = fixture.prepare(
                        query,
                        origin,
                        mode,
                        SearchScope::Kjv,
                        SearchRange::All,
                        BookChip::AllBooks,
                    );
                    let outcome = execute(&prepared, None, false);
                    assert_eq!(references(&outcome), expected, "{query}, {mode:?}");
                    assert_eq!(outcome.total, 5);
                    assert_eq!(outcome.chosen_book, None);
                    assert_eq!(
                        outcome.by_book,
                        vec![
                            BookCount { book: 5, count: 1 },
                            BookCount { book: 23, count: 1 },
                            BookCount { book: 44, count: 3 },
                        ]
                    );
                }
            }
        }
    }

    #[test]
    fn match_case_controls_every_text_mode_and_their_counts() {
        let fixture = SearchFixture::new();
        for mode in MatchMode::ALL {
            for (query, match_case, expected) in [
                (
                    "Holy One",
                    false,
                    if mode == MatchMode::AnyWord { 6 } else { 5 },
                ),
                (
                    "holy one",
                    false,
                    if mode == MatchMode::AnyWord { 6 } else { 5 },
                ),
                ("Holy One", true, 4),
                (
                    "holy one",
                    true,
                    if mode == MatchMode::AnyWord { 2 } else { 1 },
                ),
            ] {
                let prepared = fixture.prepare_with_case(
                    query,
                    at(44, 2, 27),
                    mode,
                    match_case,
                    SearchScope::Kjv,
                    SearchRange::All,
                    BookChip::AllBooks,
                );
                let outcome = execute(&prepared, None, false);
                assert_eq!(outcome.total, expected, "{mode:?}, {query:?}, {match_case}");
                assert_eq!(outcome.hits.len() as i64, expected);
                if match_case && query == "Holy One" {
                    assert!(!references(&outcome).contains(&(5, 33, 8)));
                    assert!(references(&outcome).contains(&(44, 2, 27)));
                }
                if match_case && query == "holy one" {
                    assert!(references(&outcome).contains(&(5, 33, 8)));
                    assert!(!references(&outcome).contains(&(44, 2, 27)));
                }
            }
        }
    }

    #[test]
    fn match_case_near_query_is_planned_and_returns_results() {
        let fixture = SearchFixture::new();
        for query in ["Holy NEAR One", "Holy NEAR/3 One"] {
            let prepared = fixture.prepare_with_case(
                query,
                at(44, 2, 27),
                MatchMode::ExactWord,
                true,
                SearchScope::Kjv,
                SearchRange::All,
                BookChip::AllBooks,
            );
            let outcome = execute(&prepared, None, false);
            assert_eq!(outcome.total, 4, "{query}");
            assert!(references(&outcome).contains(&(44, 2, 27)));
            assert!(!references(&outcome).contains(&(5, 33, 8)));
        }
    }

    #[test]
    fn match_case_applies_to_commentary_entries_notes_and_all_library() {
        let fixture = SearchFixture::new();
        for mode in MatchMode::ALL {
            for scope in [
                SearchScope::Commentary,
                SearchScope::Dictionaries,
                SearchScope::Topics,
                SearchScope::Notes,
                SearchScope::All,
            ] {
                for (query, match_case) in [
                    ("Holy One", false),
                    ("holy one", false),
                    ("Holy One", true),
                    ("holy one", true),
                ] {
                    let prepared = fixture.prepare_with_case(
                        query,
                        at(44, 2, 27),
                        mode,
                        match_case,
                        scope,
                        SearchRange::All,
                        BookChip::AllBooks,
                    );
                    let outcome = execute(&prepared, None, false);
                    let expected_total = if scope == SearchScope::All {
                        if !match_case {
                            if mode == MatchMode::AnyWord {
                                14
                            } else {
                                13
                            }
                        } else if query == "Holy One" {
                            8
                        } else if mode == MatchMode::AnyWord {
                            6
                        } else {
                            5
                        }
                    } else if match_case {
                        1
                    } else {
                        2
                    };
                    assert_eq!(
                        outcome.total, expected_total,
                        "{scope:?}, {mode:?}, {query:?}, {match_case}"
                    );
                    let lower = outcome.hits.iter().any(|hit| {
                        hit.snippet.contains("holy one")
                            || hit.headword.as_deref() == Some("holy one")
                    });
                    let upper = outcome.hits.iter().any(|hit| {
                        hit.snippet.contains("Holy One")
                            || hit.headword.as_deref() == Some("Holy One")
                    });
                    assert_eq!(
                        (lower, upper),
                        if !match_case {
                            (true, true)
                        } else if query == "Holy One" {
                            (false, true)
                        } else {
                            (true, false)
                        },
                        "{scope:?}, {mode:?}, {query:?}, {match_case}: {:?}",
                        outcome.hits
                    );
                }
            }
        }
    }

    #[test]
    fn match_case_applies_to_strongs_codes_and_reference_queries() {
        let fixture = SearchFixture::new();
        for (query, match_case, count) in [
            ("G40", false, 3),
            ("g40", false, 3),
            ("G40", true, 3),
            ("g40", true, 0),
        ] {
            let prepared = fixture.prepare_with_case(
                query,
                at(44, 2, 27),
                MatchMode::ExactWord,
                match_case,
                SearchScope::Kjv,
                SearchRange::All,
                BookChip::AllBooks,
            );
            let outcome = execute(&prepared, None, false);
            assert_eq!(outcome.total, count, "{query}, {match_case}");
            assert_eq!(outcome.hits.len() as i64, count);
        }
        let origin = at(44, 2, 27);
        for (query, match_case, expected) in [
            ("Deuteronomy 33:8", true, true),
            ("deuteronomy 33:8", true, true),
            ("DEUTERONOMY 33:8", true, true),
            ("deuteronomy 33:8", false, true),
            ("De 33:8", true, true),
            ("de 33:8", true, true),
            ("33:8", true, true),
        ] {
            let planned = plan(
                query,
                &fixture.books,
                origin,
                MatchMode::ExactWord,
                match_case,
                SearchRange::All,
                BookChip::AllBooks,
                SearchScope::Kjv,
            );
            assert_eq!(matches!(planned, Plan::Goto(_)), expected, "{query}");
        }
        for (query, expected) in [("Deuteronomy/holy one", 1), ("Deuteronomy/Holy One", 0)] {
            let prepared = fixture.prepare_with_case(
                query,
                origin,
                MatchMode::ExactWord,
                true,
                SearchScope::Kjv,
                SearchRange::All,
                BookChip::AllBooks,
            );
            assert_eq!(execute(&prepared, None, false).total, expected, "{query}");
        }
        let prepared = fixture.prepare_with_case(
            "deuteronomy/holy one",
            origin,
            MatchMode::ExactWord,
            true,
            SearchScope::Kjv,
            SearchRange::All,
            BookChip::AllBooks,
        );
        assert_eq!(execute(&prepared, None, false).total, 1);
    }

    #[test]
    #[ignore = "requires BIBLE_APP_DB or an unpacked data/bible-app.sqlite"]
    fn shipped_holy_one_search_includes_deuteronomy_and_acts() {
        let path = std::env::var_os("BIBLE_APP_DB")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../data/bible-app.sqlite");
                path.is_file().then_some(path)
            })
            .expect("set BIBLE_APP_DB to an unpacked bible-app.sqlite");
        let conn = open_reader(&path).unwrap();
        let books = bible_app_db::books(&conn).unwrap();
        let mut previous = None;
        for query in ["Holy One", "holy one", "HOLY ONE"] {
            let Plan::Ready(mut prepared) = plan(
                query,
                &books,
                at(44, 2, 27),
                MatchMode::ExactWord,
                false,
                SearchRange::All,
                BookChip::AllBooks,
                SearchScope::Kjv,
            ) else {
                panic!("expected a search");
            };
            prepared.db_path = path.clone();
            let outcome = execute(&prepared, None, false);
            let refs = references(&outcome);
            assert!(refs.contains(&(5, 33, 8)), "{query}: {refs:?}");
            assert!(refs.contains(&(44, 2, 27)), "{query}: {refs:?}");
            assert!(outcome.total > 3);
            assert_eq!(outcome.chosen_book, None);
            if let Some(expected) = &previous {
                assert_eq!(&refs, expected);
            }
            previous = Some(refs);
        }
        for (query, expected, excluded) in [
            ("Holy One", (44, 2, 27), (5, 33, 8)),
            ("holy one", (5, 33, 8), (44, 2, 27)),
        ] {
            let Plan::Ready(mut prepared) = plan(
                query,
                &books,
                at(44, 2, 27),
                MatchMode::ExactWord,
                true,
                SearchRange::All,
                BookChip::AllBooks,
                SearchScope::Kjv,
            ) else {
                panic!("expected a case-sensitive search");
            };
            prepared.db_path = path.clone();
            let outcome = execute(&prepared, None, false);
            let refs = references(&outcome);
            assert!(refs.contains(&expected), "{query}: {refs:?}");
            assert!(!refs.contains(&excluded), "{query}: {refs:?}");
        }

        for match_case in [false, true] {
            let Plan::Ready(mut prepared) = plan(
                "Holy One",
                &books,
                at(44, 2, 27),
                MatchMode::ExactWord,
                match_case,
                SearchRange::All,
                BookChip::AllBooks,
                SearchScope::All,
            ) else {
                panic!("expected a whole-library search");
            };
            prepared.db_path = path.clone();
            let mut cursor = None;
            let mut seen = std::collections::HashSet::new();
            loop {
                let page = execute(&prepared, cursor, cursor.is_some());
                for hit in &page.hits {
                    let key = format!(
                        "{:?} {} {:?} {:?} {:?} {:?} {}",
                        hit.kind,
                        hit.module,
                        hit.book,
                        hit.chapter,
                        hit.verse,
                        hit.headword,
                        hit.snippet
                    );
                    assert!(seen.insert(key.clone()), "repeated {key}");
                }
                if seen.len() as i64 == page.total {
                    break;
                }
                assert!(
                    !page.hits.is_empty(),
                    "stopped at {} of {}",
                    seen.len(),
                    page.total
                );
                cursor = Some(page.next);
            }
        }
    }

    #[test]
    fn explicit_ranges_and_book_chips_still_narrow_the_results() {
        let fixture = SearchFixture::new();
        for (range, chip, expected) in [
            (
                SearchRange::ThisBook,
                BookChip::AllBooks,
                vec![(44, 2, 27), (44, 3, 14), (44, 13, 35)],
            ),
            (
                SearchRange::ThisChapter,
                BookChip::AllBooks,
                vec![(44, 2, 27)],
            ),
            (
                SearchRange::Ot,
                BookChip::AllBooks,
                vec![(5, 33, 8), (23, 1, 4)],
            ),
            (
                SearchRange::Nt,
                BookChip::AllBooks,
                vec![(44, 2, 27), (44, 3, 14), (44, 13, 35)],
            ),
            (SearchRange::All, BookChip::Book(5), vec![(5, 33, 8)]),
        ] {
            let outcome = fixture.search("Holy One", SearchScope::Kjv, range, chip);
            assert_eq!(references(&outcome), expected, "{range:?}, {chip:?}");
            assert_eq!(outcome.total, expected.len() as i64);
        }
        let selected = fixture.search(
            "Holy One",
            SearchScope::Kjv,
            SearchRange::All,
            BookChip::Book(44),
        );
        assert_eq!(selected.total, 3);
        assert_eq!(selected.chosen_book, Some(44));
        assert_eq!(selected.by_book.len(), 3);
        let all = fixture.search(
            "Holy One",
            SearchScope::Kjv,
            SearchRange::All,
            BookChip::AllBooks,
        );
        assert_eq!(all.total, 5);
        assert_eq!(all.chosen_book, None);

        let slash = fixture.search(
            "Deuteronomy/Holy One",
            SearchScope::Kjv,
            SearchRange::All,
            BookChip::AllBooks,
        );
        assert_eq!(references(&slash), vec![(5, 33, 8)]);
    }

    #[test]
    fn whole_bible_search_pages_continue_into_later_books() {
        let fixture = SearchFixture::new();
        let conn = Connection::open(&fixture.db_path).unwrap();
        for verse in 1..=85 {
            conn.execute(
                "INSERT INTO verses (book, chapter, verse, text) VALUES (5, 1, ?1, 'holy one')",
                [verse],
            )
            .unwrap();
        }
        bible_app_db::rebuild_verses_fts(&conn).unwrap();
        drop(conn);

        let prepared = fixture.prepare(
            "Holy One",
            at(44, 2, 27),
            MatchMode::ExactWord,
            SearchScope::Kjv,
            SearchRange::All,
            BookChip::AllBooks,
        );
        let first = execute(&prepared, None, false);
        assert_eq!(first.hits.len(), PAGE);
        assert_eq!(first.total, 90);
        assert_eq!(first.chosen_book, None);
        let second = execute(&prepared, Some(first.next), true);
        assert!(second.append);
        assert_eq!(second.total, 90);
        assert_eq!(second.hits.len(), 10);
        assert!(references(&second).contains(&(44, 2, 27)));

        let lower = fixture.prepare_with_case(
            "holy one",
            at(44, 2, 27),
            MatchMode::ExactWord,
            true,
            SearchScope::Kjv,
            SearchRange::All,
            BookChip::AllBooks,
        );
        let first = execute(&lower, None, false);
        assert_eq!(first.hits.len(), PAGE);
        assert_eq!(first.total, 86);
        let second = execute(&lower, Some(first.next), true);
        assert_eq!(second.hits.len(), 6);
        assert_eq!(second.total, 86);
        assert!(references(&second).contains(&(5, 33, 8)));
        assert!(!references(&second).contains(&(44, 2, 27)));
    }

    #[test]
    fn all_sources_and_individual_scopes_page_without_duplicates_or_gaps() {
        use std::collections::HashSet;

        let fixture = SearchFixture::new();
        let conn = Connection::open(&fixture.db_path).unwrap();
        conn.execute_batch(
            "INSERT INTO modules (id, kind, title, license) VALUES
                ('TSK', 'commentary', 'TSK', 'public-domain'),
                ('Smith', 'dictionary', 'Smith', 'public-domain');
             BEGIN",
        )
        .unwrap();
        for verse in 1..=85 {
            conn.execute(
                "INSERT INTO verses (book, chapter, verse, text) VALUES (5, 1, ?1, 'CasePage verse')",
                [verse],
            )
            .unwrap();
            for module in ["MHC", "TSK"] {
                conn.execute(
                    "INSERT INTO resources (module, book, chapter, verse, text)
                     VALUES (?1, 5, 1, ?2, 'CasePage commentary')",
                    rusqlite::params![module, verse],
                )
                .unwrap();
            }
            for module in ["Easton", "Smith", "Nave"] {
                conn.execute(
                    "INSERT INTO entries (module, i, headword, text)
                     VALUES (?1, ?2, ?3, 'CasePage entry')",
                    rusqlite::params![module, verse + 1000, format!("Entry {verse:03}")],
                )
                .unwrap();
            }
        }
        conn.execute_batch("COMMIT").unwrap();
        bible_app_db::rebuild_verses_fts(&conn).unwrap();
        bible_app_db::rebuild_resources_fts(&conn).unwrap();
        bible_app_db::rebuild_entries_fts(&conn).unwrap();
        drop(conn);
        let user = crate::user_db::open(&fixture.user_path).unwrap();
        user.execute_batch("BEGIN").unwrap();
        for verse in 1..=85 {
            crate::user_db::upsert_note(&user, at(5, 1, verse), "CasePage note").unwrap();
        }
        user.execute_batch("COMMIT").unwrap();
        drop(user);

        for (scope, total) in [
            (SearchScope::All, 595),
            (SearchScope::Kjv, 85),
            (SearchScope::Commentary, 170),
            (SearchScope::Dictionaries, 170),
            (SearchScope::Topics, 85),
            (SearchScope::Notes, 85),
        ] {
            for match_case in [false, true] {
                let prepared = fixture.prepare_with_case(
                    "CasePage",
                    at(44, 2, 27),
                    MatchMode::ExactWord,
                    match_case,
                    scope,
                    SearchRange::All,
                    BookChip::AllBooks,
                );
                let mut cursor = None;
                let mut seen = HashSet::new();
                while seen.len() < total {
                    let page = execute(&prepared, cursor, cursor.is_some());
                    assert_eq!(page.total, total as i64, "{scope:?}");
                    assert!(!page.hits.is_empty(), "{scope:?} stopped at {}", seen.len());
                    assert!(page.hits.len() <= PAGE);
                    for hit in &page.hits {
                        let key = format!(
                            "{:?} {} {:?} {:?} {:?} {:?}",
                            hit.kind, hit.module, hit.book, hit.chapter, hit.verse, hit.headword
                        );
                        assert!(seen.insert(key.clone()), "{scope:?} repeated {key}");
                    }
                    assert_ne!(cursor, Some(page.next), "{scope:?} cursor did not advance");
                    cursor = Some(page.next);
                }
                assert_eq!(seen.len(), total, "{scope:?}");
                assert!(
                    execute(&prepared, cursor, true).hits.is_empty(),
                    "{scope:?}"
                );
            }
        }
    }

    #[test]
    fn all_scope_book_chip_counts_only_results_in_that_book() {
        let fixture = SearchFixture::new();
        let selected = fixture.search(
            "Holy One",
            SearchScope::All,
            SearchRange::All,
            BookChip::Book(44),
        );
        assert_eq!(selected.total, 5);
        assert_eq!(selected.hits.len(), 5);
        assert!(selected.hits.iter().all(|hit| hit.book == Some(44)));
        assert!(!selected
            .hits
            .iter()
            .any(|hit| { matches!(hit.kind, LibraryKind::Dictionary | LibraryKind::Topic) }));
    }

    #[test]
    fn strongs_and_notes_search_across_books_until_a_chip_is_selected() {
        let fixture = SearchFixture::new();
        for (query, scope, all_refs, acts_refs) in [
            (
                "G40",
                SearchScope::Kjv,
                vec![(40, 4, 5), (44, 2, 27), (44, 3, 14)],
                vec![(44, 2, 27), (44, 3, 14)],
            ),
            (
                "Holy One",
                SearchScope::Notes,
                vec![(5, 33, 8), (44, 2, 27)],
                vec![(44, 2, 27)],
            ),
        ] {
            let all = fixture.search(query, scope, SearchRange::All, BookChip::AllBooks);
            assert_eq!(references(&all), all_refs);
            assert_eq!(all.total, all_refs.len() as i64);
            assert_eq!(all.chosen_book, None);
            let acts = fixture.search(query, scope, SearchRange::All, BookChip::Book(44));
            assert_eq!(references(&acts), acts_refs);
            assert_eq!(acts.total, acts_refs.len() as i64);
            assert_eq!(acts.chosen_book, Some(44));
            assert_eq!(acts.by_book.len(), 2);
        }
    }

    #[test]
    fn commentary_and_whole_library_search_do_not_narrow_to_the_open_book() {
        let fixture = SearchFixture::new();
        let comments = fixture.search(
            "Holy One",
            SearchScope::Commentary,
            SearchRange::All,
            BookChip::AllBooks,
        );
        assert_eq!(references(&comments), vec![(5, 33, 8), (44, 2, 27)]);
        assert_eq!(comments.chosen_book, None);

        let library = fixture.search(
            "Holy One",
            SearchScope::All,
            SearchRange::All,
            BookChip::AllBooks,
        );
        for kind in [
            LibraryKind::Verse,
            LibraryKind::Commentary,
            LibraryKind::Note,
        ] {
            assert!(library
                .hits
                .iter()
                .any(|hit| hit.kind == kind && hit.book == Some(5)));
            assert!(library
                .hits
                .iter()
                .any(|hit| hit.kind == kind && hit.book == Some(44)));
        }
        assert!(library
            .hits
            .iter()
            .any(|hit| hit.kind == LibraryKind::Dictionary));
        assert_eq!(library.chosen_book, None);
    }

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
        for query in ["john 3:16", "JOHN 3:16", "John 3:16"] {
            assert!(
                matches!(
                    plan(
                        query, &books, current, MatchMode::ExactWord, true,
                        SearchRange::All, BookChip::AllBooks, SearchScope::Kjv,
                    ),
                    Plan::Goto(at) if (at.book, at.chapter, at.verse) == (43, 3, 16)
                ),
                "{query}"
            );
        }
        match plan(
            "John 3:16",
            &books,
            current,
            MatchMode::Phrase,
            false,
            SearchRange::All,
            BookChip::AllBooks,
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
            false,
            SearchRange::All,
            BookChip::AllBooks,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => assert_eq!(p.compiled.strongs.as_deref(), Some("H430")),
            other => panic!("expected strongs, got {other:?}"),
        }
        match plan(
            "H430",
            &books,
            current,
            MatchMode::Phrase,
            false,
            SearchRange::All,
            BookChip::AllBooks,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => {
                assert_eq!(p.compiled.strongs.as_deref(), Some("H430"));
                assert_eq!(p.chip, BookChip::AllBooks);
                assert_eq!(p.filter.book, None);
            }
            other => panic!("expected strongs, got {other:?}"),
        }
        match plan(
            "John/light",
            &books,
            current,
            MatchMode::Phrase,
            false,
            SearchRange::All,
            BookChip::AllBooks,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => {
                assert_eq!(p.compiled.fts, "light*");
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
            false,
            SearchRange::All,
            BookChip::AllBooks,
            SearchScope::Kjv,
        ) {
            Plan::Ready(p) => {
                assert_eq!(p.filter.book, Some(43));
                assert_eq!(p.filter.chapter, Some(3));
                assert_eq!(p.compiled.fts, "born*");
            }
            other => panic!("expected chapter search, got {other:?}"),
        }
        assert!(matches!(
            plan(
                "a",
                &books,
                current,
                MatchMode::Phrase,
                false,
                SearchRange::All,
                BookChip::AllBooks,
                SearchScope::Kjv,
            ),
            Plan::Short
        ));
    }

    #[test]
    fn emphasize_bolds_the_match_and_escapes() {
        assert_eq!(
            emphasize(
                "the only begotten Son",
                &["begotten".into()],
                WordMatch::Prefix,
                false
            ),
            "the only <b>begotten</b> Son"
        );
        assert_eq!(
            emphasize(
                "the heaven and the earth.",
                &["eart".into()],
                WordMatch::Prefix,
                false
            ),
            "the heaven and the <b>eart</b>h."
        );
        assert_eq!(
            emphasize(
                "the heaven and the earth.",
                &["ear".into()],
                WordMatch::Exact,
                false
            ),
            "the heaven and the earth."
        );
        assert_eq!(
            emphasize(
                "hear thou in heaven",
                &["ear".into()],
                WordMatch::Prefix,
                false
            ),
            "h<b>ear</b> thou in heaven"
        );
        assert!(emphasize("a < b & c", &["b".into()], WordMatch::Prefix, false).contains("&lt;"));
    }

    #[test]
    fn emphasize_respects_match_case() {
        let tokens = ["Holy".into()];
        assert_eq!(
            emphasize("holy Holy HOLY", &tokens, WordMatch::Exact, true),
            "holy <b>Holy</b> HOLY"
        );
        assert_eq!(
            emphasize("holy Holy HOLY", &tokens, WordMatch::Exact, false),
            "<b>holy</b> <b>Holy</b> <b>HOLY</b>"
        );
        assert_eq!(
            emphasize(
                "Elohim Elōhim Elo\u{304}him elōhim",
                &["Elohim".into()],
                WordMatch::Exact,
                true,
            ),
            "<b>Elohim</b> <b>Elōhim</b> <b>Elo\u{304}him</b> elōhim"
        );
    }

    fn verse_hit(snippet: &str) -> LibraryHit {
        LibraryHit {
            kind: LibraryKind::Verse,
            module: "KJV".into(),
            title: "King James Version".into(),
            book: Some(1),
            chapter: Some(1),
            verse: Some(1),
            headword: None,
            snippet: snippet.into(),
        }
    }

    #[test]
    fn alignment_bolds_the_phrase_and_leaves_the_decoy() {
        let verse = "sister, and a sister";
        let phrase = StoredSpan::parse(verse, 12, 20).unwrap();
        let line = MarkedLine::from_verse(verse, &[phrase]);
        assert_eq!(line.text(), "sister, and a sister");
        assert_eq!(line.markup(), "sister, and <b>a sister</b>");
    }

    #[test]
    fn alignment_bolds_brackets() {
        let verse = "the  [God] of heaven";
        let god = StoredSpan::parse(verse, 5, 10).unwrap();
        let line = MarkedLine::from_verse(verse, &[god]);
        assert_eq!(line.markup(), "the <b>[God]</b> of heaven");
    }

    #[test]
    fn alignment_drops_a_trailing_note() {
        let verse = "God created.{a note}";
        let word = StoredSpan::parse(verse, 0, 3).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[word]).markup(),
            "<b>God</b> created."
        );
    }

    #[test]
    fn alignment_escapes_markup_characters() {
        let verse = "God & Father";
        let word = StoredSpan::parse(verse, 0, 3).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[word]).markup(),
            "<b>God</b> &amp; Father"
        );
        let amp = StoredSpan::parse(verse, 4, 5).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[amp]).markup(),
            "God <b>&amp;</b> Father"
        );
    }

    #[test]
    fn leading_space_does_not_slide_the_bold() {
        let verse = " In those days";
        let word = StoredSpan::parse(verse, 1, 3).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[word]).markup(),
            "<b>In</b> those days"
        );
    }

    #[test]
    fn collapsed_whitespace_inside_a_span_stays_marked() {
        let verse = "a  b";
        let spaces = StoredSpan::parse(verse, 1, 3).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[spaces]).markup(),
            "a<b> </b>b"
        );
        let second = StoredSpan::parse(verse, 2, 3).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[second]).markup(),
            "a<b> </b>b"
        );
    }

    #[test]
    fn span_inside_a_note_adds_no_bold() {
        let verse = "God created.{a note}";
        let note = StoredSpan::parse(verse, 13, 18).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[note]).markup(),
            "God created."
        );
    }

    #[test]
    fn span_that_runs_into_a_note_is_clipped() {
        let verse = "God created.{a note}";
        let clipped = StoredSpan::parse(verse, 8, 20).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[clipped]).markup(),
            "God crea<b>ted</b>."
        );
    }

    #[test]
    fn alignment_trims_punctuation_at_the_span_edges() {
        let verse = "that [it was] good: and God divided";
        let god = StoredSpan::parse(verse, 18, 27).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[god]).markup(),
            "that [it was] good: <b>and God</b> divided"
        );
        let supplied = StoredSpan::parse(verse, 4, 14).unwrap();
        assert_eq!(
            MarkedLine::from_verse(verse, &[supplied]).markup(),
            "that <b>[it was]</b> good: and God divided"
        );
    }

    #[test]
    fn from_verse_matches_note_split_and_whitespace_collapse() {
        let verse = "  In the  beginning.{a note}";
        let line = MarkedLine::from_verse(verse, &[]);
        let (body, _) = layout::split_notes(verse);
        let expected = body.split_whitespace().collect::<Vec<_>>().join(" ");
        assert_eq!(expected, "In the beginning.");
        assert_eq!(line.text(), expected);
    }

    #[test]
    fn plain_hit_emphasizes_the_token() {
        let hit = ListedHit::plain(verse_hit("the only begotten Son"));
        assert_eq!(
            hit.line_markup(&["begotten".into()], WordMatch::Prefix, false),
            "the only <b>begotten</b> Son"
        );
    }

    #[test]
    fn alignment_hit_ignores_tokens() {
        let verse = "sister, and a sister";
        let phrase = StoredSpan::parse(verse, 12, 20).unwrap();
        let line = MarkedLine::from_verse(verse, &[phrase]);
        let hit = ListedHit::strongs(verse_hit(verse), line);
        assert_eq!(hit.snippet, "sister, and a sister");
        assert_eq!(
            hit.line_markup(&["sister".into()], WordMatch::Exact, false),
            "sister, and <b>a sister</b>"
        );
    }

    #[test]
    fn empty_alignment_does_not_fall_through_to_tokens() {
        let verse = "sister, and a sister";
        let line = MarkedLine::from_verse(verse, &[]);
        let hit = ListedHit::strongs(verse_hit(verse), line);
        assert_eq!(
            hit.line_markup(&["sister".into()], WordMatch::Exact, false),
            "sister, and a sister"
        );
    }

    #[test]
    fn shipped_g79_rows_bold_the_aligned_span() {
        let Some(path) = std::env::var_os("BIBLE_APP_DB")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_file())
            .or_else(|| {
                let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../data/bible-app.sqlite");
                path.is_file().then_some(path)
            })
        else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = bible_app_db::open(&path).unwrap();
        let hits = bible_app_db::strongs_occurrences(&conn, "G79", 10_000).unwrap();
        let markup = |book, chapter, verse| {
            let hit = hits
                .iter()
                .find(|hit| (hit.book, hit.chapter, hit.verse) == (book, chapter, verse))
                .unwrap_or_else(|| panic!("missing {book} {chapter}:{verse}"));
            MarkedLine::from_verse(&hit.text, &hit.spans).markup()
        };
        assert_eq!(
            markup(40, 12, 50),
            "For whosoever shall do the will of my Father which is in heaven, the same is my brother, and <b>sister</b>, and mother."
        );
        assert_eq!(
            markup(42, 10, 39),
            "And she had <b>a sister</b> called Mary, which also sat at Jesus' feet, and heard his word."
        );
        assert_eq!(
            markup(40, 13, 56),
            "And his <b>sisters</b>, are they not all with us? Whence then hath this [man] all these things?"
        );
    }

    #[test]
    fn supplied_words_are_italic_without_brackets() {
        let text = "darkness [was] upon";
        assert_eq!(paint(text, &[], true), "darkness <i>was</i> upon");
        assert_eq!(paint(text, &[], false), "darkness [was] upon");
        assert_eq!(
            paint("that [it was] good", &[(0, 8)], true),
            "<b>that </b><b><i>it</i></b><i> was</i> good"
        );
        assert_eq!(paint("was] upon [the", &[], true), "was upon <i>the</i>");
    }

    #[test]
    fn verse_rows_italicize_supplied_words_and_others_keep_brackets() {
        let mut verse = verse_hit("darkness [was] upon the deep");
        assert_eq!(
            ListedHit::plain(verse.clone()).line_markup(&["deep".into()], WordMatch::Exact, false),
            "darkness <i>was</i> upon the <b>deep</b>"
        );
        verse.kind = LibraryKind::Commentary;
        assert_eq!(
            ListedHit::plain(verse).line_markup(&["deep".into()], WordMatch::Exact, false),
            "darkness [was] upon the <b>deep</b>"
        );
    }

    #[test]
    fn paint_bold_skips_ranges_that_are_empty_or_misaligned() {
        assert_eq!(paint_bold("ab", &[(0, 0), (1, 5), (0, 1)]), "<b>a</b>b");
        assert_eq!(paint_bold("é", &[(1, 2)]), "é");
        assert_eq!(paint_bold("God &", &[(0, 3)]), "<b>God</b> &amp;");
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
    fn commentary_does_not_use_the_verse_range() {
        assert!(SearchRange::applies(SearchScope::Kjv));
        assert!(SearchRange::applies(SearchScope::Notes));
        assert!(SearchRange::applies(SearchScope::All));
        assert!(!SearchRange::applies(SearchScope::Commentary));
        assert!(!SearchRange::applies(SearchScope::Dictionaries));
        assert!(!SearchRange::applies(SearchScope::Topics));
        let books = books();
        let current = Ref {
            book: 4,
            chapter: 1,
            verse: 1,
        };
        match plan(
            "moses",
            &books,
            current,
            MatchMode::Phrase,
            false,
            SearchRange::ThisChapter,
            BookChip::AllBooks,
            SearchScope::Commentary,
        ) {
            Plan::Ready(prepared) => {
                assert_eq!(prepared.filter, VerseFilter::all());
            }
            other => panic!("expected a commentary search, got {other:?}"),
        }
        match plan(
            "moses",
            &books,
            current,
            MatchMode::Phrase,
            false,
            SearchRange::ThisChapter,
            BookChip::AllBooks,
            SearchScope::Kjv,
        ) {
            Plan::Ready(prepared) => {
                assert_eq!(prepared.filter.book, Some(4));
                assert_eq!(prepared.filter.chapter, Some(1));
            }
            other => panic!("expected a verse search, got {other:?}"),
        }
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
