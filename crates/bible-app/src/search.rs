use crate::nav::{self, Ref};
use bible_app_db::{Book, LibraryHit, LibraryKind, SearchScope};
use gtk::prelude::*;
use relm4::gtk;

pub fn placeholder(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::Kjv => "Search the KJV",
        SearchScope::Commentary => "Search MHC and TSK",
        SearchScope::Dictionaries => "Search dictionaries",
        SearchScope::Topics => "Search topics",
        SearchScope::All => "Search the library",
    }
}

pub fn status(query: &str, n: usize, limit: usize, scope: SearchScope) -> String {
    let q = query.trim();
    let (one, many) = nouns(scope);
    if q.is_empty() {
        empty_prompt(scope).into()
    } else if n == 0 {
        format!("No {many} match \"{q}\".")
    } else if n >= limit {
        format!("First {limit} {many}")
    } else if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn nouns(scope: SearchScope) -> (&'static str, &'static str) {
    match scope {
        SearchScope::Kjv => ("verse", "verses"),
        SearchScope::Commentary => ("comment", "comments"),
        SearchScope::Dictionaries => ("entry", "entries"),
        SearchScope::Topics => ("topic", "topics"),
        SearchScope::All => ("result", "results"),
    }
}

fn empty_prompt(scope: SearchScope) -> &'static str {
    match scope {
        SearchScope::Kjv => "Type a word or phrase from the King James Version.",
        SearchScope::Commentary => "Type a word or phrase from Matthew Henry or TSK.",
        SearchScope::Dictionaries => "Type a word or phrase from the dictionaries.",
        SearchScope::Topics => "Type a word or phrase from the topical works.",
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
        LibraryKind::Dictionary | LibraryKind::Topic => match hit.headword.as_deref() {
            Some(head) if !head.is_empty() => format!("{source} · {head}"),
            _ => source.to_string(),
        },
    }
}

pub fn row(hit: &LibraryHit, books: &[Book]) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);

    let title_text = caption(hit, books);
    let title = gtk::Label::new(Some(&title_text));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);

    let snippet = gtk::Label::new(Some(&hit.snippet));
    snippet.set_xalign(0.0);
    snippet.set_wrap(true);
    snippet.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    snippet.add_css_class("dim-label");
    snippet.set_max_width_chars(72);

    box_.append(&title);
    box_.append(&snippet);
    row.set_child(Some(&box_));
    row.set_activatable(true);
    row.set_tooltip_text(Some(&title_text));
    row
}

pub fn refill_list(list: &gtk::ListBox, hits: &[LibraryHit], books: &[Book]) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for hit in hits {
        list.append(&row(hit, books));
    }
    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
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
        assert!(status("  ", 0, 200, SearchScope::Kjv).contains("Type a word"));
        assert!(status("foo", 0, 200, SearchScope::Kjv).contains("No verses"));
        assert_eq!(status("foo", 1, 200, SearchScope::Kjv), "1 verse");
        assert_eq!(status("foo", 3, 200, SearchScope::Kjv), "3 verses");
        assert_eq!(
            status("foo", 200, 200, SearchScope::Kjv),
            "First 200 verses"
        );
        assert!(status("foo", 0, 200, SearchScope::Commentary).contains("No comments"));
        assert_eq!(status("foo", 1, 200, SearchScope::Dictionaries), "1 entry");
        assert_eq!(status("foo", 2, 200, SearchScope::Topics), "2 topics");
        assert_eq!(status("foo", 3, 200, SearchScope::All), "3 results");
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
