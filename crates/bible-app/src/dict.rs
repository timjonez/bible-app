use crate::cite;
use crate::history::History;
use crate::occurrences;
use crate::shell;
use crate::tsk_parse::Citation;
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::{self, Book, DictHit, DictModule};
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct DictWidgets {
    pub root: gtk::Widget,
    pub search: gtk::SearchEntry,
    pub popover: gtk::Popover,
    pub list: gtk::ListBox,
    pub buffer: gtk::TextBuffer,
    pub view: gtk::TextView,
    pub menu: gtk::PopoverMenu,
    pub links: Rc<RefCell<Vec<Citation>>>,
    /// Passage opened from a citation when this view could not sit beside one.
    pub companion: Cell<Option<TabId>>,
    pub prev: gtk::Button,
    pub next: gtk::Button,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub modules: Vec<DictModule>,
    pub module_id: Option<String>,
    pub hits: Vec<DictHit>,
    pub query: String,
    /// `-1` when no entry is open.
    pub entry_i: Cell<i32>,
    pub trail: RefCell<History<i32>>,
    books: Vec<Book>,
    see_kjv: gtk::Button,
    occ_code: Rc<RefCell<String>>,
    syncing: Rc<Cell<bool>>,
}

pub fn build(id: TabId, sender: relm4::Sender<super::app::Msg>, books: &[Book]) -> DictWidgets {
    let bar = shell::location_bar(
        "Previous entry (Alt+Left)",
        "Next entry (Alt+Right)",
        "Previous entry",
        "Next entry",
    );
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search a headword"));
    search.set_width_chars(24);
    search.set_hexpand(false);
    search.set_valign(gtk::Align::Center);
    search.update_property(&[gtk::accessible::Property::Label("Search a headword")]);
    bar.pickers.append(&search);

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
    // A modal popover takes keyboard focus when it opens, so the first
    // letter of a headword would land on the top hit instead of the entry.
    popover.set_autohide(false);
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
        send_q.emit(super::app::Msg::DictSearch(id, entry.text().to_string()));
    });
    let send_activate = sender.clone();
    search.connect_activate(move |_| {
        send_activate.emit(super::app::Msg::DictOpen(id, 0));
    });
    let send_row = sender.clone();
    list.connect_row_activated(move |_, row| {
        send_row.emit(super::app::Msg::DictOpen(id, row.index()));
    });

    wire_suggestions(&search, &popover, &list);

    let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
    cite::add_tag(&buffer);
    let view = gtk::TextView::new();
    view.set_buffer(Some(&buffer));
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_direction(gtk::TextDirection::Ltr);
    view.set_left_margin(20);
    view.set_right_margin(20);
    view.set_top_margin(16);
    view.set_bottom_margin(16);
    view.set_accessible_role(gtk::AccessibleRole::Document);
    let text_scroll = gtk::ScrolledWindow::new();
    text_scroll.set_hexpand(true);
    text_scroll.set_vexpand(true);
    text_scroll.set_child(Some(&view));

    let occ_code = Rc::new(RefCell::new(String::new()));
    let see_kjv = gtk::Button::with_label("See all in the KJV");
    see_kjv.set_halign(gtk::Align::Start);
    see_kjv.add_css_class("pill");
    see_kjv.set_tooltip_text(Some(
        "Open Search with every KJV verse tagged with this number",
    ));
    see_kjv.set_visible(false);
    see_kjv.set_margin_start(12);
    see_kjv.set_margin_top(8);
    see_kjv.set_margin_bottom(4);
    let send_occ = sender.clone();
    let occ_click = occ_code.clone();
    see_kjv.connect_clicked(move |_| {
        let code = occ_click.borrow().clone();
        if !code.is_empty() {
            send_occ.emit(super::app::Msg::OpenStrongsOccurrences(code));
        }
    });

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.append(&see_kjv);
    body.append(&text_scroll);
    let page = shell::bar_page(&bar.row, &body);

    install_css();
    wire_suggestion_dismiss(&search, &popover, &page);

    let popover_destroy = popover.clone();
    page.connect_destroy(move |_| {
        popover_destroy.unparent();
    });

    let menu = cite::menu_for(&view);
    let links = Rc::new(RefCell::new(Vec::new()));
    let widgets = DictWidgets {
        root: page.upcast(),
        search,
        popover,
        list,
        buffer,
        view,
        menu,
        links,
        companion: Cell::new(None),
        prev: bar.prev,
        next: bar.next,
        back: bar.back,
        forward: bar.forward,
        modules: Vec::new(),
        module_id: None,
        hits: Vec::new(),
        query: String::new(),
        entry_i: Cell::new(-1),
        trail: RefCell::new(History::new(0)),
        books: books.to_vec(),
        see_kjv,
        occ_code,
        syncing,
    };
    sync_history(&widgets);
    widgets
}

pub fn wire(widgets: &DictWidgets, id: TabId, sender: relm4::Sender<super::app::Msg>) {
    let tx = sender.clone();
    widgets
        .prev
        .connect_clicked(move |_| tx.emit(super::app::Msg::LibraryPrev(id)));
    let tx = sender.clone();
    widgets
        .next
        .connect_clicked(move |_| tx.emit(super::app::Msg::LibraryNext(id)));
    let tx = sender.clone();
    widgets
        .back
        .connect_clicked(move |_| tx.emit(super::app::Msg::LibraryBack(id)));
    let tx = sender.clone();
    widgets
        .forward
        .connect_clicked(move |_| tx.emit(super::app::Msg::LibraryForward(id)));
    cite::wire(&widgets.view, widgets.links.clone(), id, sender);
}

pub fn sync_history(widgets: &DictWidgets) {
    let open = widgets.entry_i.get() >= 0;
    let trail = widgets.trail.borrow();
    widgets.prev.set_sensitive(open);
    widgets.next.set_sensitive(open);
    widgets.back.set_sensitive(open && trail.can_back());
    widgets.forward.set_sensitive(open && trail.can_forward());
}

pub fn load_modules(widgets: &mut DictWidgets, conn: &Connection) {
    let mut modules = bible_app_db::lexicon_modules(conn).unwrap_or_default();
    modules.extend(bible_app_db::dictionary_modules(conn).unwrap_or_default());
    if !modules.iter().any(|m| m.id == bible_app_db::STRONGS_MODULE) {
        modules.insert(
            0,
            bible_app_db::DictModule {
                id: bible_app_db::STRONGS_MODULE.into(),
                title: "Strong's".into(),
                kind: "dictionary".into(),
            },
        );
    }
    widgets.modules = modules;
}

pub fn select_module(widgets: &mut DictWidgets, id: &str) {
    let changed = widgets.module_id.as_deref() != Some(id);
    widgets.module_id = Some(id.to_string());
    if changed {
        widgets.entry_i.set(-1);
    }
    widgets.syncing.set(true);
    widgets.search.set_text("");
    widgets.syncing.set(false);
    widgets.query.clear();
    widgets.hits.clear();
    refill_list(&widgets.list, &[]);
    widgets.popover.popdown();
    widgets.occ_code.borrow_mut().clear();
    widgets.see_kjv.set_visible(false);
    paint_text(widgets, "Search a headword to open its entry.");
    sync_history(widgets);
}

pub fn current_module(widgets: &DictWidgets) -> Option<&str> {
    widgets.module_id.as_deref()
}

pub fn tab_title(widgets: &DictWidgets) -> String {
    widgets
        .module_id
        .as_deref()
        .and_then(|id| widgets.modules.iter().find(|m| m.id == id))
        .map(|m| m.title.clone())
        .unwrap_or_else(|| "Library".into())
}

pub fn open_headword(widgets: &mut DictWidgets, conn: &Connection, module: &str, headword: &str) {
    select_module(widgets, module);
    widgets.query = headword.to_string();
    widgets.syncing.set(true);
    widgets.search.set_text(headword);
    widgets.syncing.set(false);
    search_hits(widgets, conn, false);
    let idx = widgets
        .hits
        .iter()
        .position(|h| h.headword.eq_ignore_ascii_case(headword))
        .unwrap_or(0);
    if !widgets.hits.is_empty() {
        open_hit(widgets, conn, idx as i32);
    }
}

pub fn search(widgets: &mut DictWidgets, conn: &Connection) {
    search_hits(widgets, conn, true);
}

fn search_hits(widgets: &mut DictWidgets, conn: &Connection, show_popover: bool) {
    let Some(module) = current_module(widgets).map(str::to_string) else {
        widgets.hits.clear();
        refill_list(&widgets.list, &[]);
        widgets.popover.popdown();
        paint_text(widgets, "No dictionaries or topics in this database.");
        return;
    };
    let q = widgets.query.trim();
    if q.is_empty() {
        widgets.hits.clear();
        refill_list(&widgets.list, &[]);
        widgets.popover.popdown();
        return;
    }
    widgets.hits = if module == bible_app_db::STRONGS_MODULE {
        bible_app_db::search_strongs(conn, q, bible_app_db::ENTRY_LIMIT).unwrap_or_default()
    } else {
        bible_app_db::search_entries(conn, &module, q, bible_app_db::ENTRY_LIMIT)
            .unwrap_or_default()
    };
    refill_list(&widgets.list, &widgets.hits);
    if widgets.hits.is_empty() {
        widgets.popover.popdown();
        return;
    }
    widgets
        .list
        .select_row(widgets.list.row_at_index(0).as_ref());
    if !show_popover {
        widgets.popover.popdown();
        return;
    }
    let width = widgets.search.width();
    if width > 0 {
        widgets.popover.set_size_request(width, -1);
    }
    widgets.popover.popup();
}

fn paint_text(widgets: &DictWidgets, text: &str) {
    let text = with_ltr_base(text);
    widgets.buffer.set_text(&text);
    widgets
        .links
        .replace(cite::relink(&widgets.buffer, &text, &widgets.books));
}

/// GtkTextView takes paragraph direction from the first strong character, so a
/// BDB line that starts with Hebrew would right-align. A leading LRM (U+200E)
/// pins each paragraph to LTR; Hebrew runs still render RTL.
fn with_ltr_base(text: &str) -> String {
    const LRM: char = '\u{200E}';
    let mut out = String::with_capacity(text.len() + 8);
    let mut at_line = true;
    for c in text.chars() {
        if at_line && c != '\n' {
            out.push(LRM);
            at_line = false;
        }
        out.push(c);
        if c == '\n' {
            at_line = true;
        }
    }
    out
}

/// Show entry `i` and return the headword to store on the tab.
pub fn display(widgets: &DictWidgets, conn: &Connection, i: i32) -> Option<String> {
    let module = current_module(widgets)?.to_string();
    let loaded = if module == bible_app_db::STRONGS_MODULE {
        bible_app_db::get_strongs_entry(conn, i)
    } else {
        bible_app_db::get_entry(conn, &module, i)
    };
    match loaded {
        Ok(Some((head, text))) => {
            widgets.syncing.set(true);
            widgets.search.set_text(&head);
            widgets.syncing.set(false);
            paint_text(widgets, &format!("{head}\n\n{text}"));
            widgets.popover.popdown();
            widgets.entry_i.set(i);
            let key = if module == bible_app_db::STRONGS_MODULE {
                head.split_whitespace()
                    .next()
                    .unwrap_or(head.as_str())
                    .to_string()
            } else {
                head
            };
            if module == bible_app_db::STRONGS_MODULE {
                if let Some((num, lang)) = bible_app_db::parse_strongs_code(&key) {
                    let code = format!("{lang}{num}");
                    let count = bible_app_db::strongs_occurrence_count(conn, &code).unwrap_or(0);
                    *widgets.occ_code.borrow_mut() = code.clone();
                    widgets
                        .see_kjv
                        .set_label(&occurrences::see_all_label(&code, count));
                    widgets.see_kjv.set_visible(true);
                } else {
                    widgets.occ_code.borrow_mut().clear();
                    widgets.see_kjv.set_visible(false);
                }
            } else {
                widgets.occ_code.borrow_mut().clear();
                widgets.see_kjv.set_visible(false);
            }
            Some(key)
        }
        Ok(None) => {
            widgets.occ_code.borrow_mut().clear();
            widgets.see_kjv.set_visible(false);
            paint_text(widgets, "Entry missing.");
            None
        }
        Err(e) => {
            widgets.occ_code.borrow_mut().clear();
            widgets.see_kjv.set_visible(false);
            paint_text(widgets, &e.to_string());
            None
        }
    }
}

pub fn open_hit(widgets: &DictWidgets, conn: &Connection, idx: i32) -> Option<String> {
    let idx = usize::try_from(idx).ok()?;
    let i = widgets.hits.get(idx)?.i;
    let was_empty = widgets.entry_i.get() < 0;
    let key = display(widgets, conn, i)?;
    {
        let mut trail = widgets.trail.borrow_mut();
        if was_empty {
            trail.restart(i);
        } else {
            trail.navigate(i);
        }
    }
    sync_history(widgets);
    Some(key)
}

pub fn step(widgets: &DictWidgets, conn: &Connection, forward: bool) -> Option<String> {
    let module = current_module(widgets)?.to_string();
    let i = widgets.entry_i.get();
    if i < 0 {
        return None;
    }
    let hit = if module == bible_app_db::STRONGS_MODULE {
        bible_app_db::adjacent_strongs(conn, i, forward).ok()?
    } else {
        bible_app_db::adjacent_entry(conn, &module, i, forward).ok()?
    }?;
    let key = display(widgets, conn, hit.i)?;
    widgets.trail.borrow_mut().navigate(hit.i);
    sync_history(widgets);
    Some(key)
}

pub fn history_step(widgets: &DictWidgets, conn: &Connection, forward: bool) -> Option<String> {
    if widgets.entry_i.get() < 0 {
        return None;
    }
    let i = {
        let mut trail = widgets.trail.borrow_mut();
        if forward {
            trail.forward()?
        } else {
            trail.back()?
        }
    };
    let key = display(widgets, conn, i);
    sync_history(widgets);
    key
}

/// Keys while the suggestion list is open. The entry keeps the caret;
/// Down moves into the highlighted hit, and Escape closes the list.
fn wire_suggestions(search: &gtk::SearchEntry, popover: &gtk::Popover, list: &gtk::ListBox) {
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let pop_keys = popover.clone();
    let list_keys = list.clone();
    keys.connect_key_pressed(move |_, keyval, _, _| {
        if !pop_keys.is_visible() || focus_inside(&pop_keys) {
            return glib::Propagation::Proceed;
        }
        if keyval == gtk::gdk::Key::Escape {
            pop_keys.popdown();
            return glib::Propagation::Stop;
        }
        if matches!(keyval, gtk::gdk::Key::Down | gtk::gdk::Key::KP_Down) {
            if let Some(row) = list_keys
                .selected_row()
                .or_else(|| list_keys.row_at_index(0))
            {
                list_keys.select_row(Some(&row));
                row.grab_focus();
            }
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    search.add_controller(keys);

    let list_keys = gtk::EventControllerKey::new();
    list_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let list_nav = list.clone();
    let search_nav = search.clone();
    let pop_nav = popover.clone();
    list_keys.connect_key_pressed(move |_, keyval, _, _| {
        if keyval == gtk::gdk::Key::Escape {
            pop_nav.popdown();
            search_nav.grab_focus();
            return glib::Propagation::Stop;
        }
        let at_top = list_nav
            .selected_row()
            .map(|row| row.index() <= 0)
            .unwrap_or(true);
        if at_top && matches!(keyval, gtk::gdk::Key::Up | gtk::gdk::Key::KP_Up) {
            search_nav.grab_focus();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    list.add_controller(list_keys);
}

/// Close the non-modal suggestion list when it is no longer the thing in use.
fn wire_suggestion_dismiss(search: &gtk::SearchEntry, popover: &gtk::Popover, page: &gtk::Box) {
    let focus = gtk::EventControllerFocus::new();
    let pop_leave = popover.clone();
    focus.connect_leave(move |_| {
        let pop = pop_leave.clone();
        glib::idle_add_local_once(move || {
            if pop.is_visible() && !focus_inside(&pop) {
                pop.popdown();
            }
        });
    });
    search.add_controller(focus);

    let pop_unmap = popover.clone();
    search.connect_unmap(move |_| {
        pop_unmap.popdown();
    });

    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    let pop_click = popover.clone();
    let search_click = search.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        if !pop_click.is_visible() {
            return;
        }
        let Some(widget) = gesture.widget() else {
            return;
        };
        if let Some(picked) = widget.pick(x, y, gtk::PickFlags::DEFAULT) {
            if contains_widget(&search_click, &picked) {
                return;
            }
        }
        pop_click.popdown();
    });
    page.add_controller(click);

    let active_watch: Rc<RefCell<Option<(gtk::Window, glib::SignalHandlerId)>>> =
        Rc::new(RefCell::new(None));
    let watch = active_watch.clone();
    let pop_active = popover.clone();
    search.connect_root_notify(move |entry| {
        let previous = watch.borrow_mut().take();
        if let Some((window, id)) = previous {
            window.disconnect(id);
        }
        let Some(window) = entry.root().and_downcast::<gtk::Window>() else {
            return;
        };
        let pop = pop_active.clone();
        let id = window.connect_is_active_notify(move |window| {
            if !window.is_active() {
                pop.popdown();
            }
        });
        *watch.borrow_mut() = Some((window, id));
    });
    let watch = active_watch.clone();
    page.connect_destroy(move |_| {
        let previous = watch.borrow_mut().take();
        if let Some((window, id)) = previous {
            window.disconnect(id);
        }
    });
}

fn focus_inside(container: &impl IsA<gtk::Widget>) -> bool {
    let container = container.as_ref();
    container
        .root()
        .and_then(|root| root.focus())
        .is_some_and(|focus| &focus == container || focus.is_ancestor(container))
}

fn contains_widget(ancestor: &impl IsA<gtk::Widget>, widget: &gtk::Widget) -> bool {
    let ancestor = ancestor.as_ref();
    widget == ancestor || ancestor.is_ancestor(widget)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_ltr_base_prefixes_each_nonempty_line() {
        const LRM: char = '\u{200E}';
        let out = with_ltr_base("H3190\n\n[יָטַב] vb. be good");
        let lines: Vec<&str> = out.split('\n').collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with(LRM));
        assert_eq!(lines[0].chars().skip(1).collect::<String>(), "H3190");
        assert_eq!(lines[1], "");
        assert!(lines[2].starts_with(LRM));
        assert!(lines[2].contains("[יָטַב] vb. be good"));
    }
}
