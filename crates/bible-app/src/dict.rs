use adw::prelude::*;
use bible_app_db::{self, DictHit, DictModule};
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::Cell;
use std::rc::Rc;

pub struct DictWidgets {
    pub window: adw::ApplicationWindow,
    pub title: adw::WindowTitle,
    pub search: gtk::SearchEntry,
    pub popover: gtk::Popover,
    pub list: gtk::ListBox,
    pub buffer: gtk::TextBuffer,
    pub modules: Vec<DictModule>,
    pub module_id: Option<String>,
    pub hits: Vec<DictHit>,
    pub query: String,
    syncing: Rc<Cell<bool>>,
}

pub fn open(sender: relm4::Sender<super::app::Msg>) -> DictWidgets {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("Library"));
    window.set_default_size(560, 740);

    let title = adw::WindowTitle::new("Library", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search a headword"));
    search.set_hexpand(true);
    search.update_property(&[gtk::accessible::Property::Label("Search a headword")]);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.set_accessible_role(gtk::AccessibleRole::List);
    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_min_content_height(80);
    list_scroll.set_max_content_height(280);
    list_scroll.set_propagate_natural_height(true);
    list_scroll.set_child(Some(&list));

    let popover = gtk::Popover::new();
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_offset(0, 4);
    popover.add_css_class("dict-search-popover");
    popover.set_child(Some(&list_scroll));
    popover.set_parent(&search);

    let syncing = Rc::new(Cell::new(false));
    let send_q = sender.clone();
    let sync_search = syncing.clone();
    search.connect_search_changed(move |entry| {
        if sync_search.get() {
            return;
        }
        send_q.emit(super::app::Msg::DictSearch(entry.text().to_string()));
    });
    let send_activate = sender.clone();
    search.connect_activate(move |_| {
        send_activate.emit(super::app::Msg::DictOpen(0));
    });
    let send_row = sender.clone();
    list.connect_row_activated(move |_, row| {
        send_row.emit(super::app::Msg::DictOpen(row.index()));
    });

    let keys = gtk::EventControllerKey::new();
    let list_keys = list.clone();
    keys.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Down {
            if let Some(row) = list_keys.selected_row().or_else(|| list_keys.row_at_index(0))
            {
                let next = list_keys.row_at_index(row.index() + 1).unwrap_or(row);
                list_keys.select_row(Some(&next));
                next.grab_focus();
            }
            return glib::Propagation::Stop;
        }
        if keyval == gtk::gdk::Key::Up {
            if let Some(row) = list_keys.selected_row() {
                let prev = list_keys
                    .row_at_index((row.index() - 1).max(0))
                    .unwrap_or(row);
                list_keys.select_row(Some(&prev));
                prev.grab_focus();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    search.add_controller(keys);

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
    body.append(&search);
    body.append(&text_scroll);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));
    window.set_content(Some(&toolbar));

    install_css();

    let popover_destroy = popover.clone();
    window.connect_destroy(move |_| {
        popover_destroy.unparent();
    });
    window.connect_close_request(move |_| {
        sender.emit(super::app::Msg::DictClosed);
        glib::Propagation::Proceed
    });
    window.present();
    search.grab_focus();
    DictWidgets {
        window,
        title,
        search,
        popover,
        list,
        buffer,
        modules: Vec::new(),
        module_id: None,
        hits: Vec::new(),
        query: String::new(),
        syncing,
    }
}

pub fn load_modules(widgets: &mut DictWidgets, conn: &Connection) {
    widgets.modules = bible_app_db::dictionary_modules(conn).unwrap_or_default();
}

pub fn select_module(widgets: &mut DictWidgets, id: &str) {
    widgets.module_id = Some(id.to_string());
    if let Some(m) = widgets.modules.iter().find(|m| m.id == id) {
        widgets.title.set_title(&m.title);
        widgets.title.set_subtitle("");
        widgets.window.set_title(Some(&m.title));
    }
    widgets.syncing.set(true);
    widgets.search.set_text("");
    widgets.syncing.set(false);
    widgets.query.clear();
    widgets.hits.clear();
    refill_list(&widgets.list, &[]);
    widgets.popover.popdown();
    widgets
        .buffer
        .set_text("Search a headword to open its entry.");
}

pub fn current_module(widgets: &DictWidgets) -> Option<&str> {
    widgets.module_id.as_deref()
}

pub fn search(widgets: &mut DictWidgets, conn: &Connection) {
    let Some(module) = current_module(widgets).map(str::to_string) else {
        widgets.hits.clear();
        refill_list(&widgets.list, &[]);
        widgets.popover.popdown();
        widgets
            .buffer
            .set_text("No dictionaries or topics in this database.");
        return;
    };
    let q = widgets.query.trim();
    if q.is_empty() {
        widgets.hits.clear();
        refill_list(&widgets.list, &[]);
        widgets.popover.popdown();
        return;
    }
    widgets.hits =
        bible_app_db::search_entries(conn, &module, q, bible_app_db::ENTRY_LIMIT).unwrap_or_default();
    refill_list(&widgets.list, &widgets.hits);
    if widgets.hits.is_empty() {
        widgets.popover.popdown();
        return;
    }
    widgets.list.select_row(widgets.list.row_at_index(0).as_ref());
    let width = widgets.search.width();
    if width > 0 {
        widgets.popover.set_size_request(width, -1);
    }
    widgets.popover.popup();
}

pub fn open_hit(widgets: &DictWidgets, conn: &Connection, idx: i32) {
    let Ok(idx) = usize::try_from(idx) else {
        return;
    };
    let Some(hit) = widgets.hits.get(idx) else {
        return;
    };
    let Some(module) = current_module(widgets) else {
        return;
    };
    match bible_app_db::get_entry(conn, module, hit.i) {
        Ok(Some((head, text))) => {
            widgets.syncing.set(true);
            widgets.search.set_text(&head);
            widgets.syncing.set(false);
            widgets.buffer.set_text(&format!("{head}\n\n{text}"));
            widgets.popover.popdown();
        }
        Ok(None) => widgets.buffer.set_text("Entry missing."),
        Err(e) => widgets.buffer.set_text(&e.to_string()),
    }
}

fn refill_list(list: &gtk::ListBox, hits: &[DictHit]) {
    while let Some(child) = list.row_at_index(0) {
        list.remove(&child);
    }
    for hit in hits {
        let row = gtk::ListBoxRow::new();
        let label = gtk::Label::new(Some(&hit.headword));
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

fn install_css() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            popover.dict-search-popover contents {
              background-color: var(--popover-bg-color);
              color: var(--popover-fg-color);
              padding: 4px;
              border-radius: 9px;
            }
            "#,
        );
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    });
}
