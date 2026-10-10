use crate::cite;
use crate::occurrences;
use adw::prelude::*;
use bible_app_db::{Book, ClickedDict, DictEntry, StrongDef};
use gtk::gio;
use gtk::glib;
use relm4::{adw, gtk};

pub fn create(parent: &impl gtk::prelude::IsA<gtk::Widget>) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(parent);
    popover.set_autohide(true);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_accessible_role(gtk::AccessibleRole::Dialog);
    popover.add_css_class("word-popover");
    install_css();
    popover
}

#[allow(clippy::too_many_arguments)]
pub fn present(
    popover: &gtk::Popover,
    view: &gtk::TextView,
    start: i32,
    defs: &[StrongDef],
    counts: &[usize],
    dict: &ClickedDict,
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) {
    let tabs = source_tabs(!defs.is_empty(), dict);
    if tabs.is_empty() {
        return;
    }
    let stack = gtk::Stack::new();
    stack.set_hhomogeneous(true);
    stack.set_vhomogeneous(false);
    if !defs.is_empty() {
        stack.add_titled(
            &strongs_page(defs, counts, books, sender.clone()),
            Some("strongs"),
            "Strong's",
        );
    }
    for (module, entries) in group_lexicons(&dict.lexicons) {
        let name = lexicon_tab_label(module);
        stack.add_titled(
            &lexicon_page(&entries, books, sender.clone()),
            Some(module),
            &name,
        );
    }
    for entry in &dict.bible {
        let name = dict_tab_label(entry);
        stack.add_titled(
            &dict_page(entry, books, sender.clone()),
            Some(&entry.module),
            &name,
        );
    }
    for entry in &dict.topics {
        let name = dict_tab_label(entry);
        stack.add_titled(
            &dict_page(entry, books, sender.clone()),
            Some(&entry.module),
            &name,
        );
    }
    if let Some(entry) = &dict.english {
        let name = dict_tab_label(entry);
        stack.add_titled(
            &dict_page(entry, books, sender.clone()),
            Some(&entry.module),
            &name,
        );
    }
    let content = scrolled_page(stack.clone());
    let child: gtk::Widget = if tabs.len() == 1 {
        content.upcast()
    } else {
        with_source_sidebar(stack, content, &tabs)
    };
    popover.set_child(Some(&child));
    point_at_word(popover, view, start);
    popover.popup();
}

const PAGE_MAX_HEIGHT: i32 = 360;

fn scrolled_page(stack: gtk::Stack) -> gtk::ScrolledWindow {
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
    scroll.set_child(Some(&stack));
    scroll.add_css_class("word-body");
    scroll
}

fn scroll_body_to_top(scroll: &gtk::ScrolledWindow) {
    let adj = scroll.vadjustment();
    adj.set_value(adj.lower());
    // The new page is measured after this handler returns. Snap again once
    // that allocation has clamped the adjustment.
    let scroll = scroll.clone();
    glib::idle_add_local_once(move || {
        let adj = scroll.vadjustment();
        adj.set_value(adj.lower());
    });
}

fn with_source_sidebar(
    stack: gtk::Stack,
    content: gtk::ScrolledWindow,
    tabs: &[(String, String)],
) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.add_css_class("navigation-sidebar");
    list.set_accessible_role(gtk::AccessibleRole::List);
    list.update_property(&[gtk::accessible::Property::Label("Sources")]);

    for (_, title) in tabs {
        let label = gtk::Label::new(Some(title));
        label.set_xalign(0.0);
        label.set_wrap(false);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&label));
        list.append(&row);
    }

    let names: Vec<String> = tabs.iter().map(|(id, _)| id.clone()).collect();
    let body = content.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else { return };
        let index = row.index();
        if index < 0 {
            return;
        }
        let Some(name) = names.get(index as usize) else {
            return;
        };
        // One scrolled window wraps every source. Keep the next article at
        // the top instead of reusing the page the reader just left.
        stack.set_visible_child_name(name);
        scroll_body_to_top(&body);
    });
    if let Some(row) = list.row_at_index(0) {
        list.select_row(Some(&row));
    }

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_overlay_scrolling(false);
    scroll.set_propagate_natural_width(true);
    scroll.set_propagate_natural_height(true);
    scroll.set_min_content_width(108);
    scroll.set_max_content_height(PAGE_MAX_HEIGHT);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&list));
    scroll.add_css_class("word-sources");

    let sep = gtk::Separator::new(gtk::Orientation::Vertical);
    let wrap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    wrap.append(&scroll);
    wrap.append(&sep);
    wrap.append(&content);
    wrap.upcast()
}

fn install_css() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(
            r#"
            popover.word-popover contents {
              padding: 0;
            }
            .word-sources {
              padding: 4px 0;
            }
            .word-sources list {
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

fn source_tabs(has_strongs: bool, dict: &ClickedDict) -> Vec<(String, String)> {
    let mut tabs = Vec::new();
    if has_strongs {
        tabs.push(("strongs".into(), "Strong's".into()));
    }
    for (module, _) in group_lexicons(&dict.lexicons) {
        tabs.push((module.to_string(), lexicon_tab_label(module)));
    }
    for entry in dict
        .bible
        .iter()
        .chain(dict.topics.iter())
        .chain(dict.english.iter())
    {
        tabs.push((entry.module.clone(), dict_tab_label(entry)));
    }
    tabs
}

fn dict_tab_label(entry: &DictEntry) -> String {
    match entry.module.as_str() {
        "Webster" => "Webster's".into(),
        "Easton" => "Easton's".into(),
        "Smith" => "Smith's".into(),
        "Names" => "Hitchcock's".into(),
        "ATSD" => "ATS".into(),
        "Nave" => "Nave".into(),
        "Torrey" => "Torrey".into(),
        "BDB" => "BDB".into(),
        "Thayer" => "Thayer".into(),
        _ => entry
            .title
            .split_whitespace()
            .next()
            .unwrap_or("Dictionary")
            .to_string(),
    }
}

fn lexicon_tab_label(module: &str) -> String {
    match module {
        "BDB" => "BDB".into(),
        "Thayer" => "Thayer".into(),
        other => other.to_string(),
    }
}

fn group_lexicons(entries: &[DictEntry]) -> Vec<(&str, Vec<&DictEntry>)> {
    let mut groups: Vec<(&str, Vec<&DictEntry>)> = Vec::new();
    for entry in entries {
        if let Some((_, items)) = groups
            .iter_mut()
            .find(|(module, _)| *module == entry.module)
        {
            items.push(entry);
        } else {
            groups.push((entry.module.as_str(), vec![entry]));
        }
    }
    groups
}

fn library_button(
    sender: relm4::Sender<super::app::Msg>,
    module: &str,
    headword: &str,
) -> gtk::Button {
    let btn = gtk::Button::with_label("Open in library");
    btn.set_halign(gtk::Align::Start);
    btn.add_css_class("pill");
    btn.set_tooltip_text(Some("Open this word in the library"));
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

fn strongs_page(
    defs: &[StrongDef],
    counts: &[usize],
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::Box {
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

        let code = format!("{}{}", def.lang, def.num);
        let title = gtk::Label::new(Some(&code));
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
            let label = cite::linked_label(&text, books, sender.clone());
            label.set_max_width_chars(44);
            body.append(&label);
        }
        for see_code in see {
            body.append(&see_link(&see_code, sender.clone()));
        }
        body.append(&library_button(
            sender.clone(),
            bible_app_db::STRONGS_MODULE,
            &code,
        ));
        let count = counts.get(i).copied().unwrap_or(0);
        body.append(&occurrences_button(sender.clone(), &code, count));
    }
    body
}

fn see_link(code: &str, sender: relm4::Sender<super::app::Msg>) -> gtk::Label {
    let link = gtk::Label::new(None);
    link.set_markup(&format!("<a href=\"{code}\">See {code}</a>"));
    link.set_xalign(0.0);
    link.set_use_markup(true);
    let send = sender.clone();
    link.connect_activate_link(move |_, uri| {
        send.emit(super::app::Msg::OpenStrongsCode(uri.to_string()));
        glib::Propagation::Stop
    });
    attach_see_menu(&link, code, sender);
    link
}

fn attach_see_menu(
    widget: &impl gtk::prelude::IsA<gtk::Widget>,
    code: &str,
    sender: relm4::Sender<super::app::Msg>,
) {
    let model = gio::Menu::new();
    model.append(Some("Open in new tab"), Some("see.tab"));
    let menu = gtk::PopoverMenu::from_model(Some(&model));
    menu.set_parent(widget);
    menu.set_has_arrow(false);
    menu.set_halign(gtk::Align::Start);

    let group = gio::SimpleActionGroup::new();
    let tab = gio::SimpleAction::new("tab", None);
    let code = code.to_string();
    tab.connect_activate(move |_, _| {
        sender.emit(super::app::Msg::OpenStrongsCodeTab(code.clone()));
    });
    group.add_action(&tab);
    menu.insert_action_group("see", Some(&group));

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

fn occurrences_button(
    sender: relm4::Sender<super::app::Msg>,
    code: &str,
    count: usize,
) -> gtk::Button {
    let btn = gtk::Button::with_label(&occurrences::see_all_label(code, count));
    btn.set_halign(gtk::Align::Start);
    btn.add_css_class("pill");
    btn.set_tooltip_text(Some(
        "Open Search with every KJV verse tagged with this number",
    ));
    let code = code.to_string();
    btn.connect_clicked(move |_| {
        sender.emit(super::app::Msg::OpenStrongsOccurrences(code.clone()));
    });
    btn
}

fn lexicon_page(
    entries: &[&DictEntry],
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_width_request(300);

    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        append_dict_entry(&body, entry, books, sender.clone());
        body.append(&library_button(
            sender.clone(),
            &entry.module,
            &entry.headword,
        ));
    }
    body
}

fn dict_page(
    entry: &DictEntry,
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) -> gtk::Box {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
    body.set_margin_start(14);
    body.set_margin_end(14);
    body.set_margin_top(12);
    body.set_margin_bottom(12);
    body.set_width_request(300);

    append_dict_entry(&body, entry, books, sender.clone());
    body.append(&library_button(sender, &entry.module, &entry.headword));
    body
}

fn append_dict_entry(
    body: &gtk::Box,
    entry: &DictEntry,
    books: &[Book],
    sender: relm4::Sender<super::app::Msg>,
) {
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

    let text = cite::linked_label(&entry.text, books, sender);
    text.set_width_chars(40);
    text.set_max_width_chars(44);
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

    fn on_main_thread() -> bool {
        let Ok(tid) = std::fs::read_link("/proc/thread-self") else {
            return false;
        };
        tid.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse::<u32>().ok())
            == Some(std::process::id())
    }

    #[test]
    fn switching_source_scrolls_the_body_to_the_top() {
        if !on_main_thread() {
            eprintln!("skip source scroll test: GTK must run on the main thread");
            return;
        }
        let display =
            std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
        if !display {
            eprintln!("skip source scroll test: no display");
            return;
        }
        gtk::init().expect("gtk init");

        let stack = gtk::Stack::new();
        stack.set_vhomogeneous(false);
        for (name, title) in [("strongs", "Strong's"), ("easton", "Easton's")] {
            let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
            page.set_size_request(300, 2000);
            page.append(&gtk::Label::new(Some(title)));
            stack.add_titled(&page, Some(name), title);
        }
        let content = scrolled_page(stack.clone());
        let tabs = vec![
            ("strongs".into(), "Strong's".into()),
            ("easton".into(), "Easton's".into()),
        ];
        let shown = stack.clone();
        let wrap = with_source_sidebar(stack, content.clone(), &tabs);
        let list = find_list_box(&wrap).expect("source list");

        let adj = content.vadjustment();
        adj.set_upper(2000.0);
        adj.set_page_size(360.0);
        adj.set_value(480.0);
        assert!(
            adj.value() > 100.0,
            "fixture scroll should sit below the top"
        );

        let row = list.row_at_index(1).expect("second source");
        list.select_row(Some(&row));
        for _ in 0..50 {
            if !glib::MainContext::default().iteration(false) {
                break;
            }
        }

        let adj = content.vadjustment();
        assert_eq!(shown.visible_child_name().as_deref(), Some("easton"));
        assert_eq!(adj.value(), adj.lower());
    }

    fn find_list_box(widget: &gtk::Widget) -> Option<gtk::ListBox> {
        if let Ok(list) = widget.clone().downcast::<gtk::ListBox>() {
            return Some(list);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = find_list_box(&current) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }

    #[test]
    fn source_tabs_put_strongs_then_lexicon_then_dicts() {
        let dict = ClickedDict {
            lexicons: vec![entry("BDB", "H168")],
            bible: vec![entry("Easton", "Tabernacle"), entry("ATSD", "Tabernacle")],
            topics: vec![entry("Nave", "Tabernacle")],
            english: Some(entry("Webster", "tabernacle")),
        };
        assert_eq!(
            source_tabs(true, &dict)
                .iter()
                .map(|(_, title)| title.as_str())
                .collect::<Vec<_>>(),
            vec!["Strong's", "BDB", "Easton's", "ATS", "Nave", "Webster's",]
        );
    }

    #[test]
    fn topic_tab_labels_use_short_names() {
        let nave = DictEntry {
            module: "Nave".into(),
            title: "Nave's Topical Bible".into(),
            headword: "God".into(),
            text: "topic".into(),
        };
        let torrey = DictEntry {
            module: "Torrey".into(),
            title: "Torrey's Topical Textbook".into(),
            headword: "God".into(),
            text: "topic".into(),
        };
        assert_eq!(dict_tab_label(&nave), "Nave");
        assert_eq!(dict_tab_label(&torrey), "Torrey");
    }

    fn entry(module: &str, headword: &str) -> DictEntry {
        DictEntry {
            module: module.into(),
            title: module.into(),
            headword: headword.into(),
            text: "def".into(),
        }
    }

    #[test]
    fn lexicon_tabs_are_bdb_then_thayer() {
        let entries = vec![
            entry("BDB", "H430"),
            entry("Thayer", "G26"),
            entry("BDB", "H433"),
        ];
        let groups = group_lexicons(&entries);
        assert_eq!(
            groups
                .iter()
                .map(|(m, items)| (
                    *m,
                    items
                        .iter()
                        .map(|e| e.headword.as_str())
                        .collect::<Vec<_>>()
                ))
                .collect::<Vec<_>>(),
            vec![("BDB", vec!["H430", "H433"]), ("Thayer", vec!["G26"]),]
        );
    }
}
