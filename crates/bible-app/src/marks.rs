use crate::column;
use crate::layout::{self, ChapterLayout};
use crate::nav::{self, Ref};
use crate::user_db::{self, Bookmark, Note, VerseMarks};
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::Book;
use gtk::gio;
use gtk::glib::object::IsA;
use relm4::{adw, gtk};
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
    tab: TabId,
    sender: relm4::Sender<super::app::Msg>,
}

pub struct NotesWidgets {
    pub root: gtk::Widget,
    pub list: gtk::ListBox,
    pub notes: Vec<Note>,
    pub empty: gtk::Label,
    pub editor: gtk::TextView,
    pub buffer: gtk::TextBuffer,
    pub editor_title: gtk::Label,
    pub editing: Rc<Cell<Option<Ref>>>,
    pub syncing: Rc<Cell<bool>>,
    pub export: gtk::Button,
    pub actions: gtk::MenuButton,
    pub column: column::Column,
    tab: TabId,
    sender: relm4::Sender<super::app::Msg>,
}

pub struct NoteDialog {
    pub dialog: adw::Dialog,
    pub editor: gtk::TextView,
    pub buffer: gtk::TextBuffer,
    pub delete: gtk::Button,
    pub editing: Option<Ref>,
    pub syncing: Rc<Cell<bool>>,
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

pub fn verse_menu_model(has_note: bool, has_mhc: bool) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Copy"), Some("win.copy-verse-here"));
    append_item(&menu, "Bookmark", "win.toggle-bookmark", Some("<Control>d"));
    let highlight = gio::Menu::new();
    let gold = format!("win.highlight('{}')", user_db::DEFAULT_HIGHLIGHT);
    highlight.append(Some("Gold"), Some(gold.as_str()));
    highlight.append(Some("Green"), Some("win.highlight('green')"));
    highlight.append(Some("Blue"), Some("win.highlight('blue')"));
    highlight.append(Some("Rose"), Some("win.highlight('rose')"));
    highlight.append(Some("Remove"), Some("win.highlight('none')"));
    menu.append_submenu(Some("Highlight"), &highlight);
    let note = if has_note { "Edit note" } else { "Add note" };
    append_item(&menu, note, "win.add-note", Some("<Control><Shift>n"));
    if has_mhc {
        menu.append(Some("Matthew Henry"), Some("win.open-mhc-here"));
    }
    menu
}

fn append_item(menu: &gio::Menu, label: &str, action: &str, accel: Option<&str>) {
    let item = gio::MenuItem::new(Some(label), Some(action));
    if let Some(accel) = accel {
        item.set_attribute_value("accel", Some(&accel.to_variant()));
    }
    menu.append_item(&item);
}

pub fn build_bookmarks(id: TabId, sender: relm4::Sender<super::app::Msg>) -> BookmarksWidgets {
    let list = list_box();
    let send_bm = sender.clone();
    list.connect_row_activated(move |_, row| {
        send_bm.emit(super::app::Msg::MarksBookmarkActivated(id, row.index()));
    });

    let empty = empty_label("No bookmarks. Right-click a verse in the chapter to add one.");

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));

    let page = page_box();
    page.append(&empty);
    page.append(&scroll);

    BookmarksWidgets {
        root: page.upcast(),
        list,
        bookmarks: Vec::new(),
        empty,
        tab: id,
        sender,
    }
}

pub fn build_notes(
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
    column_px: i32,
) -> NotesWidgets {
    let list = list_box();
    list.add_css_class("navigation-sidebar");
    list.remove_css_class("boxed-list");
    let send_note_sel = sender.clone();
    list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            send_note_sel.emit(super::app::Msg::MarksNoteSelected(id, row.index()));
        }
    });

    let empty = empty_label("No notes yet. Right-click a verse and choose Add note.");

    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_hexpand(true);
    list_scroll.set_vexpand(true);
    list_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    list_scroll.set_child(Some(&list));

    let export = gtk::Button::from_icon_name("document-save-as-symbolic");
    export.set_tooltip_text(Some("Export all notes as Markdown"));
    export.add_css_class("flat");
    export.set_sensitive(false);
    export.update_property(&[gtk::accessible::Property::Label("Export notes")]);
    let send_export = sender.clone();
    export.connect_clicked(move |_| {
        send_export.emit(super::app::Msg::ExportNotes);
    });

    let sidebar_title = gtk::Label::new(Some("Notes"));
    sidebar_title.set_xalign(0.0);
    sidebar_title.set_hexpand(true);
    sidebar_title.add_css_class("heading");

    let sidebar_header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    sidebar_header.append(&sidebar_title);
    sidebar_header.append(&export);

    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 8);
    sidebar.set_margin_start(12);
    sidebar.set_margin_end(8);
    sidebar.set_margin_top(8);
    sidebar.set_margin_bottom(8);
    sidebar.set_width_request(240);
    sidebar.append(&sidebar_header);
    sidebar.append(&empty);
    sidebar.append(&list_scroll);

    let editor_title = gtk::Label::new(Some("Select a note"));
    editor_title.set_xalign(0.0);
    editor_title.set_hexpand(true);
    editor_title.add_css_class("heading");
    editor_title.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let editing = Rc::new(Cell::new(None));
    let actions = note_menu_button(id, sender.clone(), editing.clone());

    let content_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content_header.set_valign(gtk::Align::Center);
    content_header.append(&editor_title);
    content_header.append(&actions);
    attach_note_menu(&content_header, id, editing.clone(), sender.clone());

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let editor = note_editor(&buffer);
    let editor_scroll = gtk::ScrolledWindow::new();
    editor_scroll.set_hexpand(true);
    editor_scroll.set_vexpand(true);
    editor_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    editor_scroll.set_child(Some(&editor));
    // The tab shares the reading column. The verse dialog keeps its own padding.
    let column = column::Column::new(&editor_scroll, &editor, column_px);
    column.bind(sender.clone());

    let syncing = Rc::new(Cell::new(false));
    let send_save = sender.clone();
    let sync_save = syncing.clone();
    buffer.connect_changed(move |_| {
        if sync_save.get() {
            return;
        }
        send_save.emit(super::app::Msg::SaveNote(id));
    });

    let editor_pane = gtk::Box::new(gtk::Orientation::Vertical, 8);
    editor_pane.set_margin_start(8);
    editor_pane.set_margin_end(12);
    editor_pane.set_margin_top(8);
    editor_pane.set_margin_bottom(8);
    editor_pane.set_hexpand(true);
    editor_pane.append(&content_header);
    editor_pane.append(&column.root);

    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.set_start_child(Some(&sidebar));
    paned.set_end_child(Some(&editor_pane));
    paned.set_resize_start_child(false);
    paned.set_shrink_start_child(false);
    paned.set_position(280);
    paned.set_hexpand(true);
    paned.set_vexpand(true);

    NotesWidgets {
        root: paned.upcast(),
        list,
        notes: Vec::new(),
        empty,
        editor,
        buffer,
        editor_title,
        editing,
        syncing,
        export,
        actions,
        column,
        tab: id,
        sender,
    }
}

pub fn build_note_dialog(sender: relm4::Sender<super::app::Msg>) -> NoteDialog {
    let dialog = adw::Dialog::new();
    dialog.set_content_width(520);
    dialog.set_content_height(380);

    let delete = gtk::Button::with_label("Delete");
    delete.add_css_class("destructive-action");
    delete.set_sensitive(false);
    let send_del = sender.clone();
    delete.connect_clicked(move |btn| {
        confirm_delete_note(btn, {
            let send_del = send_del.clone();
            move || send_del.emit(super::app::Msg::DeleteDialogNote)
        });
    });

    let header = adw::HeaderBar::new();
    header.pack_start(&delete);

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    let editor = note_editor(&buffer);
    editor.set_sensitive(true);
    let editor_scroll = gtk::ScrolledWindow::new();
    editor_scroll.set_hexpand(true);
    editor_scroll.set_vexpand(true);
    editor_scroll.set_child(Some(&editor));

    let syncing = Rc::new(Cell::new(false));
    let send_save = sender;
    let sync_save = syncing.clone();
    buffer.connect_changed(move |_| {
        if sync_save.get() {
            return;
        }
        send_save.emit(super::app::Msg::SaveDialogNote);
    });

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&editor_scroll));
    dialog.set_child(Some(&toolbar));

    NoteDialog {
        dialog,
        editor,
        buffer,
        delete,
        editing: None,
        syncing,
    }
}

pub fn editor_text(widgets: &NotesWidgets) -> String {
    buffer_text(&widgets.buffer)
}

pub fn dialog_text(dialog: &NoteDialog) -> String {
    buffer_text(&dialog.buffer)
}

pub fn fill_bookmarks(widgets: &mut BookmarksWidgets, user: &Connection, books: &[Book]) {
    widgets.bookmarks = user_db::list_bookmarks(user).unwrap_or_default();
    refill_bookmarks(
        &widgets.list,
        &widgets.bookmarks,
        books,
        widgets.tab,
        &widgets.sender,
    );
    let has_bm = !widgets.bookmarks.is_empty();
    widgets.list.set_visible(has_bm);
    widgets.empty.set_visible(!has_bm);
}

pub fn fill_notes(widgets: &mut NotesWidgets, user: &Connection, books: &[Book]) {
    widgets.syncing.set(true);
    widgets.notes = user_db::list_notes(user).unwrap_or_default();
    refill_note_rows(
        &widgets.list,
        &widgets.notes,
        books,
        widgets.tab,
        &widgets.sender,
    );
    let has_notes = !widgets.notes.is_empty();
    widgets.list.set_visible(has_notes);
    widgets.empty.set_visible(!has_notes);
    widgets.export.set_sensitive(has_notes);

    if let Some(at) = widgets.editing.get() {
        if let Some(i) = widgets.notes.iter().position(|n| n.at() == at) {
            widgets
                .list
                .select_row(widgets.list.row_at_index(i as i32).as_ref());
            widgets.actions.set_sensitive(true);
        } else {
            widgets.list.unselect_all();
        }
    } else {
        widgets.actions.set_sensitive(false);
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

pub fn show_note(widgets: &mut NotesWidgets, books: &[Book], at: Ref, text: &str) {
    widgets.syncing.set(true);
    widgets.editing.set(Some(at));
    widgets.buffer.set_text(text);
    widgets.editor_title.set_label(&nav::format_ref(books, at));
    widgets.editor.set_sensitive(true);
    widgets.actions.set_sensitive(true);
    if let Some(i) = widgets.notes.iter().position(|n| n.at() == at) {
        widgets
            .list
            .select_row(widgets.list.row_at_index(i as i32).as_ref());
    }
    widgets.syncing.set(false);
}

pub fn load_note(widgets: &mut NotesWidgets, books: &[Book], at: Ref, text: &str) {
    if widgets.syncing.get() || widgets.editing.get() == Some(at) {
        return;
    }
    show_note(widgets, books, at, text);
}

pub fn clear_editor(widgets: &mut NotesWidgets) {
    widgets.syncing.set(true);
    widgets.editing.set(None);
    widgets.buffer.set_text("");
    widgets.syncing.set(false);
    widgets.editor_title.set_label("Select a note");
    widgets.editor.set_sensitive(false);
    widgets.actions.set_sensitive(false);
}

pub fn open_note_dialog(
    dialog: &mut NoteDialog,
    books: &[Book],
    at: Ref,
    text: &str,
    parent: Option<&impl IsA<gtk::Widget>>,
) {
    dialog.syncing.set(true);
    dialog.editing = Some(at);
    dialog.buffer.set_text(text);
    dialog.syncing.set(false);
    dialog.dialog.set_title(&nav::format_ref(books, at));
    dialog.delete.set_sensitive(!text.trim().is_empty());
    dialog.dialog.set_focus(Some(&dialog.editor));
    dialog.dialog.present(parent);
}

pub fn clear_dialog(dialog: &mut NoteDialog) {
    dialog.syncing.set(true);
    dialog.editing = None;
    dialog.buffer.set_text("");
    dialog.syncing.set(false);
    dialog.delete.set_sensitive(false);
}

fn note_editor(buffer: &gtk::TextBuffer) -> gtk::TextView {
    let editor = gtk::TextView::new();
    editor.set_buffer(Some(buffer));
    editor.set_wrap_mode(gtk::WrapMode::WordChar);
    editor.set_left_margin(12);
    editor.set_right_margin(12);
    editor.set_top_margin(8);
    editor.set_bottom_margin(8);
    editor.set_hexpand(true);
    editor.set_vexpand(true);
    editor.set_sensitive(false);
    editor.set_accessible_role(gtk::AccessibleRole::TextBox);
    editor.update_property(&[gtk::accessible::Property::Label("Note")]);
    editor
}

fn buffer_text(buffer: &gtk::TextBuffer) -> String {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).to_string()
}

fn confirm_delete_note(parent: &impl IsA<gtk::Widget>, on_delete: impl Fn() + 'static) {
    let dlg = adw::AlertDialog::new(Some("Delete this note?"), Some("This cannot be undone."));
    dlg.add_response("cancel", "Cancel");
    dlg.add_response("delete", "Delete");
    dlg.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    dlg.set_default_response(Some("cancel"));
    dlg.set_close_response("cancel");
    dlg.connect_response(None, move |_, response| {
        if response == "delete" {
            on_delete();
        }
    });
    dlg.present(Some(parent));
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

fn refill_note_rows(
    list: &gtk::ListBox,
    notes: &[Note],
    books: &[Book],
    id: TabId,
    sender: &relm4::Sender<super::app::Msg>,
) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for note in notes {
        list.append(&note_row(note.at(), books, &note.text, id, sender.clone()));
    }
}

fn refill_bookmarks(
    list: &gtk::ListBox,
    bookmarks: &[Bookmark],
    books: &[Book],
    id: TabId,
    sender: &relm4::Sender<super::app::Msg>,
) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for bm in bookmarks {
        list.append(&bookmark_row(
            bm.at(),
            books,
            bm.label.as_str(),
            id,
            sender.clone(),
        ));
    }
}

fn note_row(
    at: Ref,
    books: &[Book],
    extra: &str,
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    box_.set_margin_start(10);
    box_.set_margin_end(10);
    box_.set_margin_top(8);
    box_.set_margin_bottom(8);

    let title = gtk::Label::new(Some(&nav::format_ref(books, at)));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    box_.append(&title);

    let snippet = snippet(extra);
    if !snippet.is_empty() {
        let sub = gtk::Label::new(Some(&snippet));
        sub.set_xalign(0.0);
        sub.add_css_class("dim-label");
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        box_.append(&sub);
    }

    row.set_child(Some(&box_));
    row.set_activatable(false);
    row.set_tooltip_text(Some(&nav::format_ref(books, at)));
    attach_note_menu(&row, id, Rc::new(Cell::new(Some(at))), sender);
    row
}

fn bookmark_row(
    at: Ref,
    books: &[Book],
    extra: &str,
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::ListBoxRow {
    let row = ref_row(at, books, extra);

    let model = gio::Menu::new();
    model.append(Some("Open in a new tab"), Some("bookmark.tab"));
    model.append(Some("Remove"), Some("bookmark.remove"));
    let menu = gtk::PopoverMenu::from_model(Some(&model));
    menu.set_parent(&row);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);

    let group = gio::SimpleActionGroup::new();
    let tab = gio::SimpleAction::new("tab", None);
    let tx = sender.clone();
    tab.connect_activate(move |_, _| {
        tx.emit(super::app::Msg::OpenBookmarkTab(id, at));
    });
    let remove = gio::SimpleAction::new("remove", None);
    remove.connect_activate(move |_, _| {
        sender.emit(super::app::Msg::RemoveBookmark(at));
    });
    group.add_action(&tab);
    group.add_action(&remove);
    menu.insert_action_group("bookmark", Some(&group));

    let right = gtk::GestureClick::new();
    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.set_propagation_phase(gtk::PropagationPhase::Capture);
    let menu_click = menu.clone();
    right.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        menu_click.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        menu_click.popup();
    });
    row.add_controller(right);

    row.connect_destroy(move |_| {
        menu.unparent();
    });
    row
}

fn note_menu_model() -> gio::Menu {
    let model = gio::Menu::new();
    model.append(Some("Open verse"), Some("note.open"));
    model.append(Some("Open verse in new tab"), Some("note.tab"));
    let destructive = gio::Menu::new();
    destructive.append(Some("Delete"), Some("note.delete"));
    model.append_section(None, &destructive);
    model
}

fn note_actions(
    id: TabId,
    at: Rc<Cell<Option<Ref>>>,
    sender: relm4::Sender<super::app::Msg>,
    confirm_parent: gtk::Widget,
) -> gio::SimpleActionGroup {
    let group = gio::SimpleActionGroup::new();
    let open = gio::SimpleAction::new("open", None);
    let tx = sender.clone();
    let at_open = at.clone();
    open.connect_activate(move |_, _| {
        if let Some(at) = at_open.get() {
            tx.emit(super::app::Msg::OpenNoteVerse(id, at));
        }
    });
    let tab = gio::SimpleAction::new("tab", None);
    let tx = sender.clone();
    let at_tab = at.clone();
    tab.connect_activate(move |_, _| {
        if let Some(at) = at_tab.get() {
            tx.emit(super::app::Msg::OpenNoteTab(id, at));
        }
    });
    let delete = gio::SimpleAction::new("delete", None);
    delete.connect_activate(move |_, _| {
        let Some(at) = at.get() else {
            return;
        };
        confirm_delete_note(&confirm_parent, {
            let sender = sender.clone();
            move || sender.emit(super::app::Msg::DeleteNote(at))
        });
    });
    group.add_action(&open);
    group.add_action(&tab);
    group.add_action(&delete);
    group
}

fn note_menu_button(
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
    editing: Rc<Cell<Option<Ref>>>,
) -> gtk::MenuButton {
    let btn = gtk::MenuButton::new();
    btn.set_icon_name("view-more-symbolic");
    btn.set_tooltip_text(Some("Note actions"));
    btn.add_css_class("flat");
    btn.set_sensitive(false);
    btn.update_property(&[gtk::accessible::Property::Label("Note actions")]);
    btn.set_menu_model(Some(&note_menu_model()));
    let group = note_actions(id, editing, sender, btn.clone().upcast());
    btn.insert_action_group("note", Some(&group));
    if let Some(popover) = btn.popover() {
        popover.insert_action_group("note", Some(&group));
    }
    btn
}

fn attach_note_menu(
    widget: &impl IsA<gtk::Widget>,
    id: TabId,
    at: Rc<Cell<Option<Ref>>>,
    sender: relm4::Sender<super::app::Msg>,
) {
    let widget = widget.upcast_ref::<gtk::Widget>().clone();
    let menu = gtk::PopoverMenu::from_model(Some(&note_menu_model()));
    menu.set_parent(&widget);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);
    let group = note_actions(id, at.clone(), sender, widget.clone());
    menu.insert_action_group("note", Some(&group));

    let right = gtk::GestureClick::new();
    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.set_propagation_phase(gtk::PropagationPhase::Capture);
    let menu_click = menu.clone();
    right.connect_pressed(move |gesture, _, x, y| {
        if at.get().is_none() {
            return;
        }
        gesture.set_state(gtk::EventSequenceState::Claimed);
        menu_click.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        menu_click.popup();
    });
    widget.add_controller(right);
    widget.connect_destroy(move |_| {
        menu.unparent();
    });
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

#[cfg(test)]
mod tests {
    use super::gio;
    use super::snippet;
    use gio::prelude::MenuModelExt;

    #[test]
    fn snippet_keeps_a_short_first_line() {
        assert_eq!(snippet("Lamp, not floodlight."), "Lamp, not floodlight.");
    }

    #[test]
    fn snippet_truncates_a_long_first_line() {
        let long = "a".repeat(90);
        let out = snippet(&long);
        assert!(out.ends_with('…'));
        assert_eq!(out.chars().count(), 81);
    }

    #[test]
    fn snippet_uses_only_the_first_line() {
        assert_eq!(snippet("one\ntwo"), "one");
    }

    fn item_attr(menu: &gio::Menu, index: i32, name: &str) -> String {
        menu.item_attribute_value(index, name, None)
            .and_then(|v| v.get())
            .unwrap_or_default()
    }

    #[test]
    fn verse_menu_keeps_bookmark_label_and_shows_mhc_when_present() {
        let plain = super::verse_menu_model(false, false);
        assert_eq!(plain.n_items(), 4);
        assert_eq!(item_attr(&plain, 0, "label"), "Copy");
        assert_eq!(item_attr(&plain, 0, "action"), "win.copy-verse-here");
        assert_eq!(item_attr(&plain, 1, "label"), "Bookmark");
        assert_eq!(item_attr(&plain, 1, "action"), "win.toggle-bookmark");
        assert_eq!(item_attr(&plain, 1, "accel"), "<Control>d");
        assert_eq!(item_attr(&plain, 2, "label"), "Highlight");
        assert_eq!(item_attr(&plain, 3, "label"), "Add note");
        assert_eq!(item_attr(&plain, 3, "action"), "win.add-note");
        assert_eq!(item_attr(&plain, 3, "accel"), "<Control><Shift>n");

        let noted = super::verse_menu_model(true, false);
        assert_eq!(noted.n_items(), 4);
        assert_eq!(item_attr(&noted, 1, "label"), "Bookmark");
        assert_eq!(item_attr(&noted, 3, "label"), "Edit note");

        let mhc = super::verse_menu_model(false, true);
        assert_eq!(mhc.n_items(), 5);
        assert_eq!(item_attr(&mhc, 4, "label"), "Matthew Henry");
        assert_eq!(item_attr(&mhc, 4, "action"), "win.open-mhc-here");
    }
}
