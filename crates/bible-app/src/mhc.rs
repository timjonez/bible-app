use crate::nav::{self, Ref};
use adw::prelude::*;
use bible_app_db::Book;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct MhcWidgets {
    pub root: gtk::Widget,
    pub heading: gtk::Label,
    pub buffer: gtk::TextBuffer,
}

pub fn build() -> MhcWidgets {
    let heading = gtk::Label::new(None);
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    heading.set_wrap_mode(gtk::pango::WrapMode::WordChar);

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
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_accessible_role(gtk::AccessibleRole::Document);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&view));

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(16);
    body.set_margin_end(16);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.append(&heading);
    body.append(&scroll);

    MhcWidgets {
        root: body.upcast(),
        heading,
        buffer,
    }
}

pub fn fill(widgets: &MhcWidgets, conn: &Connection, books: &[Book], at: Ref) {
    widgets.heading.set_label(&nav::format_ref(books, at));
    match bible_app_db::resource_covering(conn, "MHC", at.book, at.chapter, at.verse) {
        Ok(Some(res)) => {
            let covering = nav::format_ref(
                books,
                Ref {
                    book: res.book,
                    chapter: res.chapter,
                    verse: res.verse,
                },
            );
            if res.verse != at.verse {
                widgets
                    .heading
                    .set_label(&format!("{} · from {covering}", nav::format_ref(books, at)));
            }
            widgets.buffer.set_text(&res.text);
        }
        Ok(None) => {
            widgets.buffer.set_text(&format!(
                "No Matthew Henry on {}.",
                nav::format_chapter(books, at.book, at.chapter)
            ));
        }
        Err(e) => widgets.buffer.set_text(&e.to_string()),
    }
}
