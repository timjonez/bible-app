use crate::workspace::Pane;
use adw::prelude::*;
use gtk::gio;
use relm4::{adw, gtk};

pub struct PaneHost {
    pub root: gtk::Box,
    pub bar: adw::TabBar,
    pub view: adw::TabView,
}

impl PaneHost {
    fn new() -> Self {
        let view = adw::TabView::new();
        view.set_hexpand(true);
        view.set_vexpand(true);
        let bar = adw::TabBar::new();
        bar.set_view(Some(&view));
        bar.set_autohide(true);
        bar.set_expand_tabs(true);
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.append(&bar);
        root.append(&view);
        Self { root, bar, view }
    }
}

pub struct SplitShell {
    pub paned: gtk::Paned,
    pub left: PaneHost,
    pub right: PaneHost,
}

impl SplitShell {
    pub fn new() -> Self {
        let left = PaneHost::new();
        let right = PaneHost::new();
        right.root.set_visible(false);
        let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
        paned.set_hexpand(true);
        paned.set_vexpand(true);
        paned.set_wide_handle(true);
        paned.add_css_class("pane-split");
        paned.set_resize_start_child(true);
        paned.set_resize_end_child(true);
        paned.set_shrink_start_child(false);
        paned.set_shrink_end_child(false);
        paned.set_start_child(Some(&left.root));
        paned.set_end_child(Some(&right.root));
        mark_split_handle(&paned);
        Self { paned, left, right }
    }

    pub fn host(&self, pane: Pane) -> &PaneHost {
        match pane {
            Pane::Left => &self.left,
            Pane::Right => &self.right,
        }
    }

    pub fn sync_split(&self) {
        let split = self.right.view.n_pages() > 0;
        if split && !self.right.root.is_visible() {
            let width = self.paned.width();
            if width > 0 {
                self.paned.set_position(width / 2);
            } else {
                self.paned.set_position(480);
            }
        }
        self.right.root.set_visible(split);
        self.left.bar.set_autohide(!split);
        self.right.bar.set_autohide(!split);
    }
}

pub struct DetachedHost {
    pub window: adw::ApplicationWindow,
    pub view: adw::TabView,
}

pub fn open_detached(title: &str) -> DetachedHost {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some(title));
    window.set_default_size(520, 720);

    let view = adw::TabView::new();
    view.set_hexpand(true);
    view.set_vexpand(true);
    let bar = adw::TabBar::new();
    bar.set_view(Some(&view));
    bar.set_autohide(true);

    let header = adw::HeaderBar::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_top_bar(&bar);
    toolbar.set_content(Some(&view));
    window.set_content(Some(&toolbar));
    window.present();
    DetachedHost { window, view }
}

/// The paned handle is the only resize control once a second view is open.
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
