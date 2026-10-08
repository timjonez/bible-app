use crate::cite;
use crate::history::History;
use crate::nav::{self, Ref};
use crate::picker;
use crate::shell;
use crate::tsk_parse::Citation;
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::{Book, Resource};
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct MhcWidgets {
    pub root: gtk::Widget,
    pub book: gtk::DropDown,
    pub chapter: gtk::DropDown,
    pub prev: gtk::Button,
    pub next: gtk::Button,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub follow: gtk::ToggleButton,
    pub follow_syncing: Rc<Cell<bool>>,
    pub buffer: gtk::TextBuffer,
    pub view: gtk::TextView,
    pub menu: gtk::PopoverMenu,
    pub links: Rc<RefCell<Vec<Citation>>>,
    /// Passage opened from a citation when this view could not sit beside one.
    pub companion: Cell<Option<TabId>>,
    pub syncing: Rc<Cell<bool>>,
    pub history: RefCell<History<Ref>>,
    pub placed: Cell<bool>,
    loaded: Cell<(u8, u8)>,
    sections: RefCell<Vec<(u8, i32)>>,
}

pub fn build() -> MhcWidgets {
    let bar = shell::location_bar(
        "Previous chapter (Alt+Left)",
        "Next chapter (Alt+Right)",
        "Previous chapter",
        "Next chapter",
    );
    let book = picker::book_dropdown();
    let chapter = picker::chapter_dropdown();
    bar.pickers.append(&book);
    bar.pickers.append(&chapter);
    let follow = shell::follow_pin();
    bar.end.append(&follow);

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let heading = gtk::TextTag::new(Some("section"));
    buffer.tag_table().add(&heading);
    heading.set_weight(700);
    heading.set_pixels_above_lines(16);
    heading.set_pixels_below_lines(4);
    cite::add_tag(&buffer);

    let view = gtk::TextView::new();
    view.set_buffer(Some(&buffer));
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_left_margin(20);
    view.set_right_margin(20);
    view.set_top_margin(8);
    view.set_bottom_margin(16);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_accessible_role(gtk::AccessibleRole::Document);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&view));

    let page = shell::bar_page(&bar.row, &scroll);
    let menu = cite::menu_for(&view);
    let links = Rc::new(RefCell::new(Vec::new()));
    MhcWidgets {
        root: page.upcast(),
        book,
        chapter,
        prev: bar.prev,
        next: bar.next,
        back: bar.back,
        forward: bar.forward,
        follow,
        follow_syncing: Rc::new(Cell::new(false)),
        buffer,
        view,
        menu,
        links,
        companion: Cell::new(None),
        syncing: Rc::new(Cell::new(false)),
        history: RefCell::new(History::new(Ref {
            book: 1,
            chapter: 1,
            verse: 1,
        })),
        placed: Cell::new(false),
        loaded: Cell::new((0, 0)),
        sections: RefCell::new(Vec::new()),
    }
}

pub fn wire(
    widgets: &MhcWidgets,
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
    books: &[Book],
) {
    widgets.syncing.set(true);
    let book_tx = sender.clone();
    let chapter_tx = sender.clone();
    picker::wire_place(
        &widgets.book,
        &widgets.chapter,
        &widgets.syncing,
        books,
        move |pos| book_tx.emit(super::app::Msg::StudyBook(id, pos)),
        move |pos| chapter_tx.emit(super::app::Msg::StudyChapter(id, pos)),
    );
    widgets.syncing.set(false);
    let tx = sender.clone();
    widgets
        .prev
        .connect_clicked(move |_| tx.emit(super::app::Msg::StudyPrev(id)));
    let tx = sender.clone();
    widgets
        .next
        .connect_clicked(move |_| tx.emit(super::app::Msg::StudyNext(id)));
    let tx = sender.clone();
    widgets
        .back
        .connect_clicked(move |_| tx.emit(super::app::Msg::StudyBack(id)));
    let tx = sender.clone();
    widgets
        .forward
        .connect_clicked(move |_| tx.emit(super::app::Msg::StudyForward(id)));
    let tx = sender.clone();
    let syncing = widgets.follow_syncing.clone();
    widgets.follow.connect_toggled(move |btn| {
        if syncing.get() {
            return;
        }
        tx.emit(super::app::Msg::SetFollow(id, btn.is_active()));
    });
    cite::wire(&widgets.view, widgets.links.clone(), id, sender);
}

pub fn show(widgets: &MhcWidgets, conn: &Connection, books: &[Book], at: Ref) {
    let key = (at.book, at.chapter);
    if widgets.loaded.get() != key {
        let rows =
            bible_app_db::chapter_resources(conn, "MHC", at.book, at.chapter).unwrap_or_default();
        paint(widgets, books, &rows, at);
        widgets.loaded.set(key);
    }
    scroll_to(widgets, at.verse);
}

fn paint(widgets: &MhcWidgets, books: &[Book], rows: &[Resource], at: Ref) {
    if rows.is_empty() {
        widgets.sections.borrow_mut().clear();
        widgets.links.borrow_mut().clear();
        widgets.buffer.set_text(&format!(
            "No Matthew Henry on {}.",
            nav::format_chapter(books, at.book, at.chapter)
        ));
        return;
    }
    let parts: Vec<(String, &str)> = rows
        .iter()
        .map(|row| {
            (
                nav::format_ref(
                    books,
                    Ref {
                        book: row.book,
                        chapter: row.chapter,
                        verse: row.verse,
                    },
                ),
                row.text.as_str(),
            )
        })
        .collect();
    let borrowed: Vec<(&str, &str)> = parts.iter().map(|(h, b)| (h.as_str(), *b)).collect();
    let stacked = nav::stack_sections(&borrowed);
    widgets.buffer.set_text(&stacked.text);
    if let Some(tag) = widgets.buffer.tag_table().lookup("section") {
        for (start, len) in stacked.heading_at.iter().zip(&stacked.heading_len) {
            let s = widgets.buffer.iter_at_offset(*start);
            let e = widgets.buffer.iter_at_offset(start + len);
            widgets.buffer.apply_tag(&tag, &s, &e);
        }
    }
    widgets
        .links
        .replace(cite::relink(&widgets.buffer, &stacked.text, books));
    widgets.sections.replace(
        rows.iter()
            .map(|row| row.verse)
            .zip(stacked.heading_at)
            .collect(),
    );
}

fn scroll_to(widgets: &MhcWidgets, verse: u8) {
    let sections = widgets.sections.borrow();
    let verses: Vec<u8> = sections.iter().map(|(v, _)| *v).collect();
    let offsets: Vec<i32> = sections.iter().map(|(_, off)| *off).collect();
    let Some(offset) = nav::section_offset(&verses, &offsets, verse) else {
        return;
    };
    let view = widgets.view.clone();
    glib::idle_add_local_once(move || {
        let buffer = view.buffer();
        let mut iter = buffer.iter_at_offset(offset);
        view.scroll_to_iter(&mut iter, 0.05, true, 0.0, 0.12);
    });
}
