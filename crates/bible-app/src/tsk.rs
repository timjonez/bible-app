use crate::nav::{self, Ref};
use adw::prelude::*;
use bible_app_db::Book;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct TskWidgets {
    pub root: gtk::Widget,
    pub heading: gtk::Label,
    pub buffer: gtk::TextBuffer,
    pub xref_list: gtk::ListBox,
    pub xrefs: Vec<Ref>,
}

pub fn build(sender: relm4::Sender<super::app::Msg>) -> TskWidgets {
    let heading = gtk::Label::new(None);
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    heading.set_wrap_mode(gtk::pango::WrapMode::WordChar);

    let xref_list = gtk::ListBox::new();
    xref_list.set_selection_mode(gtk::SelectionMode::Single);
    xref_list.add_css_class("boxed-list");
    xref_list.set_accessible_role(gtk::AccessibleRole::List);
    let list = xref_list.clone();
    let send = sender.clone();
    xref_list.connect_row_activated(move |_, row| {
        send.emit(super::app::Msg::OpenTskXref(row.index()));
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
    body.append(&heading);
    body.append(&hint);
    body.append(&xref_scroll);
    body.append(&text_scroll);

    TskWidgets {
        root: body.upcast(),
        heading,
        buffer,
        xref_list,
        xrefs: Vec::new(),
    }
}

pub fn fill(
    widgets: &mut TskWidgets,
    conn: &Connection,
    books: &[Book],
    at: Ref,
    sender: relm4::Sender<super::app::Msg>,
) {
    widgets.heading.set_label(&nav::format_ref(books, at));
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
                    .heading
                    .set_label(&format!("{} · from {covering}", nav::format_ref(books, at)));
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
    refill_xrefs(&widgets.xref_list, &widgets.xrefs, books, sender);
}

pub fn xref_at(widgets: &TskWidgets, idx: i32) -> Option<Ref> {
    widgets.xrefs.get(usize::try_from(idx).ok()?).copied()
}

pub fn create_popover(parent: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_accessible_role(gtk::AccessibleRole::Dialog);
    popover
}

pub fn present_phrase(
    popover: &gtk::Popover,
    view: &gtk::TextView,
    start: i32,
    heading: &str,
    dests: &[Ref],
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(12);
    body.set_margin_end(12);
    body.set_margin_top(10);
    body.set_margin_bottom(10);
    body.set_width_request(280);

    let title = gtk::Label::new(Some(heading));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    title.set_wrap(true);
    body.append(&title);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.set_accessible_role(gtk::AccessibleRole::List);
    let dests_vec = dests.to_vec();
    let jump = sender.clone();
    list.connect_row_activated(move |_, row| {
        let Ok(idx) = usize::try_from(row.index()) else {
            return;
        };
        if let Some(at) = dests_vec.get(idx).copied() {
            jump.emit(super::app::Msg::OpenTskDest(at));
        }
    });
    for at in dests {
        list.append(&dest_row(*at, books, sender.clone()));
    }

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_min_content_height(80);
    scroll.set_max_content_height(280);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&list));
    body.append(&scroll);
    popover.set_child(Some(&body));

    let buffer = view.buffer();
    let iter = buffer.iter_at_offset(start);
    let loc = view.iter_location(&iter);
    let (x, y) =
        view.buffer_to_window_coords(gtk::TextWindowType::Widget, loc.x(), loc.y() + loc.height());
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, loc.width().max(1), 1)));
    popover.popup();
}

fn dest_row(at: Ref, books: &[Book], sender: relm4::Sender<super::app::Msg>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    box_.set_margin_start(10);
    box_.set_margin_end(6);
    box_.set_margin_top(4);
    box_.set_margin_bottom(4);
    let label = gtk::Label::new(Some(&nav::format_ref(books, at)));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_wrap(true);
    box_.append(&label);
    let beside = gtk::Button::from_icon_name("tab-new-symbolic");
    beside.set_tooltip_text(Some("Open beside"));
    beside.add_css_class("flat");
    beside.set_valign(gtk::Align::Center);
    beside.set_has_frame(false);
    beside.connect_clicked(move |_| {
        sender.emit(super::app::Msg::OpenTskDestBeside(at));
    });
    box_.append(&beside);
    row.set_child(Some(&box_));
    row.set_activatable(true);
    row
}

fn refill_xrefs(
    list: &gtk::ListBox,
    xrefs: &[Ref],
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for at in xrefs {
        list.append(&dest_row(*at, books, sender.clone()));
    }
}
