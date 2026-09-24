use crate::layout::{self, ChapterLayout};
use crate::nav::{self, Ref};
use crate::user_db::{self, Bookmark, Note, VerseMarks};
use adw::prelude::*;
use bible_app_db::Book;
use gtk::gio;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

const HIGHLIGHT_TAGS: &[(&str, &str, f32)] = &[
    ("gold", "#e5a50a", 0.32),
    ("green", "#57e389", 0.28),
    ("blue", "#62a0ea", 0.28),
    ("rose", "#ed333b", 0.22),
];

const USER_TAG_NAMES: &[&str] = &[
    "hl-gold",
    "hl-green",
    "hl-blue",
    "hl-rose",
    "user-bookmark",
    "user-note",
];

pub struct MarksWidgets {
    pub root: gtk::Widget,
    pub stack: adw::ViewStack,
    pub bookmark_list: gtk::ListBox,
    pub bookmarks: Vec<Bookmark>,
    pub bookmarks_empty: gtk::Label,
    pub note_list: gtk::ListBox,
    pub notes: Vec<Note>,
    pub notes_empty: gtk::Label,
    pub editor: gtk::TextView,
    pub buffer: gtk::TextBuffer,
    pub editor_title: gtk::Label,
    pub editing: Option<Ref>,
    pub syncing: Rc<Cell<bool>>,
    pub remove_bookmark: gtk::Button,
    pub delete_note: gtk::Button,
    pub export_notes: gtk::Button,
}

pub fn install_tags(buffer: &gtk::TextBuffer) {
    let table = buffer.tag_table();
    for (name, hex, alpha) in HIGHLIGHT_TAGS {
        let tag = gtk::TextTag::new(Some(&format!("hl-{name}")));
        let mut color = gtk::gdk::RGBA::parse(*hex)
            .unwrap_or_else(|_| gtk::gdk::RGBA::new(0.9, 0.75, 0.2, *alpha));
        color.set_alpha(*alpha);
        tag.set_background_rgba(Some(&color));
        tag.set_priority(0);
        table.add(&tag);
    }
    let bookmark = gtk::TextTag::new(Some("user-bookmark"));
    bookmark.set_underline(gtk::pango::Underline::Low);
    if let Ok(color) = gtk::gdk::RGBA::parse("#c64600") {
        bookmark.set_underline_rgba(Some(&color));
    }
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

pub fn build(sender: relm4::Sender<super::app::Msg>) -> MarksWidgets {
    let stack = adw::ViewStack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(&stack));
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);
    switcher.set_halign(gtk::Align::Center);

    let bookmark_list = gtk::ListBox::new();
    bookmark_list.set_selection_mode(gtk::SelectionMode::Single);
    bookmark_list.add_css_class("boxed-list");
    bookmark_list.set_accessible_role(gtk::AccessibleRole::List);
    let send_bm = sender.clone();
    bookmark_list.connect_row_activated(move |_, row| {
        send_bm.emit(super::app::Msg::MarksBookmarkActivated(row.index()));
    });
    let send_bm_sel = sender.clone();
    bookmark_list.connect_row_selected(move |_, _| {
        send_bm_sel.emit(super::app::Msg::MarksBookmarkSelected);
    });

    let bookmarks_empty = gtk::Label::new(Some(
        "No bookmarks. Right-click a verse in the chapter to add one.",
    ));
    bookmarks_empty.set_wrap(true);
    bookmarks_empty.set_xalign(0.0);
    bookmarks_empty.add_css_class("dim-label");
    bookmarks_empty.set_margin_start(4);
    bookmarks_empty.set_margin_end(4);

    let bookmark_scroll = gtk::ScrolledWindow::new();
    bookmark_scroll.set_hexpand(true);
    bookmark_scroll.set_vexpand(true);
    bookmark_scroll.set_child(Some(&bookmark_list));

    let remove_bookmark = gtk::Button::with_label("Remove");
    remove_bookmark.set_halign(gtk::Align::Start);
    remove_bookmark.set_sensitive(false);
    let send_rm = sender.clone();
    remove_bookmark.connect_clicked(move |_| {
        send_rm.emit(super::app::Msg::RemoveSelectedBookmark);
    });

    let bookmarks_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    bookmarks_page.set_margin_start(12);
    bookmarks_page.set_margin_end(12);
    bookmarks_page.set_margin_top(8);
    bookmarks_page.set_margin_bottom(8);
    bookmarks_page.append(&bookmarks_empty);
    bookmarks_page.append(&bookmark_scroll);
    bookmarks_page.append(&remove_bookmark);

    let note_list = gtk::ListBox::new();
    note_list.set_selection_mode(gtk::SelectionMode::Single);
    note_list.add_css_class("boxed-list");
    note_list.set_accessible_role(gtk::AccessibleRole::List);
    let send_note = sender.clone();
    note_list.connect_row_activated(move |_, row| {
        send_note.emit(super::app::Msg::MarksNoteActivated(row.index()));
    });
    let send_note_sel = sender.clone();
    note_list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            send_note_sel.emit(super::app::Msg::MarksNoteSelected(row.index()));
        }
    });

    let notes_empty = gtk::Label::new(Some(
        "No notes yet. Right-click a verse and choose Add note.",
    ));
    notes_empty.set_wrap(true);
    notes_empty.set_xalign(0.0);
    notes_empty.add_css_class("dim-label");
    notes_empty.set_margin_start(4);
    notes_empty.set_margin_end(4);

    let note_scroll = gtk::ScrolledWindow::new();
    note_scroll.set_hexpand(true);
    note_scroll.set_min_content_height(140);
    note_scroll.set_vexpand(true);
    note_scroll.set_child(Some(&note_list));

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
        send_save.emit(super::app::Msg::SaveNote);
    });

    let delete_note = gtk::Button::with_label("Delete note");
    delete_note.set_halign(gtk::Align::Start);
    delete_note.set_sensitive(false);
    let send_del = sender.clone();
    delete_note.connect_clicked(move |_| {
        send_del.emit(super::app::Msg::DeleteEditingNote);
    });

    let export_notes = gtk::Button::with_label("Export notes…");
    export_notes.set_halign(gtk::Align::Start);
    export_notes.set_visible(false);
    export_notes.set_tooltip_text(Some("Save all notes as Markdown"));
    let send_export = sender.clone();
    export_notes.connect_clicked(move |_| {
        send_export.emit(super::app::Msg::ExportNotes);
    });

    let notes_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    notes_page.set_margin_start(12);
    notes_page.set_margin_end(12);
    notes_page.set_margin_top(8);
    notes_page.set_margin_bottom(8);
    notes_page.append(&export_notes);
    notes_page.append(&notes_empty);
    notes_page.append(&note_scroll);
    notes_page.append(&editor_title);
    notes_page.append(&editor_scroll);
    notes_page.append(&delete_note);

    stack.add_titled(&bookmarks_page, Some("bookmarks"), "Bookmarks");
    stack.add_titled(&notes_page, Some("notes"), "Notes");

    let body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    body.set_margin_start(8);
    body.set_margin_end(8);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    body.append(&switcher);
    body.append(&stack);

    MarksWidgets {
        root: body.upcast(),
        stack,
        bookmark_list,
        bookmarks: Vec::new(),
        bookmarks_empty,
        note_list,
        notes: Vec::new(),
        notes_empty,
        editor,
        buffer,
        editor_title,
        editing: None,
        syncing,
        remove_bookmark,
        delete_note,
        export_notes,
    }
}

pub fn show_page(widgets: &MarksWidgets, page: &str) {
    widgets.stack.set_visible_child_name(page);
}

pub fn editor_text(widgets: &MarksWidgets) -> String {
    let start = widgets.buffer.start_iter();
    let end = widgets.buffer.end_iter();
    widgets.buffer.text(&start, &end, false).to_string()
}

pub fn fill(widgets: &mut MarksWidgets, user: &Connection, books: &[Book]) {
    refresh_lists(widgets, user, books);
}

pub fn refresh_lists(widgets: &mut MarksWidgets, user: &Connection, books: &[Book]) {
    widgets.syncing.set(true);
    widgets.bookmarks = user_db::list_bookmarks(user).unwrap_or_default();
    refill_bookmarks(&widgets.bookmark_list, &widgets.bookmarks, books);
    let has_bm = !widgets.bookmarks.is_empty();
    widgets.bookmark_list.set_visible(has_bm);
    widgets.bookmarks_empty.set_visible(!has_bm);

    widgets.notes = user_db::list_notes(user).unwrap_or_default();
    refill_notes(&widgets.note_list, &widgets.notes, books);
    let has_notes = !widgets.notes.is_empty();
    widgets.note_list.set_visible(has_notes);
    widgets.notes_empty.set_visible(!has_notes);
    widgets.export_notes.set_visible(has_notes);

    if let Some(at) = widgets.editing {
        if let Some(i) = widgets.notes.iter().position(|n| n.at() == at) {
            widgets
                .note_list
                .select_row(widgets.note_list.row_at_index(i as i32).as_ref());
            widgets.delete_note.set_sensitive(true);
        } else {
            widgets.note_list.unselect_all();
            widgets.delete_note.set_sensitive(false);
        }
    } else {
        widgets.delete_note.set_sensitive(false);
    }
    widgets
        .remove_bookmark
        .set_sensitive(widgets.bookmark_list.selected_row().is_some());
    widgets.syncing.set(false);
}

pub fn bookmark_at(widgets: &MarksWidgets, idx: i32) -> Option<Ref> {
    widgets
        .bookmarks
        .get(usize::try_from(idx).ok()?)
        .map(Bookmark::at)
}

pub fn note_at(widgets: &MarksWidgets, idx: i32) -> Option<Ref> {
    widgets.notes.get(usize::try_from(idx).ok()?).map(Note::at)
}

pub fn selected_bookmark(widgets: &MarksWidgets) -> Option<Ref> {
    let row = widgets.bookmark_list.selected_row()?;
    bookmark_at(widgets, row.index())
}

pub fn edit_note(widgets: &mut MarksWidgets, books: &[Book], at: Ref, text: &str) {
    widgets.syncing.set(true);
    widgets.editing = Some(at);
    widgets.buffer.set_text(text);
    widgets.syncing.set(false);
    widgets.editor_title.set_label(&nav::format_ref(books, at));
    widgets.editor.set_sensitive(true);
    widgets.delete_note.set_sensitive(!text.trim().is_empty());
    show_page(widgets, "notes");
    if let Some(i) = widgets.notes.iter().position(|n| n.at() == at) {
        widgets.syncing.set(true);
        widgets
            .note_list
            .select_row(widgets.note_list.row_at_index(i as i32).as_ref());
        widgets.syncing.set(false);
    }
    widgets.editor.grab_focus();
}

pub fn load_note(widgets: &mut MarksWidgets, books: &[Book], at: Ref, text: &str) {
    if widgets.syncing.get() || widgets.editing == Some(at) {
        return;
    }
    edit_note(widgets, books, at, text);
}

pub fn clear_editor(widgets: &mut MarksWidgets) {
    widgets.syncing.set(true);
    widgets.editing = None;
    widgets.buffer.set_text("");
    widgets.syncing.set(false);
    widgets.editor_title.set_label("Select a note to edit");
    widgets.editor.set_sensitive(false);
    widgets.delete_note.set_sensitive(false);
}

fn refill_bookmarks(list: &gtk::ListBox, bookmarks: &[Bookmark], books: &[Book]) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for bm in bookmarks {
        list.append(&ref_row(bm.at(), books, bm.label.as_str()));
    }
}

fn refill_notes(list: &gtk::ListBox, notes: &[Note], books: &[Book]) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for note in notes {
        list.append(&ref_row(note.at(), books, note.text.as_str()));
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
