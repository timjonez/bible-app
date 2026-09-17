use adw::prelude::*;
use bible_app_db::{ClickedDict, DictEntry, StrongDef};
use gtk::glib;
use relm4::{adw, gtk};

pub fn create(parent: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_accessible_role(gtk::AccessibleRole::Dialog);
    popover
}

pub fn present(
    popover: &gtk::Popover,
    view: &gtk::TextView,
    start: i32,
    defs: &[StrongDef],
    dict: &ClickedDict,
    sender: relm4::Sender<super::app::Msg>,
) {
    let stack = gtk::Stack::new();
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(false);
    let mut pages = 0u32;
    if !defs.is_empty() {
        stack.add_titled(
            &strongs_page(defs, sender.clone()),
            Some("strongs"),
            "Strong's",
        );
        pages += 1;
    }
    for entry in &dict.bible {
        let name = dict_tab_label(entry);
        stack.add_titled(
            &dict_page(entry, sender.clone()),
            Some(&entry.module),
            &name,
        );
        pages += 1;
    }
    if let Some(entry) = &dict.english {
        let name = dict_tab_label(entry);
        stack.add_titled(
            &dict_page(entry, sender.clone()),
            Some(&entry.module),
            &name,
        );
        pages += 1;
    }
    if pages == 0 {
        return;
    }
    let child: gtk::Widget = if pages == 1 {
        stack.upcast()
    } else {
        let switcher = gtk::StackSwitcher::new();
        switcher.set_stack(Some(&stack));
        switcher.set_halign(gtk::Align::Center);
        let wrap = gtk::Box::new(gtk::Orientation::Vertical, 8);
        wrap.set_margin_top(8);
        wrap.append(&switcher);
        wrap.append(&stack);
        wrap.upcast()
    };
    popover.set_child(Some(&child));
    point_at_word(popover, view, start);
    popover.popup();
}

fn dict_tab_label(entry: &DictEntry) -> String {
    match entry.module.as_str() {
        "Webster" => "Webster's".into(),
        "Easton" => "Easton's".into(),
        "Smith" => "Smith's".into(),
        "Names" => "Hitchcock's".into(),
        "ATSD" => "ATS".into(),
        _ => entry
            .title
            .split_whitespace()
            .next()
            .unwrap_or("Dictionary")
            .to_string(),
    }
}

fn library_button(
    sender: relm4::Sender<super::app::Msg>,
    module: &str,
    headword: &str,
) -> gtk::Button {
    let btn = gtk::Button::with_label("Open in library");
    btn.set_halign(gtk::Align::Start);
    btn.add_css_class("pill");
    btn.set_tooltip_text(Some("Open this word in the library window"));
    let module = module.to_string();
    let headword = headword.to_string();
    btn.connect_clicked(move |_| {
        sender.emit(super::app::Msg::OpenDictWord {
            module: module.clone(),
            headword: headword.clone(),
        });
    });
    btn
}

fn strongs_page(defs: &[StrongDef], sender: relm4::Sender<super::app::Msg>) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_width_request(300);

    for (i, def) in defs.iter().enumerate() {
        if i > 0 {
            body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }

        if !def.lemma.is_empty() {
            let sub = if def.pronunciation.is_empty() {
                def.lemma.clone()
            } else {
                format!("{}  ({})", def.lemma, def.pronunciation)
            };
            let lemma = gtk::Label::new(Some(&sub));
            lemma.add_css_class("heading");
            lemma.set_xalign(0.0);
            lemma.set_wrap(true);
            lemma.set_selectable(true);
            body.append(&lemma);
        }

        let title = gtk::Label::new(Some(&format!("{}{}", def.lang, def.num)));
        if def.lemma.is_empty() {
            title.add_css_class("heading");
        } else {
            title.add_css_class("dim-label");
        }
        title.set_xalign(0.0);
        title.set_selectable(true);
        body.append(&title);

        let (text, see) = split_see_also(&def.definition, &def.lang);
        if !text.is_empty() {
            let label = gtk::Label::new(Some(&text));
            label.set_wrap(true);
            label.set_max_width_chars(44);
            label.set_xalign(0.0);
            label.set_selectable(true);
            body.append(&label);
        }
        for code in see {
            let link = gtk::Label::new(None);
            link.set_markup(&format!("<a href=\"{code}\">See {code}</a>"));
            link.set_xalign(0.0);
            link.set_use_markup(true);
            let send = sender.clone();
            link.connect_activate_link(move |_, uri| {
                send.emit(super::app::Msg::OpenStrongsCode(uri.to_string()));
                glib::Propagation::Stop
            });
            body.append(&link);
        }
    }
    if let Some(def) = defs.first() {
        let code = format!("{}{}", def.lang, def.num);
        body.append(&library_button(
            sender,
            bible_app_db::STRONGS_MODULE,
            &code,
        ));
    }
    body
}

fn dict_page(entry: &DictEntry, sender: relm4::Sender<super::app::Msg>) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_width_request(300);

    append_dict_entry(&body, entry);
    body.append(&library_button(sender, &entry.module, &entry.headword));

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_min_content_height(80);
    scroll.set_max_content_height(320);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&body));

    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
    wrap.append(&scroll);
    wrap
}

fn append_dict_entry(body: &gtk::Box, entry: &DictEntry) {
    let source = gtk::Label::new(Some(&entry.title));
    source.add_css_class("dim-label");
    source.add_css_class("caption");
    source.set_xalign(0.0);
    source.set_wrap(true);
    body.append(&source);

    let head = gtk::Label::new(Some(&entry.headword));
    head.add_css_class("heading");
    head.set_xalign(0.0);
    head.set_wrap(true);
    head.set_selectable(true);
    body.append(&head);

    let text = gtk::Label::new(Some(&entry.text));
    text.set_wrap(true);
    text.set_max_width_chars(44);
    text.set_xalign(0.0);
    text.set_selectable(true);
    body.append(&text);
}

fn point_at_word(popover: &gtk::Popover, view: &gtk::TextView, start: i32) {
    let buffer = view.buffer();
    let iter = buffer.iter_at_offset(start);
    let loc = view.iter_location(&iter);
    let (x, y) =
        view.buffer_to_window_coords(gtk::TextWindowType::Widget, loc.x(), loc.y() + loc.height());
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, loc.width().max(1), 1)));
}

pub fn present_text(popover: &gtk::Popover, view: &gtk::TextView, start: i32, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.set_wrap(true);
    label.set_max_width_chars(44);
    label.set_xalign(0.0);
    label.set_selectable(true);
    label.set_margin_start(14);
    label.set_margin_end(14);
    label.set_margin_top(12);
    label.set_margin_bottom(12);
    popover.set_child(Some(&label));
    point_at_word(popover, view, start);
    popover.popup();
}

pub fn make_tag() -> gtk::TextTag {
    gtk::TextTag::new(Some("strongs"))
}

/// Pull trailing/inline "See H433" / "See 430" refs out of a Strong's definition.
pub fn split_see_also(definition: &str, default_lang: &str) -> (String, Vec<String>) {
    let s: Vec<char> = definition.chars().collect();
    let mut used = vec![false; s.len()];
    let mut codes = Vec::new();
    let lang0 = if default_lang.eq_ignore_ascii_case("G") {
        "G"
    } else {
        "H"
    };
    let mut i = 0usize;
    while i < s.len() {
        if let Some((code, end)) = see_ref_at(&s, i, lang0) {
            if !codes.iter().any(|c| c == &code) {
                codes.push(code);
            }
            used[i..end].fill(true);
            i = end;
            continue;
        }
        i += 1;
    }
    let mut body = String::new();
    for (idx, ch) in s.iter().enumerate() {
        if !used[idx] {
            body.push(*ch);
        }
    }
    let body = collapse_blank_lines(body.trim());
    (body, codes)
}

fn see_ref_at(s: &[char], i: usize, default_lang: &str) -> Option<(String, usize)> {
    if i + 3 > s.len() {
        return None;
    }
    let see = s[i].eq_ignore_ascii_case(&'s')
        && s[i + 1].eq_ignore_ascii_case(&'e')
        && s[i + 2].eq_ignore_ascii_case(&'e');
    if !see {
        return None;
    }
    let before_ok = i == 0 || !s[i - 1].is_alphanumeric();
    if !before_ok {
        return None;
    }
    let mut j = i + 3;
    if j >= s.len() || !s[j].is_whitespace() {
        return None;
    }
    while j < s.len() && s[j].is_whitespace() {
        j += 1;
    }
    let (lang, k) = if j < s.len()
        && (s[j].eq_ignore_ascii_case(&'h') || s[j].eq_ignore_ascii_case(&'g'))
        && j + 1 < s.len()
        && s[j + 1].is_ascii_digit()
    {
        (s[j].to_ascii_uppercase().to_string(), j + 1)
    } else {
        (default_lang.to_string(), j)
    };
    if k >= s.len() || !s[k].is_ascii_digit() {
        return None;
    }
    let mut k2 = k;
    while k2 < s.len() && s[k2].is_ascii_digit() {
        k2 += 1;
    }
    let num: String = s[k..k2].iter().collect();
    let mut end = k2;
    if end < s.len() && matches!(s[end], '.' | ',') {
        end += 1;
    }
    Some((format!("{lang}{num}"), end))
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::new();
    let mut blank = 0u8;
    for line in s.lines() {
        if line.trim().is_empty() {
            blank = blank.saturating_add(1);
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
            if blank > 0 {
                out.push('\n');
            }
        }
        out.push_str(line.trim_end());
        blank = 0;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn see_h_line_becomes_a_code() {
        let (body, codes) = split_see_also(
            "plural of 433; gods in the ordinary sense.\n\nSee H433",
            "H",
        );
        assert_eq!(codes, vec!["H433".to_string()]);
        assert!(body.contains("plural of 433"));
        assert!(!body.to_ascii_lowercase().contains("see h433"));
    }

    #[test]
    fn see_without_letter_inherits_lang() {
        let (body, codes) = split_see_also("a deity or the Deity:--God, god. See 430.", "H");
        assert_eq!(codes, vec!["H430".to_string()]);
        assert!(body.contains("Deity"));
        assert!(!body.to_ascii_lowercase().contains("see 430"));
    }

    #[test]
    fn several_see_lines() {
        let (_, codes) = split_see_also("See H410 \nSee H430", "H");
        assert_eq!(codes, vec!["H410".to_string(), "H430".to_string()]);
    }
}
