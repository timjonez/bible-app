use crate::layout;
use crate::nav::{self, Ref};
use adw::prelude::*;
use bible_app_db::{Book, Occurrence};
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct OccWidgets {
    pub window: adw::ApplicationWindow,
    pub title: adw::WindowTitle,
    pub status: gtk::Label,
    pub list: gtk::ListBox,
    pub hits: Vec<Occurrence>,
}

pub fn open(sender: relm4::Sender<super::app::Msg>) -> OccWidgets {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("Strong's in the KJV"));
    window.set_default_size(520, 720);

    let title = adw::WindowTitle::new("Strong's in the KJV", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let status = gtk::Label::new(None);
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.add_css_class("dim-label");

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.set_accessible_role(gtk::AccessibleRole::List);
    let send = sender.clone();
    list.connect_row_activated(move |_, row| {
        send.emit(super::app::Msg::OpenOccurrenceHit(row.index()));
    });

    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_hexpand(true);
    list_scroll.set_vexpand(true);
    list_scroll.set_child(Some(&list));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    body.append(&status);
    body.append(&list_scroll);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));
    window.set_content(Some(&toolbar));

    window.connect_close_request(move |_| {
        sender.emit(super::app::Msg::OccurrencesClosed);
        glib::Propagation::Proceed
    });
    window.present();
    OccWidgets {
        window,
        title,
        status,
        list,
        hits: Vec::new(),
    }
}

pub fn fill(widgets: &mut OccWidgets, conn: &Connection, books: &[Book], code: &str) {
    let heading = format!("{code} in the KJV");
    widgets.title.set_title(&heading);
    widgets.window.set_title(Some(&heading));

    let total = bible_app_db::strongs_occurrence_count(conn, code).unwrap_or(0);
    widgets.hits = bible_app_db::strongs_occurrences(conn, code, bible_app_db::DEFAULT_LIMIT)
        .unwrap_or_default();
    widgets.title.set_subtitle(&status_line(
        widgets.hits.len(),
        total,
        bible_app_db::DEFAULT_LIMIT,
    ));
    widgets.status.set_text(&status_line(
        widgets.hits.len(),
        total,
        bible_app_db::DEFAULT_LIMIT,
    ));
    refill_list(&widgets.list, &widgets.hits, books);
}

pub fn hit_at(widgets: &OccWidgets, idx: i32) -> Option<Ref> {
    let hit = widgets.hits.get(usize::try_from(idx).ok()?)?;
    Some(Ref {
        book: hit.book,
        chapter: hit.chapter,
        verse: hit.verse,
    })
}

pub fn see_all_label(code: &str, count: usize) -> String {
    if count == 0 {
        format!("See all {code} in the KJV")
    } else {
        format!("See all {code} in the KJV ({})", with_commas(count))
    }
}

pub fn status_line(shown: usize, total: usize, limit: usize) -> String {
    if total == 0 {
        "No KJV verses use this number.".into()
    } else if total == 1 {
        "1 verse".into()
    } else if shown < total || total > limit {
        format!(
            "First {} of {} verses",
            with_commas(shown.min(limit)),
            with_commas(total)
        )
    } else {
        format!("{} verses", with_commas(total))
    }
}

pub fn with_commas(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    let len = digits.len();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn display_snippet(text: &str) -> String {
    let (stored, _) = layout::split_notes(text);
    let (plain, _) = layout::strip_supplied(&stored);
    plain
}

fn refill_list(list: &gtk::ListBox, hits: &[Occurrence], books: &[Book]) {
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

fn row(hit: &Occurrence, books: &[Book]) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);

    let at = Ref {
        book: hit.book,
        chapter: hit.chapter,
        verse: hit.verse,
    };
    let title = gtk::Label::new(Some(&nav::format_ref(books, at)));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);

    let snippet = gtk::Label::new(Some(&display_snippet(&hit.snippet)));
    snippet.set_xalign(0.0);
    snippet.set_wrap(true);
    snippet.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    snippet.add_css_class("dim-label");
    snippet.set_max_width_chars(72);

    box_.append(&title);
    box_.append(&snippet);
    row.set_child(Some(&box_));
    row.set_activatable(true);
    row.set_tooltip_text(Some(&nav::format_ref(books, at)));
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn see_all_includes_code_and_count() {
        assert_eq!(see_all_label("H430", 0), "See all H430 in the KJV");
        assert_eq!(
            see_all_label("H430", 2249),
            "See all H430 in the KJV (2,249)"
        );
        assert_eq!(see_all_label("G26", 106), "See all G26 in the KJV (106)");
    }

    #[test]
    fn status_shows_total_and_limit() {
        assert!(status_line(0, 0, 200).contains("No KJV verses"));
        assert_eq!(status_line(1, 1, 200), "1 verse");
        assert_eq!(status_line(106, 106, 200), "106 verses");
        assert_eq!(status_line(200, 2249, 200), "First 200 of 2,249 verses");
        assert_eq!(with_commas(2602), "2,602");
        assert_eq!(with_commas(12), "12");
    }
}
