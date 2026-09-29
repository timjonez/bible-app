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
}

impl SplitShell {
    pub fn new() -> Self {
        Self {
            host: PaneHost::new(),
        }
    }

    /// Place the tab bar on the left of the header, with the menu after it.
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
    menu.append(Some("Follow verse"), Some("win.tab-follow"));
    menu
}
