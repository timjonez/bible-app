use crate::nav::{self, Ref};
use adw::prelude::*;
use bible_app_db::Book;
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct MhcWidgets {
    pub window: adw::ApplicationWindow,
    pub title: adw::WindowTitle,
    pub buffer: gtk::TextBuffer,
}

pub fn open(sender: relm4::Sender<super::app::Msg>, at: Ref, books: &[Book]) -> MhcWidgets {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("Matthew Henry"));
    window.set_default_size(520, 720);

    let subtitle = nav::format_ref(books, at);
    let title = adw::WindowTitle::new("Matthew Henry", &subtitle);
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

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

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&view));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroll));
    window.set_content(Some(&toolbar));

    window.connect_close_request(move |_| {
        sender.emit(super::app::Msg::MhcClosed);
        glib::Propagation::Proceed
    });
    window.present();
    MhcWidgets {
        window,
        title,
        buffer,
    }
}

pub fn fill(widgets: &MhcWidgets, conn: &Connection, books: &[Book], at: Ref) {
    widgets.title.set_subtitle(&nav::format_ref(books, at));
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
                    .title
                    .set_subtitle(&format!("{} · from {covering}", nav::format_ref(books, at)));
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
