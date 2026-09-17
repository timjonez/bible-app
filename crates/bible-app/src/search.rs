use crate::nav::{self, Ref};
use bible_app_db::{Book, SearchHit};
use gtk::prelude::*;
use relm4::gtk;

pub fn status(query: &str, n: usize, limit: usize) -> String {
    let q = query.trim();
    if q.is_empty() {
        "Type a word or phrase from the King James Version.".into()
    } else if n == 0 {
        format!("No verses match \"{q}\".")
    } else if n >= limit {
        format!("First {limit} verses")
    } else if n == 1 {
        "1 verse".into()
    } else {
        format!("{n} verses")
    }
}

pub fn empty_description(query: &str) -> Option<&'static str> {
    if query.trim().is_empty() {
        Some("Try a short phrase, for example only begotten.")
    } else {
        None
    }
}

pub fn hit_ref(hit: &SearchHit) -> Ref {
    Ref {
        book: hit.book,
        chapter: hit.chapter,
        verse: hit.verse,
    }
}

pub fn row(hit: &SearchHit, books: &[Book]) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);

    let title = gtk::Label::new(Some(&nav::format_ref(books, hit_ref(hit))));
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
    row.set_tooltip_text(Some(&nav::format_ref(books, hit_ref(hit))));
    row
}

pub fn refill_list(list: &gtk::ListBox, hits: &[SearchHit], books: &[Book]) {
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

    #[test]
    fn status_empty_and_counts() {
        assert!(status("  ", 0, 200).contains("Type a word"));
        assert!(status("foo", 0, 200).contains("No verses"));
        assert_eq!(status("foo", 1, 200), "1 verse");
        assert_eq!(status("foo", 3, 200), "3 verses");
        assert_eq!(status("foo", 200, 200), "First 200 verses");
    }

    #[test]
    fn empty_description_prompt_only_for_blank_query() {
        assert_eq!(
            empty_description(""),
            Some("Try a short phrase, for example only begotten.")
        );
        assert_eq!(
            empty_description("  "),
            Some("Try a short phrase, for example only begotten.")
        );
        assert_eq!(empty_description("foo"), None);
    }
}
