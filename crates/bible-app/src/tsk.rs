use crate::cite;
use crate::column;
use crate::history::History;
use crate::layout;
use crate::nav::{self, Ref};
use crate::picker;
use crate::shell;
use crate::tsk_parse::Citation;
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::{Book, Resource};
use gtk::gio;
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct TskWidgets {
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
    pub column: column::Column,
    loaded: Cell<(u8, u8)>,
    sections: RefCell<Vec<(u8, i32)>>,
}

pub fn build(column_px: i32) -> TskWidgets {
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
    view.set_top_margin(8);
    view.set_bottom_margin(16);
    view.set_hexpand(true);
    view.set_vexpand(true);
    view.set_accessible_role(gtk::AccessibleRole::Document);

    let menu = cite::menu_for(&view);

    let text_scroll = gtk::ScrolledWindow::new();
    text_scroll.set_hexpand(true);
    text_scroll.set_vexpand(true);
    text_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    text_scroll.set_child(Some(&view));
    let column = column::Column::new(&text_scroll, &view, column_px);

    let page = shell::bar_page(&bar.row, &column.root);
    let links = Rc::new(RefCell::new(Vec::new()));
    TskWidgets {
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
        column,
        loaded: Cell::new((0, 0)),
        sections: RefCell::new(Vec::new()),
    }
}

pub fn wire(
    widgets: &TskWidgets,
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
    books: &[Book],
) {
    widgets.column.bind(sender.clone());
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

pub fn show(widgets: &TskWidgets, conn: &Connection, books: &[Book], at: Ref) {
    let key = (at.book, at.chapter);
    if widgets.loaded.get() != key {
        let rows =
            bible_app_db::chapter_resources(conn, "TSK", at.book, at.chapter).unwrap_or_default();
        paint(widgets, books, &rows, at);
        widgets.loaded.set(key);
    }
    scroll_to(widgets, at.verse);
}

fn paint(widgets: &TskWidgets, books: &[Book], rows: &[Resource], at: Ref) {
    if rows.is_empty() {
        widgets.sections.borrow_mut().clear();
        widgets.links.borrow_mut().clear();
        widgets.buffer.set_text(&format!(
            "No Treasury of Scripture Knowledge on {}.",
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

fn scroll_to(widgets: &TskWidgets, verse: u8) {
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

pub fn create_popover(parent: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_accessible_role(gtk::AccessibleRole::Dialog);
    popover.add_css_class("tsk-popover");
    install_css();
    popover
}

const PAGE_MAX_HEIGHT: i32 = 360;

pub(crate) struct PhraseDest {
    at: Ref,
    title: String,
    markup: String,
}

pub(crate) fn phrase_dests(
    dests: &[Ref],
    books: &[Book],
    conn: Option<&Connection>,
) -> Vec<PhraseDest> {
    dests
        .iter()
        .map(|at| PhraseDest {
            at: *at,
            title: nav::format_ref(books, *at),
            markup: dest_markup(conn, *at),
        })
        .collect()
}

pub fn present_phrase(
    popover: &gtk::Popover,
    view: &gtk::TextView,
    start: i32,
    heading: &str,
    dests: &[PhraseDest],
    sender: relm4::Sender<super::app::Msg>,
) {
    let title = gtk::Label::new(Some(heading));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.set_margin_start(12);
    title.set_margin_end(12);
    title.set_margin_top(10);
    title.set_margin_bottom(8);

    let child: gtk::Widget = if dests.is_empty() {
        title.upcast()
    } else {
        let (verse, verse_title, verse_text) = verse_page();
        let selected = Rc::new(Cell::new(dests.first().map(|item| item.at)));
        attach_dest_menu(&verse, selected.clone(), sender.clone());
        let list = dest_list(dests, selected, verse_title, verse_text, sender);
        let content = scrolled_verse(verse);
        let pane = with_ref_sidebar(list, content);
        let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
        wrap.append(&title);
        wrap.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        wrap.append(&pane);
        wrap.upcast()
    };
    popover.set_child(Some(&child));

    let buffer = view.buffer();
    let iter = buffer.iter_at_offset(start);
    let loc = view.iter_location(&iter);
    let (x, y) =
        view.buffer_to_window_coords(gtk::TextWindowType::Widget, loc.x(), loc.y() + loc.height());
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, loc.width().max(1), 1)));
    popover.popup();
}

fn dest_list(
    items: &[PhraseDest],
    selected: Rc<Cell<Option<Ref>>>,
    verse_title: gtk::Label,
    verse_text: gtk::Label,
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    list.set_accessible_role(gtk::AccessibleRole::List);
    list.update_property(&[gtk::accessible::Property::Label("References")]);

    let width_chars = items
        .iter()
        .map(|item| item.title.chars().count())
        .max()
        .unwrap_or(0)
        .min(i32::MAX as usize) as i32;
    for item in items {
        list.append(&dest_row(item, width_chars, sender.clone()));
    }

    let items = items
        .iter()
        .map(|item| (item.at, item.title.clone(), item.markup.clone()))
        .collect::<Vec<_>>();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else {
            return;
        };
        let Ok(idx) = usize::try_from(row.index()) else {
            return;
        };
        let Some((at, title, markup)) = items.get(idx) else {
            return;
        };
        selected.set(Some(*at));
        verse_title.set_text(title);
        verse_text.set_markup(markup);
    });
    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }
    list
}

fn dest_row(
    item: &PhraseDest,
    width_chars: i32,
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    let label = gtk::Label::new(Some(&item.title));
    label.set_xalign(0.0);
    label.set_wrap(false);
    label.set_width_chars(width_chars);
    row.set_child(Some(&label));
    row.set_activatable(false);
    attach_dest_menu(&row, Rc::new(Cell::new(Some(item.at))), sender);
    row
}

fn verse_page() -> (gtk::Box, gtk::Label, gtk::Label) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_width_request(300);

    let title = gtk::Label::new(None);
    title.add_css_class("heading");
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.set_selectable(true);

    let text = gtk::Label::new(None);
    text.set_wrap(true);
    text.set_wrap_mode(gtk::pango::WrapMode::Word);
    text.set_max_width_chars(44);
    text.set_xalign(0.0);
    text.set_yalign(0.0);
    text.set_selectable(true);
    text.set_use_markup(true);

    body.append(&title);
    body.append(&text);
    (body, title, text)
}

fn scrolled_verse(child: impl gtk::prelude::IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_overlay_scrolling(false);
    scroll.set_min_content_height(96);
    scroll.set_max_content_height(PAGE_MAX_HEIGHT);
    scroll.set_min_content_width(300);
    scroll.set_propagate_natural_height(true);
    scroll.set_propagate_natural_width(true);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&child));
    scroll.add_css_class("tsk-body");
    scroll
}

fn with_ref_sidebar(list: gtk::ListBox, content: gtk::ScrolledWindow) -> gtk::Widget {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_overlay_scrolling(false);
    scroll.set_propagate_natural_width(true);
    scroll.set_propagate_natural_height(true);
    scroll.set_hexpand(false);
    scroll.set_max_content_height(PAGE_MAX_HEIGHT);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    scroll.add_css_class("tsk-sources");

    let sep = gtk::Separator::new(gtk::Orientation::Vertical);
    let wrap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    wrap.append(&scroll);
    wrap.append(&sep);
    wrap.append(&content);
    wrap.upcast()
}

fn attach_dest_menu(
    widget: &impl gtk::prelude::IsA<gtk::Widget>,
    at: Rc<Cell<Option<Ref>>>,
    sender: relm4::Sender<super::app::Msg>,
) {
    let model = gio::Menu::new();
    model.append(Some("Open"), Some("dest.open"));
    model.append(Some("Open in new tab"), Some("dest.tab"));
    let menu = gtk::PopoverMenu::from_model(Some(&model));
    menu.set_parent(widget);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);

    let group = gio::SimpleActionGroup::new();
    let open = gio::SimpleAction::new("open", None);
    let at_open = at.clone();
    let tx = sender.clone();
    open.connect_activate(move |_, _| {
        if let Some(at) = at_open.get() {
            tx.emit(super::app::Msg::OpenTskDest(at));
        }
    });
    let tab = gio::SimpleAction::new("tab", None);
    tab.connect_activate(move |_, _| {
        if let Some(at) = at.get() {
            sender.emit(super::app::Msg::OpenTskDestTab(at));
        }
    });
    group.add_action(&open);
    group.add_action(&tab);
    menu.insert_action_group("dest", Some(&group));

    let right = gtk::GestureClick::new();
    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.set_propagation_phase(gtk::PropagationPhase::Capture);
    let menu_click = menu.clone();
    right.connect_pressed(move |gesture, _, x, y| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        menu_click.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        menu_click.popup();
    });
    widget.add_controller(right);
    widget.connect_destroy(move |_| {
        menu.unparent();
    });
}

fn dest_markup(conn: Option<&Connection>, at: Ref) -> String {
    let Some(conn) = conn else {
        return String::new();
    };
    match bible_app_db::get_verse(conn, at.book, at.chapter, at.verse) {
        Ok(v) => {
            let (stored, _) = layout::split_notes(&v.text);
            let (text, italics) = layout::strip_supplied(&stored);
            markup_with_italics(&text, &italics)
        }
        Err(_) => glib::markup_escape_text("This verse is not in the library.").to_string(),
    }
}

fn markup_with_italics(text: &str, italics: &[layout::Span]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as i32;
    let mut out = String::new();
    let mut at = 0i32;
    for span in italics {
        let start = span.start.clamp(0, len);
        let end = span.end.clamp(start, len);
        if start > at {
            out.push_str(&escape_chars(&chars, at, start));
        }
        if end > start {
            out.push_str("<i>");
            out.push_str(&escape_chars(&chars, start, end));
            out.push_str("</i>");
        }
        at = at.max(end);
    }
    if at < len {
        out.push_str(&escape_chars(&chars, at, len));
    }
    out
}

fn escape_chars(chars: &[char], start: i32, end: i32) -> String {
    let s: String = chars[start as usize..end as usize].iter().collect();
    glib::markup_escape_text(&s).to_string()
}

fn install_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            popover.tsk-popover contents {
              padding: 0;
            }
            .tsk-sources {
              padding: 4px 0;
            }
            .tsk-sources list {
              background: transparent;
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
    use crate::layout::Span;

    #[test]
    fn italics_wrap_supplied_words() {
        let italics = [Span { start: 4, end: 7 }];
        assert_eq!(markup_with_italics("the man", &italics), "the <i>man</i>");
    }

    #[test]
    fn markup_escapes_ampersand() {
        assert_eq!(markup_with_italics("a & b", &[]), "a &amp; b");
    }

    #[test]
    fn markup_plain_text_is_unchanged() {
        assert_eq!(
            markup_with_italics("And the remnant of the meat offering", &[]),
            "And the remnant of the meat offering"
        );
    }
}
