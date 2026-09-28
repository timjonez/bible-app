use crate::workspace::Pane;
use adw::prelude::*;
use gtk::gio;
use relm4::{adw, gtk};

pub struct PaneHost {
    pub root: gtk::Box,
    pub view: adw::TabView,
    pub split_btn: gtk::Button,
    pub popout_btn: gtk::Button,
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

        let split_btn = gtk::Button::from_icon_name("view-dual-symbolic");
        split_btn.set_tooltip_text(Some("Split this view"));
        split_btn.add_css_class("flat");
        split_btn.update_property(&[gtk::accessible::Property::Label("Split this view")]);
        let popout_btn = gtk::Button::from_icon_name("window-new-symbolic");
        popout_btn.set_tooltip_text(Some("Open in a new window"));
        popout_btn.add_css_class("flat");
        popout_btn.update_property(&[gtk::accessible::Property::Label("Open in a new window")]);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        actions.add_css_class("linked");
        actions.append(&split_btn);
        actions.append(&popout_btn);
        bar.set_end_action_widget(Some(&actions));

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.append(&bar);
        root.append(&view);
        Self {
            root,
            view,
            split_btn,
            popout_btn,
        }
    }

    pub fn set_split_sensitive(&self, on: bool) {
        self.split_btn.set_sensitive(on);
        self.split_btn.set_tooltip_text(Some(if on {
            "Split this view"
        } else {
            "Already beside another view"
        }));
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
    }
}

/// Header controls for a reader window other than the main one.
pub struct SideChrome {
    pub window: adw::ApplicationWindow,
    pub shell: SplitShell,
    pub book: gtk::DropDown,
    pub chapter: gtk::DropDown,
    pub search_btn: gtk::ToggleButton,
    pub prev: gtk::Button,
    pub next: gtk::Button,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub history: gtk::Box,
    pub goto_entry: gtk::Entry,
    pub goto_popover: gtk::Popover,
}

pub fn open_side_window(menu: &impl IsA<gio::MenuModel>) -> SideChrome {
    let app = relm4::main_adw_application();
    let window = adw::ApplicationWindow::new(&app);
    window.set_title(Some("bible-app"));
    window.set_default_size(960, 720);

    let book = gtk::DropDown::from_strings(&[]);
    book.set_enable_search(true);
    book.set_search_match_mode(gtk::StringFilterMatchMode::Substring);
    book.set_tooltip_text(Some("Book"));
    book.add_css_class("passage-picker");
    book.update_property(&[gtk::accessible::Property::Label("Book")]);

    let chapter = gtk::DropDown::from_strings(&[]);
    chapter.set_enable_search(true);
    chapter.set_search_match_mode(gtk::StringFilterMatchMode::Prefix);
    chapter.set_tooltip_text(Some("Chapter"));
    chapter.add_css_class("chapter-picker");
    chapter.update_property(&[gtk::accessible::Property::Label("Chapter")]);

    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    title_box.set_valign(gtk::Align::Center);
    title_box.set_halign(gtk::Align::Center);
    title_box.add_css_class("passage-title");
    title_box.append(&book);
    title_box.append(&chapter);

    let prev = gtk::Button::from_icon_name("go-previous-symbolic");
    prev.set_tooltip_text(Some("Previous chapter (Alt+Left)"));
    let next = gtk::Button::from_icon_name("go-next-symbolic");
    next.set_tooltip_text(Some("Next chapter (Alt+Right)"));
    let chapters = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chapters.add_css_class("linked");
    chapters.append(&prev);
    chapters.append(&next);

    let back = gtk::Button::from_icon_name("edit-undo-symbolic");
    back.set_tooltip_text(Some("Back in history (Alt+Shift+Left)"));
    let forward = gtk::Button::from_icon_name("edit-redo-symbolic");
    forward.set_tooltip_text(Some("Forward in history (Alt+Shift+Right)"));
    let history = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    history.add_css_class("linked");
    history.append(&back);
    history.append(&forward);
    history.set_visible(false);

    let start = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    start.append(&chapters);
    start.append(&history);

    let search_btn = gtk::ToggleButton::new();
    search_btn.set_icon_name("edit-find-symbolic");
    search_btn.set_tooltip_text(Some("Search (Ctrl+F)"));

    let menu_btn = gtk::MenuButton::new();
    menu_btn.set_icon_name("open-menu-symbolic");
    menu_btn.set_tooltip_text(Some("Menu"));
    menu_btn.set_primary(true);
    menu_btn.add_css_class("primary-menu");
    menu_btn.set_menu_model(Some(menu));

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title_box));
    header.pack_start(&start);
    header.pack_end(&menu_btn);
    header.pack_end(&search_btn);

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
    goto_popover.set_parent(&title_box);

    let shell = SplitShell::new();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&shell.paned));
    window.set_content(Some(&toolbar));
    window.present();

    SideChrome {
        window,
        shell,
        book,
        chapter,
        search_btn,
        prev,
        next,
        back,
        forward,
        history,
        goto_entry,
        goto_popover,
    }
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

pub fn selected_tab_id(view: &adw::TabView) -> Option<crate::workspace::TabId> {
    view.selected_page()
        .and_then(|page| page.keyword())
        .as_deref()
        .and_then(crate::workspace::TabId::from_keyword)
}
