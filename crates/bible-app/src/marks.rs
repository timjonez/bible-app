use crate::layout::{self, ChapterLayout};
use crate::nav::{self, Ref};
use crate::user_db::{self, Bookmark, Note, VerseMarks};
use crate::workspace::TabId;
use bible_app_db::Book;
use gtk::gio;
use gtk::prelude::*;
use relm4::gtk;
use rusqlite::Connection;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

const HIGHLIGHT_TAGS: &[&str] = &["gold", "green", "blue", "rose"];

const USER_TAG_NAMES: &[&str] = &[
    "hl-gold",
    "hl-green",
    "hl-blue",
    "hl-rose",
    "user-bookmark",
    "user-note",
];

pub struct BookmarksWidgets {
    pub root: gtk::Widget,
    pub list: gtk::ListBox,
    pub bookmarks: Vec<Bookmark>,
    pub empty: gtk::Label,
    pub remove: gtk::Button,
}

pub struct NotesWidgets {
    pub root: gtk::Widget,
    pub list: gtk::ListBox,
    pub notes: Vec<Note>,
    pub empty: gtk::Label,
    pub editor: gtk::TextView,
    pub buffer: gtk::TextBuffer,
    pub editor_title: gtk::Label,
    pub editing: Option<Ref>,
    pub syncing: Rc<Cell<bool>>,
    pub delete: gtk::Button,
    pub export: gtk::Button,
}

pub fn install_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();
    for name in HIGHLIGHT_TAGS {
        let tag = gtk::TextTag::new(Some(&format!("hl-{name}")));
        tag.set_priority(0);
        table.add(&tag);
    }
    let bookmark = gtk::TextTag::new(Some("user-bookmark"));
    bookmark.set_underline(gtk::pango::Underline::Low);
    bookmark.set_priority(1);
    table.add(&bookmark);
    let note = gtk::TextTag::new(Some("user-note"));
    note.set_style(gtk::pango::Style::Italic);
    note.set_priority(1);
    table.add(&note);
}

pub fn apply_tags(
    buffer: &gtk::TextBuffer,
    layout: &ChapterLayout,
    marks: &HashMap<u8, VerseMarks>,
) {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    for name in USER_TAG_NAMES {
        if let Some(tag) = buffer.tag_table().lookup(name) {
            buffer.remove_tag(&tag, &start, &end);
        }
    }
    for (verse, mark) in marks {
        for hl in &mark.highlights {
            if let Some(span) = layout::highlight_paint_span(layout, *verse, hl.start, hl.end) {
                apply_tag(buffer, &format!("hl-{}", hl.color), span);
            }
        }
        let Some(num) = verse_num_span(layout, *verse) else {
            continue;
        };
        if mark.bookmark {
            apply_tag(buffer, "user-bookmark", num);
        }
        if mark.note {
            apply_tag(buffer, "user-note", num);
        }
    }
}

pub fn verse_num_span(layout: &ChapterLayout, verse: u8) -> Option<layout::Span> {
    let (_, vs) = layout.verse_start.iter().find(|(v, _)| *v == verse)?;
    layout.verse_nums.iter().copied().find(|s| s.start == *vs)
}

fn apply_tag(buffer: &gtk::TextBuffer, name: &str, span: layout::Span) {
    let Some(tag) = buffer.tag_table().lookup(name) else {
        return;
    };
    if span.end <= span.start {
        return;
    }
    let s = buffer.iter_at_offset(span.start);
    let e = buffer.iter_at_offset(span.end);
    buffer.apply_tag(&tag, &s, &e);
}

pub fn verse_menu_model(bookmarked: bool, has_note: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Copy verse"), Some("win.copy-verse-here"));
    let bookmark = if bookmarked {
        "Remove bookmark"
    } else {
        "Bookmark"
    };
    menu.append(Some(bookmark), Some("win.toggle-bookmark"));
    let highlight = gio::Menu::new();
    let gold = format!("win.highlight('{}')", user_db::DEFAULT_HIGHLIGHT);
    highlight.append(Some("Gold"), Some(gold.as_str()));
    highlight.append(Some("Green"), Some("win.highlight('green')"));
    highlight.append(Some("Blue"), Some("win.highlight('blue')"));
    highlight.append(Some("Rose"), Some("win.highlight('rose')"));
    highlight.append(Some("Remove"), Some("win.highlight('none')"));
    menu.append_submenu(Some("Highlight"), &highlight);
    let note = if has_note { "Edit note" } else { "Add note" };
    menu.append(Some(note), Some("win.add-note"));
    menu
}

pub fn build_bookmarks(id: TabId, sender: relm4::Sender<super::app::Msg>) -> BookmarksWidgets {
    let list = list_box();
    let send_bm = sender.clone();
    list.connect_row_activated(move |_, row| {
        send_bm.emit(super::app::Msg::MarksBookmarkActivated(id, row.index()));
    });
    let send_bm_sel = sender.clone();
    list.connect_row_selected(move |_, _| {
        send_bm_sel.emit(super::app::Msg::MarksBookmarkSelected(id));
    });

    let empty = empty_label("No bookmarks. Right-click a verse in the chapter to add one.");

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));

    let remove = gtk::Button::with_label("Remove");
    remove.set_halign(gtk::Align::Start);
    remove.set_sensitive(false);
    let send_rm = sender.clone();
    remove.connect_clicked(move |_| {
        send_rm.emit(super::app::Msg::RemoveSelectedBookmark(id));
    });

    let page = page_box();
    page.append(&empty);
    page.append(&scroll);
    page.append(&remove);

    BookmarksWidgets {
        root: page.upcast(),
        list,
        bookmarks: Vec::new(),
        empty,
        remove,
    }
}

pub fn build_notes(id: TabId, sender: relm4::Sender<super::app::Msg>) -> NotesWidgets {
    let list = list_box();
    let send_note = sender.clone();
    list.connect_row_activated(move |_, row| {
        send_note.emit(super::app::Msg::MarksNoteActivated(id, row.index()));
    });
    let send_note_sel = sender.clone();
    list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            send_note_sel.emit(super::app::Msg::MarksNoteSelected(id, row.index()));
        }
    });

    let empty = empty_label("No notes yet. Right-click a verse and choose Add note.");

    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_hexpand(true);
    list_scroll.set_min_content_height(140);
    list_scroll.set_vexpand(true);
    list_scroll.set_child(Some(&list));

    let editor_title = gtk::Label::new(Some("Select a note to edit"));
    editor_title.set_xalign(0.0);
    editor_title.add_css_class("heading");

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let editor = gtk::TextView::new();
    editor.set_buffer(Some(&buffer));
    editor.set_wrap_mode(gtk::WrapMode::WordChar);
    editor.set_left_margin(12);
    editor.set_right_margin(12);
    editor.set_top_margin(8);
    editor.set_bottom_margin(8);
    editor.set_hexpand(true);
    editor.set_vexpand(true);
    editor.set_sensitive(false);
    editor.set_accessible_role(gtk::AccessibleRole::TextBox);
    let editor_scroll = gtk::ScrolledWindow::new();
    editor_scroll.set_hexpand(true);
    editor_scroll.set_vexpand(true);
    editor_scroll.set_min_content_height(160);
    editor_scroll.set_child(Some(&editor));

    let syncing = Rc::new(Cell::new(false));
    let send_save = sender.clone();
    let sync_save = syncing.clone();
    buffer.connect_changed(move |_| {
        if sync_save.get() {
            return;
        }
        send_save.emit(super::app::Msg::SaveNote(id));
    });

    let delete = gtk::Button::with_label("Delete note");
    delete.set_halign(gtk::Align::Start);
    delete.set_sensitive(false);
    let send_del = sender.clone();
    delete.connect_clicked(move |_| {
        send_del.emit(super::app::Msg::DeleteEditingNote(id));
    });

    let export = gtk::Button::with_label("Export notes…");
    export.set_halign(gtk::Align::Start);
    export.set_visible(false);
    export.set_tooltip_text(Some("Save all notes as Markdown"));
    let send_export = sender.clone();
    export.connect_clicked(move |_| {
        send_export.emit(super::app::Msg::ExportNotes);
    });

    let page = page_box();
    page.append(&export);
    page.append(&empty);
    page.append(&list_scroll);
    page.append(&editor_title);
    page.append(&editor_scroll);
    page.append(&delete);

    NotesWidgets {
        root: page.upcast(),
        list,
        notes: Vec::new(),
        empty,
        editor,
        buffer,
        editor_title,
        editing: None,
        syncing,
        delete,
        export,
    }
}

pub fn editor_text(widgets: &NotesWidgets) -> String {
    let start = widgets.buffer.start_iter();
    let end = widgets.buffer.end_iter();
    widgets.buffer.text(&start, &end, false).to_string()
}

pub fn fill_bookmarks(widgets: &mut BookmarksWidgets, user: &Connection, books: &[Book]) {
    widgets.bookmarks = user_db::list_bookmarks(user).unwrap_or_default();
    refill_rows(&widgets.list, &widgets.bookmarks, books, |bm| {
        (bm.at(), bm.label.as_str())
    });
    let has_bm = !widgets.bookmarks.is_empty();
    widgets.list.set_visible(has_bm);
    widgets.empty.set_visible(!has_bm);
    widgets
        .remove
        .set_sensitive(widgets.list.selected_row().is_some());
}

pub fn fill_notes(widgets: &mut NotesWidgets, user: &Connection, books: &[Book]) {
    widgets.syncing.set(true);
    widgets.notes = user_db::list_notes(user).unwrap_or_default();
    refill_rows(&widgets.list, &widgets.notes, books, |note| {
        (note.at(), note.text.as_str())
    });
    let has_notes = !widgets.notes.is_empty();
    widgets.list.set_visible(has_notes);
    widgets.empty.set_visible(!has_notes);
    widgets.export.set_visible(has_notes);

    if let Some(at) = widgets.editing {
        if let Some(i) = widgets.notes.iter().position(|n| n.at() == at) {
            widgets
                .list
                .select_row(widgets.list.row_at_index(i as i32).as_ref());
            widgets.delete.set_sensitive(true);
        } else {
            widgets.list.unselect_all();
            widgets.delete.set_sensitive(false);
        }
    } else {
        widgets.delete.set_sensitive(false);
    }
    widgets.syncing.set(false);
}

pub fn bookmark_at(widgets: &BookmarksWidgets, idx: i32) -> Option<Ref> {
    widgets
        .bookmarks
        .get(usize::try_from(idx).ok()?)
        .map(Bookmark::at)
}

pub fn note_at(widgets: &NotesWidgets, idx: i32) -> Option<Ref> {
    widgets.notes.get(usize::try_from(idx).ok()?).map(Note::at)
}

pub fn selected_bookmark(widgets: &BookmarksWidgets) -> Option<Ref> {
    let row = widgets.list.selected_row()?;
    bookmark_at(widgets, row.index())
}

pub fn edit_note(widgets: &mut NotesWidgets, books: &[Book], at: Ref, text: &str) {
    widgets.syncing.set(true);
    widgets.editing = Some(at);
    widgets.buffer.set_text(text);
    widgets.syncing.set(false);
    widgets.editor_title.set_label(&nav::format_ref(books, at));
    widgets.editor.set_sensitive(true);
    widgets.delete.set_sensitive(!text.trim().is_empty());
    if let Some(i) = widgets.notes.iter().position(|n| n.at() == at) {
        widgets.syncing.set(true);
        widgets
            .list
            .select_row(widgets.list.row_at_index(i as i32).as_ref());
        widgets.syncing.set(false);
    }
    widgets.editor.grab_focus();
}

pub fn load_note(widgets: &mut NotesWidgets, books: &[Book], at: Ref, text: &str) {
    if widgets.syncing.get() || widgets.editing == Some(at) {
        return;
    }
    edit_note(widgets, books, at, text);
}

pub fn clear_editor(widgets: &mut NotesWidgets) {
    widgets.syncing.set(true);
    widgets.editing = None;
    widgets.buffer.set_text("");
    widgets.syncing.set(false);
    widgets.editor_title.set_label("Select a note to edit");
    widgets.editor.set_sensitive(false);
    widgets.delete.set_sensitive(false);
}

fn list_box() -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.set_accessible_role(gtk::AccessibleRole::List);
    list
}

fn empty_label(text: &str) -> gtk::Label {
    let empty = gtk::Label::new(Some(text));
    empty.set_wrap(true);
    empty.set_xalign(0.0);
    empty.add_css_class("dim-label");
    empty.set_margin_start(4);
    empty.set_margin_end(4);
    empty
}

fn page_box() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_start(12);
    page.set_margin_end(12);
    page.set_margin_top(8);
    page.set_margin_bottom(8);
    page.set_hexpand(true);
    page.set_vexpand(true);
    page
}

fn refill_rows<T>(
    list: &gtk::ListBox,
    items: &[T],
    books: &[Book],
    row: impl Fn(&T) -> (Ref, &str),
) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for item in items {
        let (at, extra) = row(item);
        list.append(&ref_row(at, books, extra));
    }
}

fn ref_row(at: Ref, books: &[Book], extra: &str) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    box_.set_margin_start(12);
    box_.set_margin_end(12);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);

    let title = gtk::Label::new(Some(&nav::format_ref(books, at)));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    box_.append(&title);

    let snippet = snippet(extra);
    if !snippet.is_empty() {
        let sub = gtk::Label::new(Some(&snippet));
        sub.set_xalign(0.0);
        sub.set_wrap(true);
        sub.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        sub.add_css_class("dim-label");
        sub.set_max_width_chars(64);
        box_.append(&sub);
    }

    row.set_child(Some(&box_));
    row.set_activatable(true);
    row.set_tooltip_text(Some(&nav::format_ref(books, at)));
    row
}

fn snippet(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    const MAX: usize = 80;
    let count = line.chars().count();
    if count > MAX {
        format!("{}…", line.chars().take(MAX).collect::<String>())
    } else {
        line.to_string()
    }
}
