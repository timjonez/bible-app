use adw::prelude::*;
use bible_app_db::StrongDef;
use relm4::{adw, gtk};

pub fn create(parent: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_accessible_role(gtk::AccessibleRole::Dialog);
    popover
}

pub fn present(popover: &gtk::Popover, view: &gtk::TextView, start: i32, defs: &[StrongDef]) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_width_request(280);

    for (i, def) in defs.iter().enumerate() {
        if i > 0 {
            body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        let title = gtk::Label::new(Some(&format!("{}{}", def.lang, def.num)));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_selectable(true);
        body.append(&title);

        if !def.lemma.is_empty() {
            let sub = if def.pronunciation.is_empty() {
                def.lemma.clone()
            } else {
                format!("{}  ({})", def.lemma, def.pronunciation)
            };
            let lemma = gtk::Label::new(Some(&sub));
            lemma.add_css_class("dim-label");
            lemma.set_xalign(0.0);
            lemma.set_wrap(true);
            lemma.set_selectable(true);
            body.append(&lemma);
        }

        if !def.definition.is_empty() {
            let text = gtk::Label::new(Some(&def.definition));
            text.set_wrap(true);
            text.set_max_width_chars(44);
            text.set_xalign(0.0);
            text.set_selectable(true);
            body.append(&text);
        }
    }

    popover.set_child(Some(&body));

    let buffer = view.buffer();
    let iter = buffer.iter_at_offset(start);
    let loc = view.iter_location(&iter);
    let (x, y) =
        view.buffer_to_window_coords(gtk::TextWindowType::Widget, loc.x(), loc.y() + loc.height());
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, loc.width().max(1), 1)));
    popover.popup();
}

pub fn make_tag() -> gtk::TextTag {
    let tag = gtk::TextTag::new(Some("strongs"));
    tag.set_underline(gtk::pango::Underline::Single);
    tag
}
