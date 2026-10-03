use crate::nav::Ref;
use crate::tsk_parse::{self, Citation};
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::Book;
use gtk::gio;
use relm4::{adw, gtk};
use std::cell::RefCell;
use std::rc::Rc;

pub fn add_tag(buffer: &gtk::TextBuffer) {
    let cite = gtk::TextTag::new(Some("cite"));
    buffer.tag_table().add(&cite);
    cite.set_underline(gtk::pango::Underline::Single);
}

pub fn menu_for(view: &gtk::TextView) -> gtk::PopoverMenu {
    let menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
    menu.set_parent(view);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);
    let menu_on_destroy = menu.clone();
    view.connect_destroy(move |_| {
        menu_on_destroy.unparent();
    });
    menu
}

pub fn apply(buffer: &gtk::TextBuffer, links: &[Citation]) {
    if let Some(tag) = buffer.tag_table().lookup("cite") {
        for link in links {
            let s = buffer.iter_at_offset(link.start);
            let e = buffer.iter_at_offset(link.end);
            buffer.apply_tag(&tag, &s, &e);
        }
    }
}

pub fn relink(buffer: &gtk::TextBuffer, text: &str, books: &[Book]) -> Vec<Citation> {
    let links = tsk_parse::citations(text, books);
    apply(buffer, &links);
    crate::theme::paint_buffer(buffer);
    links
}

pub fn at(links: &RefCell<Vec<Citation>>, offset: i32) -> Option<Ref> {
    links
        .borrow()
        .iter()
        .find(|link| offset >= link.start && offset < link.end)
        .map(|link| link.at)
}

pub fn wire(
    view: &gtk::TextView,
    links: Rc<RefCell<Vec<Citation>>>,
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
) {
    let click_view = view.clone();
    let click_links = links.clone();
    let click = gtk::GestureClick::new();
    click.set_button(1);
    let tx = sender.clone();
    click.connect_pressed(move |gesture, _, x, y| {
        let Some(offset) = offset_at(&click_view, x, y) else {
            return;
        };
        if at(&click_links, offset).is_none() {
            return;
        }
        gesture.set_state(gtk::EventSequenceState::Claimed);
        tx.emit(super::app::Msg::ClickCite { id, offset });
    });
    view.add_controller(click);

    let right_view = view.clone();
    let right_links = links.clone();
    let right = gtk::GestureClick::new();
    right.set_button(gtk::gdk::BUTTON_SECONDARY);
    right.set_propagation_phase(gtk::PropagationPhase::Capture);
    let tx = sender;
    right.connect_pressed(move |gesture, _, x, y| {
        let Some(offset) = offset_at(&right_view, x, y) else {
            return;
        };
        if at(&right_links, offset).is_none() {
            return;
        }
        gesture.set_state(gtk::EventSequenceState::Claimed);
        tx.emit(super::app::Msg::CiteMenu {
            id,
            offset,
            x: x as i32,
            y: y as i32,
        });
    });
    view.add_controller(right);

    let motion_view = view.clone();
    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(move |_, x, y| {
        let over = offset_at(&motion_view, x, y).is_some_and(|offset| at(&links, offset).is_some());
        let cursor = if over {
            gtk::gdk::Cursor::from_name("pointer", None)
        } else {
            None
        };
        motion_view.set_cursor(cursor.as_ref());
    });
    let leave_view = view.clone();
    motion.connect_leave(move |_| {
        leave_view.set_cursor(None);
    });
    view.add_controller(motion);
}

pub fn popup(
    menu: &gtk::PopoverMenu,
    x: i32,
    y: i32,
    at: Ref,
    id: TabId,
    sender: relm4::Sender<super::app::Msg>,
) {
    let model = gio::Menu::new();
    model.append(Some("Open in new tab"), Some("cite.tab"));
    model.append(Some("Open in new window"), Some("cite.window"));
    let group = gio::SimpleActionGroup::new();
    let tab = gio::SimpleAction::new("tab", None);
    let tx = sender.clone();
    tab.connect_activate(move |_, _| {
        tx.emit(super::app::Msg::OpenCiteTab { id, at });
    });
    let window = gio::SimpleAction::new("window", None);
    window.connect_activate(move |_, _| {
        sender.emit(super::app::Msg::OpenCiteWindow { id, at });
    });
    group.add_action(&tab);
    group.add_action(&window);
    menu.insert_action_group("cite", Some(&group));
    menu.set_menu_model(Some(&model));
    menu.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, 1, 1)));
    menu.popup();
}

pub fn linked_label(
    text: &str,
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_markup(&tsk_parse::markup_with_cites(text, books));
    label.set_use_markup(true);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::Word);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.connect_activate_link(move |_, uri| {
        if let Some(at) = tsk_parse::parse_cite_href(uri) {
            sender.emit(super::app::Msg::OpenTskDest(at));
        }
        gtk::glib::Propagation::Stop
    });
    label
}

fn offset_at(view: &gtk::TextView, x: f64, y: f64) -> Option<i32> {
    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
    view.iter_at_location(bx, by).map(|iter| iter.offset())
}
