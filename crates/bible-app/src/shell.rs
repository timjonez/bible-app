use adw::prelude::*;
use gtk::gio;
use relm4::{adw, gtk};

pub struct PaneHost {
    pub root: gtk::Box,
    pub bar: adw::TabBar,
    pub view: adw::TabView,
    pub new_btn: gtk::Button,
}

impl PaneHost {
    fn new() -> Self {
        let view = adw::TabView::new();
        view.set_hexpand(true);
        view.set_vexpand(true);
        let bar = adw::TabBar::new();
        bar.set_view(Some(&view));
        bar.set_autohide(false);
        bar.set_expand_tabs(false);

        let new_btn = gtk::Button::from_icon_name("list-add-symbolic");
        new_btn.set_tooltip_text(Some("New tab (Ctrl+T)"));
        new_btn.add_css_class("flat");
        new_btn.update_property(&[gtk::accessible::Property::Label("New tab")]);
        // The tab strip normally takes the leftover width, which pins the
        // new-tab button to the far end. Size the strip to the tabs so the
        // button sits against the last one.
        if let Some(strip) = tab_strip(&bar) {
            strip.set_hexpand(false);
            strip.set_propagate_natural_width(true);
        }
        bar.set_end_action_widget(Some(&new_btn));

        bar.set_hexpand(true);
        bar.set_valign(gtk::Align::Fill);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.append(&view);
        Self {
            root,
            bar,
            view,
            new_btn,
        }
    }
}

/// The scrollable tab strip. The pinned strip does not expand.
fn tab_strip(bar: &adw::TabBar) -> Option<gtk::ScrolledWindow> {
    fn walk(widget: &gtk::Widget) -> Option<gtk::ScrolledWindow> {
        if let Ok(scrolled) = widget.clone().downcast::<gtk::ScrolledWindow>() {
            if scrolled.hexpands() {
                return Some(scrolled);
            }
        }
        let mut child = widget.first_child();
        while let Some(widget) = child {
            if let Some(found) = walk(&widget) {
                return Some(found);
            }
            child = widget.next_sibling();
        }
        None
    }
    walk(bar.upcast_ref())
}

pub struct SplitShell {
    pub host: PaneHost,
    pub search_btn: gtk::Button,
}

impl SplitShell {
    pub fn new() -> Self {
        let search_btn = gtk::Button::from_icon_name("system-search-symbolic");
        search_btn.set_tooltip_text(Some("Search (Ctrl+F)"));
        search_btn.add_css_class("flat");
        search_btn.set_valign(gtk::Align::Center);
        search_btn.update_property(&[gtk::accessible::Property::Label("Search")]);
        Self {
            host: PaneHost::new(),
            search_btn,
        }
    }

    /// Place the tab bar on the left of the header, with search and the menu
    /// after it.
    ///
    /// The header centers its title against the trailing controls, which
    /// insets the first tab. Hiding those controls and putting the menu in
    /// the title row lets the tabs start at the left edge.
    pub fn attach_header(&self, header: &adw::HeaderBar, menu: &gtk::MenuButton) {
        if menu.parent().is_some() {
            header.remove(menu);
        }
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);
        header.set_show_title(true);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.set_hexpand(true);
        row.set_valign(gtk::Align::Fill);
        self.host.bar.set_hexpand(true);
        row.append(&self.host.bar);
        row.append(&self.search_btn);
        menu.set_valign(gtk::Align::Center);
        row.append(menu);
        let controls = gtk::WindowControls::new(gtk::PackType::End);
        controls.set_valign(gtk::Align::Center);
        row.append(&controls);
        header.set_title_widget(Some(&row));
    }
}

/// Back and forward through one passage's history, at the start of its bar.
pub struct HistoryNav {
    pub row: gtk::Box,
    pub back: gtk::Button,
    pub forward: gtk::Button,
}

pub fn history_nav() -> HistoryNav {
    let back = gtk::Button::from_icon_name("edit-undo-symbolic");
    back.set_tooltip_text(Some("Back in history (Alt+Shift+Left)"));
    back.add_css_class("flat");
    back.set_sensitive(false);
    back.update_property(&[gtk::accessible::Property::Label("Back")]);
    let forward = gtk::Button::from_icon_name("edit-redo-symbolic");
    forward.set_tooltip_text(Some("Forward in history (Alt+Shift+Right)"));
    forward.add_css_class("flat");
    forward.set_sensitive(false);
    forward.update_property(&[gtk::accessible::Property::Label("Forward")]);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.add_css_class("linked");
    row.set_valign(gtk::Align::Center);
    row.append(&back);
    row.append(&forward);
    HistoryNav { row, back, forward }
}

/// The passage bar: history on the left, previous and next around the widgets
/// the caller packs into `pickers` (book and chapter, or a headword search).
pub struct LocationBar {
    pub row: gtk::CenterBox,
    pub prev: gtk::Button,
    pub next: gtk::Button,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub pickers: gtk::Box,
    /// Always-allocated trailing slot. MHC/TSK pack the follow pin here so a
    /// later show still gets width; a CenterBox end child that starts hidden
    /// often stays at zero size.
    pub end: gtk::Box,
}

pub fn location_bar(
    prev_tip: &str,
    next_tip: &str,
    prev_label: &str,
    next_label: &str,
) -> LocationBar {
    let prev = gtk::Button::from_icon_name("go-previous-symbolic");
    prev.set_tooltip_text(Some(prev_tip));
    prev.add_css_class("flat");
    prev.set_valign(gtk::Align::Center);
    prev.update_property(&[gtk::accessible::Property::Label(prev_label)]);
    let next = gtk::Button::from_icon_name("go-next-symbolic");
    next.set_tooltip_text(Some(next_tip));
    next.add_css_class("flat");
    next.set_valign(gtk::Align::Center);
    next.update_property(&[gtk::accessible::Property::Label(next_label)]);

    let pickers = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    pickers.set_valign(gtk::Align::Center);

    let cluster = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    cluster.set_halign(gtk::Align::Center);
    cluster.set_valign(gtk::Align::Center);
    cluster.append(&prev);
    cluster.append(&pickers);
    cluster.append(&next);

    let end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    end.set_valign(gtk::Align::Center);
    end.set_halign(gtk::Align::End);

    let history = history_nav();
    let row = gtk::CenterBox::new();
    row.set_orientation(gtk::Orientation::Horizontal);
    row.set_hexpand(true);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_start_widget(Some(&history.row));
    row.set_center_widget(Some(&cluster));
    row.set_end_widget(Some(&end));

    LocationBar {
        row,
        prev,
        next,
        back: history.back,
        forward: history.forward,
        pickers,
        end,
    }
}

/// Navigation row, a separator, then the tab body.
pub fn bar_page(bar: &impl IsA<gtk::Widget>, body: &impl IsA<gtk::Widget>) -> gtk::Box {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.set_hexpand(true);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_hexpand(true);
    page.set_vexpand(true);
    page.append(bar);
    page.append(&separator);
    page.append(body);
    page
}

/// A reader window other than the main one.
pub struct SideChrome {
    pub window: adw::ApplicationWindow,
    pub shell: SplitShell,
    pub goto_entry: gtk::Entry,
    pub goto_popover: gtk::Popover,
}

pub fn open_side_window(menu: &impl IsA<gio::MenuModel>) -> SideChrome {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("bible-app"));
    window.set_default_size(960, 720);

    let menu_btn = gtk::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_tooltip_text(Some("Menu"));
    menu_btn.set_primary(true);
    menu_btn.add_css_class("primary-menu");
    menu_btn.set_menu_model(Some(menu));

    let header = adw::HeaderBar::new();

    let goto_entry = gtk::Entry::new();
    goto_entry.set_placeholder_text(Some("John 3:16"));
    goto_entry.set_tooltip_text(Some(
        "Go to a reference (Enter). Shift+Enter opens beside (Ctrl+L)",
    ));
    goto_entry.set_width_chars(18);
    goto_entry.update_property(&[gtk::accessible::Property::Label("Go to reference")]);
    let goto_popover = gtk::Popover::new();
    goto_popover.set_autohide(true);
    goto_popover.add_css_class("goto-popover");
    goto_popover.set_child(Some(&goto_entry));
    goto_popover.set_parent(&header);

    let shell = SplitShell::new();
    shell.attach_header(&header, &menu_btn);
    let toolbar = adw::ToolbarView::new();
    toolbar.set_top_bar_style(adw::ToolbarStyle::Flat);
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&shell.host.root));
    window.set_content(Some(&toolbar));
    window.present();

    SideChrome {
        window,
        shell,
        goto_entry,
        goto_popover,
    }
}

/// The paned handle is the only resize control once a tab is split.
pub fn mark_split_handle(paned: &gtk::Paned) {
    fn apply(paned: &gtk::Paned) {
        let mut child = paned.first_child();
        while let Some(widget) = child {
            if widget.css_name().as_str() == "separator" {
                widget.set_tooltip_text(Some("Drag to resize"));
                widget.set_cursor_from_name(Some("ew-resize"));
                widget.update_property(&[gtk::accessible::Property::Label("Drag to resize")]);
                return;
            }
            child = widget.next_sibling();
        }
    }
    apply(paned);
    let paned = paned.clone();
    paned.connect_realize(apply);
}

pub fn tab_menu_model() -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Open beside"), Some("win.tab-open-beside"));
    menu.append(Some("Open in a window"), Some("win.tab-detach"));
    menu
}

/// Pin on an MHC/TSK location bar. Shown only when that view is a guest of a chapter.
///
/// `margin_end` clears the guest-pane close overlay (28px button + 8px inset)
/// so the pin is not drawn under the X.
pub fn follow_pin() -> gtk::ToggleButton {
    let btn = gtk::ToggleButton::new();
    btn.set_icon_name("view-pin-symbolic");
    btn.set_tooltip_text(Some("Follow the chapter"));
    btn.add_css_class("flat");
    btn.set_valign(gtk::Align::Center);
    btn.set_margin_end(36);
    btn.set_visible(false);
    btn.update_property(&[gtk::accessible::Property::Label("Follow the chapter")]);
    btn
}
