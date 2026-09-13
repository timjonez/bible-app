use crate::nav::{self, Ref};
use adw::prelude::*;
use bible_app_db::Book;
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct TskWidgets {
    pub window: adw::ApplicationWindow,
    pub title: adw::WindowTitle,
    pub buffer: gtk::TextBuffer,
    pub xref_list: gtk::ListBox,
    pub xrefs: Vec<Ref>,
}

pub fn open(sender: relm4::Sender<super::app::Msg>, at: Ref, books: &[Book]) -> TskWidgets {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("Treasury of Scripture Knowledge"));
    window.set_default_size(520, 720);

    let subtitle = nav::format_ref(books, at);
    let title = adw::WindowTitle::new("TSK", &subtitle);
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let xref_list = gtk::ListBox::new();
    xref_list.set_selection_mode(gtk::SelectionMode::Single);
    xref_list.add_css_class("boxed-list");
    xref_list.set_accessible_role(gtk::AccessibleRole::List);
    let list = xref_list.clone();
    let send = sender.clone();
    xref_list.connect_row_activated(move |_, row| {
        let idx = row.index();
        send.emit(super::app::Msg::OpenTskXref(idx));
    });

    let xref_scroll = gtk::ScrolledWindow::new();
    xref_scroll.set_min_content_height(120);
    xref_scroll.set_max_content_height(220);
    xref_scroll.set_propagate_natural_height(true);
    xref_scroll.set_child(Some(&list));

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let view = gtk::TextView::new();
    view.set_buffer(Some(&buffer));
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_left_margin(20);
    view.set_right_margin(20);
    view.set_top_margin(16);
    view.set_bottom_margin(16);
    view.set_accessible_role(gtk::AccessibleRole::Document);

    let text_scroll = gtk::ScrolledWindow::new();
    text_scroll.set_hexpand(true);
    text_scroll.set_vexpand(true);
    text_scroll.set_child(Some(&view));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    let hint = gtk::Label::new(Some("Cross-references"));
    hint.set_xalign(0.0);
    hint.add_css_class("heading");
    body.append(&hint);
    body.append(&xref_scroll);
    body.append(&text_scroll);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));
    window.set_content(Some(&toolbar));

    window.connect_close_request(move |_| {
        sender.emit(super::app::Msg::TskClosed);
        glib::Propagation::Proceed
    });
    window.present();
    TskWidgets {
        window,
        title,
        buffer,
        xref_list,
        xrefs: Vec::new(),
    }
}

pub fn fill(widgets: &mut TskWidgets, conn: &Connection, books: &[Book], at: Ref) {
    widgets.title.set_subtitle(&nav::format_ref(books, at));
    match bible_app_db::resource_covering(conn, "TSK", at.book, at.chapter, at.verse) {
        Ok(Some(res)) => {
            if res.verse != at.verse {
                let covering = nav::format_ref(
                    books,
                    Ref {
                        book: res.book,
                        chapter: res.chapter,
                        verse: res.verse,
                    },
                );
                widgets
                    .title
                    .set_subtitle(&format!("{} · from {covering}", nav::format_ref(books, at)));
            }
            widgets.buffer.set_text(&res.text);
        }
        Ok(None) => {
            widgets.buffer.set_text(&format!(
                "No Treasury of Scripture Knowledge on {}.",
                nav::format_chapter(books, at.book, at.chapter)
            ));
        }
        Err(e) => widgets.buffer.set_text(&e.to_string()),
    }

    widgets.xrefs = bible_app_db::xrefs_from(conn, at.book, at.chapter, at.verse)
        .unwrap_or_default()
        .into_iter()
        .map(|x| Ref {
            book: x.book,
            chapter: x.chapter,
            verse: x.verse,
        })
        .collect();
    refill_xrefs(&widgets.xref_list, &widgets.xrefs, books);
}

pub fn xref_at(widgets: &TskWidgets, idx: i32) -> Option<Ref> {
    widgets.xrefs.get(usize::try_from(idx).ok()?).copied()
}

fn refill_xrefs(list: &gtk::ListBox, xrefs: &[Ref], books: &[Book]) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for at in xrefs {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some(&nav::format_ref(books, *at)));
        label.set_xalign(0.0);
        label.set_margin_start(12);
        label.set_margin_end(12);
        label.set_margin_top(8);
        label.set_margin_bottom(8);
        row.set_child(Some(&label));
        row.set_activatable(true);
        list.append(&row);
    }
}
