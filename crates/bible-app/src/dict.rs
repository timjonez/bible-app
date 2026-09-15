use adw::prelude::*;
use bible_app_db::{self, DictHit, DictModule};
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct DictWidgets {
    pub window: adw::ApplicationWindow,
    pub title: adw::WindowTitle,
    pub list: gtk::ListBox,
    pub buffer: gtk::TextBuffer,
    pub dropdown: gtk::DropDown,
    pub modules: Vec<DictModule>,
    pub hits: Vec<DictHit>,
    pub query: String,
}

pub fn open(sender: relm4::Sender<super::app::Msg>) -> DictWidgets {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("Library"));
    window.set_default_size(560, 740);

    let title = adw::WindowTitle::new("Library", "");
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));

    let dropdown = gtk::DropDown::from_strings(&[]);
    dropdown.set_valign(gtk::Align::Center);
    dropdown.set_tooltip_text(Some("Choose a dictionary or topical work"));
    let send_mod = sender.clone();
    dropdown.connect_selected_notify(move |dd| {
        let _ = dd;
        send_mod.emit(super::app::Msg::DictModule);
    });
    header.pack_end(&dropdown);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Headword or topic"));
    search.set_tooltip_text(Some("Search headwords and topics"));
    search.set_hexpand(true);
    let send_q = sender.clone();
    search.connect_search_changed(move |entry| {
        send_q.emit(super::app::Msg::DictSearch(entry.text().to_string()));
    });

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("boxed-list");
    list.set_accessible_role(gtk::AccessibleRole::List);
    let send_row = sender.clone();
    list.connect_row_activated(move |_, row| {
        send_row.emit(super::app::Msg::DictOpen(row.index()));
    });
    let send_sel = sender.clone();
    list.connect_row_selected(move |_, row| {
        if let Some(row) = row {
            send_sel.emit(super::app::Msg::DictOpen(row.index()));
        }
    });
    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_min_content_height(160);
    list_scroll.set_max_content_height(260);
    list_scroll.set_propagate_natural_height(true);
    list_scroll.set_child(Some(&list));

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
    body.append(&list_scroll);
    body.append(&text_scroll);

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&body));
    window.set_content(Some(&toolbar));

    window.connect_close_request(move |_| {
        sender.emit(super::app::Msg::DictClosed);
        glib::Propagation::Proceed
    });
    window.present();
    search.grab_focus();
    DictWidgets {
        window,
        title,
        list,
        buffer,
        dropdown,
        modules: Vec::new(),
        hits: Vec::new(),
        query: String::new(),
    }
}

pub fn load_modules(widgets: &mut DictWidgets, conn: &Connection) {
    widgets.modules = bible_app_db::dictionary_modules(conn).unwrap_or_default();
    let titles: Vec<String> = widgets.modules.iter().map(|m| m.title.clone()).collect();
    let refs: Vec<&str> = titles.iter().map(String::as_str).collect();
    let model = gtk::StringList::new(&refs);
    widgets.dropdown.set_model(Some(&model));
    if !widgets.modules.is_empty() {
        widgets.dropdown.set_selected(0);
        widgets.title.set_subtitle(&widgets.modules[0].title);
    }
}

pub fn current_module(widgets: &DictWidgets) -> Option<&str> {
    let i = widgets.dropdown.selected() as usize;
    widgets.modules.get(i).map(|m| m.id.as_str())
}

pub fn search(widgets: &mut DictWidgets, conn: &Connection) {
    let Some(module) = current_module(widgets).map(str::to_string) else {
        widgets.hits.clear();
        refill_list(&widgets.list, &[]);
        widgets
            .buffer
            .set_text("No dictionaries or topics in this database.");
        return;
    };
    if let Some(m) = widgets.modules.iter().find(|m| m.id == module) {
        widgets.title.set_subtitle(&m.title);
    }
    widgets.hits =
        bible_app_db::search_entries(conn, &module, &widgets.query, bible_app_db::ENTRY_LIMIT)
            .unwrap_or_default();
    refill_list(&widgets.list, &widgets.hits);
    if widgets.hits.is_empty() {
        widgets.buffer.set_text(&format!(
            "No headword starting with {:?} in {}.",
            widgets.query.trim(),
            module
        ));
    }
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
            widgets.buffer.set_text(&format!("{head}\n\n{text}"));
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
