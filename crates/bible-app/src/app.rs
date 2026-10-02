use crate::config;
use crate::dict;
use crate::launcher;
use crate::layout;
use crate::marks;
use crate::mhc;
use crate::nav::{self, Ref};
use crate::occurrences;
use crate::passage::{self, PassageView};
use crate::picker;
use crate::search;
use crate::shell::{self, SideChrome, SplitShell};
use crate::strongs;
use crate::theme;
use crate::tsk;
use crate::user_db;
use crate::workspace::{SplitOutcome, TabId, TabKind, WindowId, Workspace};
use adw::prelude::*;
use bible_app_db::{self, Book, DictModule, LibraryKind, MatchMode, SearchScope};
use gtk::gio;
use gtk::glib;
use relm4::actions::{RelmAction, RelmActionGroup};
use relm4::prelude::*;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

struct HostedTab {
    /// None while this view is the right-hand side of another tab.
    page: Option<adw::TabPage>,
    /// Child of `page`. None while this view is embedded.
    slot: Option<gtk::Box>,
    body: gtk::Widget,
    content: TabContent,
}

enum TabContent {
    Passage(Box<PassageView>),
    Mhc(mhc::MhcWidgets),
    Tsk(tsk::TskWidgets),
    Library(dict::DictWidgets),
    Bookmarks(marks::BookmarksWidgets),
    Notes(marks::NotesWidgets),
    Occurrences(occurrences::OccWidgets),
    Search(search::Pane),
    Blank(launcher::BlankPage),
}

struct SideWindow {
    id: WindowId,
    chrome: SideChrome,
}

pub struct App {
    conn: Option<Connection>,
    books: Vec<Book>,
    error: Option<String>,
    /// Saved match mode, used when a search tab is opened.
    search_mode: MatchMode,
    search_mark: Option<SearchMark>,
    search_db_path: Option<PathBuf>,
    dict_modules: Vec<DictModule>,
    font_size: i32,
    /// `0` uses the automatic measure. A positive value is the dragged width.
    column_width: i32,
    paragraphs: bool,
    font_provider: gtk::CssProvider,
    _theme_watch: theme::Watch,
    follow_action: gio::SimpleAction,
    user: Option<Connection>,
    workspace: Workspace,
    hosted: HashMap<TabId, HostedTab>,
    /// Pages closed only so their view can be drawn inside another tab.
    embedding: HashSet<TabId>,
    shell: Option<SplitShell>,
    sides: Rc<RefCell<Vec<SideWindow>>>,
    menu_tab: Rc<Cell<Option<TabId>>>,
    /// Verse clicks and search move a new view into the other pane.
    /// The app menu and a blank tab turn this off and stay in the current tab bar.
    open_beside: bool,
    goto_entry: gtk::Entry,
    goto_popover: gtk::Popover,
    actions: gio::SimpleActionGroup,
    msg_tx: relm4::Sender<Msg>,
    copy_offset: Rc<Cell<i32>>,
}

#[derive(Clone, Debug)]
struct SearchMark {
    at: Ref,
    tokens: Vec<String>,
    strongs: Option<String>,
}

#[derive(Debug)]
pub enum Msg {
    PrevChapter(WindowId),
    NextChapter(WindowId),
    GoTo(WindowId, String),
    GoToBeside(WindowId, String),
    Escape(WindowId),
    SetSearch(WindowId, bool),
    Search(TabId, String),
    SetSearchScope(TabId, SearchScope),
    SetSearchMode(TabId, MatchMode),
    SetSearchRange(TabId, search::SearchRange),
    SelectSearchBook(TabId, Option<u8>),
    SearchReady(TabId, u64, search::Outcome),
    SearchMore(TabId),
    PreviewHit(TabId, i32),
    SearchActivate(TabId),
    OpenHit(TabId, i32),
    OpenHitBeside(TabId, i32),
    OpenMhc,
    OpenTsk,
    OpenTskDest(Ref),
    OpenTskDestBeside(Ref),
    ClickCite {
        id: TabId,
        offset: i32,
    },
    CiteMenu {
        id: TabId,
        offset: i32,
        x: i32,
        y: i32,
    },
    OpenCiteTab {
        id: TabId,
        at: Ref,
    },
    OpenCiteWindow {
        id: TabId,
        at: Ref,
    },
    OpenStrongsCode(String),
    ClickWord {
        id: TabId,
        offset: i32,
    },
    OpenDict(String),
    OpenDictWord {
        module: String,
        headword: String,
    },
    DictSearch(TabId, String),
    DictOpen(TabId, i32),
    OpenStrongsOccurrences(String),
    OpenOccurrenceHit(TabId, i32),
    Back(WindowId),
    Forward(WindowId),
    SelectPassageBook(TabId, u32),
    SelectPassageChapter(TabId, u32),
    PassagePrev(TabId),
    PassageNext(TabId),
    PassageBack(TabId),
    PassageForward(TabId),
    StudyBook(TabId, u32),
    StudyChapter(TabId, u32),
    StudyPrev(TabId),
    StudyNext(TabId),
    StudyBack(TabId),
    StudyForward(TabId),
    LibraryPrev(TabId),
    LibraryNext(TabId),
    LibraryBack(TabId),
    LibraryForward(TabId),
    CopyVerses,
    CopyAtOffset(i32),
    FontSmaller,
    FontLarger,
    SetColumnWidth(i32),
    PersistColumnWidth,
    SetParagraphs(bool),
    OpenBookmarks,
    OpenNotes,
    MarksBookmarkActivated(TabId, i32),
    MarksBookmarkSelected(TabId),
    MarksNoteActivated(TabId, i32),
    MarksNoteSelected(TabId, i32),
    SaveNote(TabId),
    DeleteEditingNote(TabId),
    RemoveSelectedBookmark(TabId),
    ToggleBookmark,
    SetHighlight(String),
    AddNote,
    VerseContext {
        id: TabId,
        offset: i32,
        x: i32,
        y: i32,
    },
    ExportNotes,
    ExportWrite(PathBuf),
    TabSelected(TabId),
    TabClosed(TabId),
    /// Dismiss the view drawn inside a split tab.
    CloseGuest(TabId),
    TabAttached {
        id: TabId,
        window: WindowId,
    },
    SplitTab(TabId),
    WireSide(WindowId),
    SetupTabMenu(Option<TabId>),
    DetachMenuTab,
    BesideMenuTab,
    SetTabFollow(bool),
    ThemeChanged,
    NewTab(WindowId),
    BlankSelectBook(TabId, u32),
    BlankSelectChapter(TabId, u32),
    BlankLaunch {
        id: TabId,
        choice: launcher::Launch,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for App {
    type Init = ();
    type Input = Msg;
    type Output = ();

    view! {
        #[root]
        adw::ApplicationWindow {
            set_title: Some("bible-app"),
            set_default_size: (960, 720),

            #[wrap(Some)]
            set_content = &adw::ToolbarView {
                set_top_bar_style: adw::ToolbarStyle::Flat,

                #[name(header_bar)]
                add_top_bar = &adw::HeaderBar {
                    #[name(menu_btn)]
                    pack_end = &gtk::MenuButton {
                        set_icon_name: "open-menu-symbolic",
                        set_tooltip_text: Some("Menu"),
                        set_primary: true,
                        set_valign: gtk::Align::Center,
                        add_css_class: "primary-menu",
                        set_menu_model: Some(&build_app_menu(&model.dict_modules)),
                    },
                },

                #[wrap(Some)]
                set_content = if model.error.is_some() {
                    adw::StatusPage {
                        set_icon_name: Some("dialog-warning-symbolic"),
                        set_title: config::database_missing_title(),
                        #[watch]
                        set_description: model.error.as_deref(),
                    }
                } else {
                    #[local_ref]
                    workspace_host -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                    }
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let workspace_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        workspace_host.set_hexpand(true);
        workspace_host.set_vexpand(true);
        let font_provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &font_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let (conn, search_db_path, books, at, font_size, paragraphs, search_mode, error) =
            match load_library() {
                Ok((conn, path, books, at, font_size, paragraphs, mode)) => (
                    Some(conn),
                    Some(path),
                    books,
                    at,
                    font_size,
                    paragraphs,
                    mode,
                    None,
                ),
                Err(e) => (
                    None,
                    None,
                    Vec::new(),
                    Ref {
                        book: 1,
                        chapter: 1,
                        verse: 1,
                    },
                    layout::DEFAULT_FONT,
                    true,
                    MatchMode::Phrase,
                    Some(e),
                ),
            };
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
        let goto_sender = sender.clone();
        goto_entry.connect_activate(move |entry| {
            goto_sender.input(Msg::GoTo(WindowId::MAIN, entry.text().to_string()));
        });
        let goto_keys = gtk::EventControllerKey::new();
        goto_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let goto_entry_shift = goto_entry.clone();
        let goto_shift_sender = sender.clone();
        goto_keys.connect_key_pressed(move |_, keyval, _, mods| {
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shift && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                goto_shift_sender.input(Msg::GoToBeside(
                    WindowId::MAIN,
                    goto_entry_shift.text().to_string(),
                ));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        goto_entry.add_controller(goto_keys);

        let paragraphs_action: RelmAction<ParagraphsAction> = {
            let sender = sender.clone();
            RelmAction::new_stateful(&paragraphs, move |_, state: &mut bool| {
                *state = !*state;
                sender.input(Msg::SetParagraphs(*state));
            })
        };
        let copy_verse: RelmAction<CopyVerseAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::CopyVerses))
        };
        if error.is_some() {
            copy_verse.gio_action().set_enabled(false);
        }
        let font_larger: RelmAction<FontLargerAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::FontLarger))
        };
        let font_smaller: RelmAction<FontSmallerAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::FontSmaller))
        };
        let mhc_action: RelmAction<MhcAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::OpenMhc))
        };
        let tsk_action: RelmAction<TskAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::OpenTsk))
        };
        if error.is_some() {
            mhc_action.gio_action().set_enabled(false);
            tsk_action.gio_action().set_enabled(false);
        }

        let dict_modules = conn
            .as_ref()
            .map(|c| {
                let mut modules = bible_app_db::lexicon_modules(c).unwrap_or_default();
                modules.extend(bible_app_db::dictionary_modules(c).unwrap_or_default());
                modules
            })
            .unwrap_or_default();
        let dict_action = gio::SimpleAction::new("open-dict", Some(glib::VariantTy::STRING));
        dict_action.set_enabled(error.is_none() && !dict_modules.is_empty());
        let dict_sender = sender.clone();
        dict_action.connect_activate(move |_, param| {
            if let Some(id) = param.and_then(|p| p.get::<String>()) {
                dict_sender.input(Msg::OpenDict(id));
            }
        });

        let user = user_db::open_default().ok();
        let marks_on = error.is_none() && user.is_some();
        let bookmarks_action: RelmAction<BookmarksAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::OpenBookmarks))
        };
        let notes_action: RelmAction<NotesAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::OpenNotes))
        };
        let export_notes_action: RelmAction<ExportNotesAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::ExportNotes))
        };
        let toggle_bookmark_action: RelmAction<ToggleBookmarkAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::ToggleBookmark))
        };
        let add_note_action: RelmAction<AddNoteAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::AddNote))
        };
        bookmarks_action.gio_action().set_enabled(marks_on);
        notes_action.gio_action().set_enabled(marks_on);
        export_notes_action.gio_action().set_enabled(marks_on);
        toggle_bookmark_action.gio_action().set_enabled(marks_on);
        add_note_action.gio_action().set_enabled(marks_on);

        let highlight_action = gio::SimpleAction::new("highlight", Some(glib::VariantTy::STRING));
        highlight_action.set_enabled(marks_on);
        let highlight_sender = sender.clone();
        highlight_action.connect_activate(move |_, param| {
            let color = param.and_then(|p| p.get::<String>()).unwrap_or_default();
            highlight_sender.input(Msg::SetHighlight(color));
        });

        let copy_offset = Rc::new(Cell::new(0i32));
        let copy_here = gio::SimpleAction::new("copy-verse-here", None);
        copy_here.set_enabled(error.is_none());
        let copy_here_sender = sender.clone();
        let copy_here_offset = copy_offset.clone();
        copy_here.connect_activate(move |_, _| {
            copy_here_sender.input(Msg::CopyAtOffset(copy_here_offset.get()));
        });

        let detach_tab: RelmAction<DetachTabAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::DetachMenuTab))
        };
        let beside_tab: RelmAction<BesideTabAction> = {
            let sender = sender.clone();
            RelmAction::new_stateless(move |_| sender.input(Msg::BesideMenuTab))
        };
        let follow_tab: RelmAction<FollowTabAction> = {
            let sender = sender.clone();
            RelmAction::new_stateful(&true, move |_, state: &mut bool| {
                *state = !*state;
                sender.input(Msg::SetTabFollow(*state));
            })
        };
        let follow_gio = follow_tab.gio_action().clone();

        let mut group = RelmActionGroup::<WindowActionGroup>::new();
        group.add_action(paragraphs_action);
        group.add_action(copy_verse);
        group.add_action(font_larger);
        group.add_action(font_smaller);
        group.add_action(mhc_action);
        group.add_action(tsk_action);
        group.add_action(bookmarks_action);
        group.add_action(notes_action);
        group.add_action(export_notes_action);
        group.add_action(toggle_bookmark_action);
        group.add_action(add_note_action);
        group.add_action(detach_tab);
        group.add_action(beside_tab);
        group.add_action(follow_tab);

        let theme_tx = sender.input_sender().clone();
        let _theme_watch = theme::install(move || theme_tx.emit(Msg::ThemeChanged));

        let mut model = App {
            conn,
            books,
            error,
            search_mode,
            search_mark: None,
            search_db_path,
            dict_modules,
            font_size,
            column_width: loaded_column_width(),
            paragraphs,
            font_provider,
            _theme_watch,
            follow_action: follow_gio,
            user,
            workspace: Workspace::new(at),
            hosted: HashMap::new(),
            embedding: HashSet::new(),
            shell: None,
            sides: Rc::new(RefCell::new(Vec::new())),
            menu_tab: Rc::new(Cell::new(None)),
            open_beside: true,
            goto_entry: goto_entry.clone(),
            goto_popover: goto_popover.clone(),
            actions: gio::SimpleActionGroup::new(),
            msg_tx: sender.input_sender().clone(),
            copy_offset,
        };
        model.apply_font();
        picker::install_css();
        passage::install_css();

        let widgets = view_output!();
        let action_group = group.into_action_group();
        action_group.add_action(&dict_action);
        action_group.add_action(&highlight_action);
        action_group.add_action(&copy_here);
        root.insert_action_group("win", Some(&action_group));
        model.actions = action_group;
        model.goto_popover.set_parent(&widgets.header_bar);
        let goto_on_destroy = model.goto_popover.clone();
        root.connect_destroy(move |_| {
            goto_on_destroy.unparent();
        });

        if model.error.is_none() {
            let shell = SplitShell::new();
            shell.attach_header(&widgets.header_bar, &widgets.menu_btn);
            let ctx = model.wire_ctx();
            wire_host(&shell.host, WindowId::MAIN, &ctx);
            workspace_host.append(&shell.host.root);
            model.shell = Some(shell);
            let id = model.workspace.focused();
            model.spawn_passage(id, at);
            model.sync_pickers();
        }

        let key = gtk::EventControllerKey::new();
        let goto_entry_keys = model.goto_entry.clone();
        let goto_popover_keys = model.goto_popover.clone();
        let sender_keys = sender.clone();
        key.connect_key_pressed(move |_, keyval, _, mods| {
            let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let alt = mods.contains(gtk::gdk::ModifierType::ALT_MASK);
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if ctrl && (keyval == gtk::gdk::Key::f || keyval == gtk::gdk::Key::F) {
                sender_keys.input(Msg::SetSearch(WindowId::MAIN, true));
                return glib::Propagation::Stop;
            }
            if ctrl && !shift && (keyval == gtk::gdk::Key::t || keyval == gtk::gdk::Key::T) {
                sender_keys.input(Msg::NewTab(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Escape {
                if goto_popover_keys.is_visible() {
                    goto_popover_keys.popdown();
                    return glib::Propagation::Stop;
                }
                sender_keys.input(Msg::Escape(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::l || keyval == gtk::gdk::Key::L) {
                goto_popover_keys.popup();
                goto_entry_keys.grab_focus();
                goto_entry_keys.select_region(0, -1);
                return glib::Propagation::Stop;
            }
            if alt && shift && keyval == gtk::gdk::Key::Left {
                sender_keys.input(Msg::Back(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if alt && shift && keyval == gtk::gdk::Key::Right {
                sender_keys.input(Msg::Forward(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if alt && keyval == gtk::gdk::Key::Left {
                sender_keys.input(Msg::PrevChapter(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if alt && keyval == gtk::gdk::Key::Right {
                sender_keys.input(Msg::NextChapter(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Back {
                sender_keys.input(Msg::Back(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Forward {
                sender_keys.input(Msg::Forward(WindowId::MAIN));
                return glib::Propagation::Stop;
            }
            if ctrl
                && (keyval == gtk::gdk::Key::plus
                    || keyval == gtk::gdk::Key::equal
                    || keyval == gtk::gdk::Key::KP_Add)
            {
                sender_keys.input(Msg::FontLarger);
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::minus || keyval == gtk::gdk::Key::KP_Subtract) {
                sender_keys.input(Msg::FontSmaller);
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::d || keyval == gtk::gdk::Key::D) {
                sender_keys.input(Msg::ToggleBookmark);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        root.add_controller(key);

        let mouse_back = gtk::GestureClick::new();
        mouse_back.set_button(8);
        let back_sender = sender.clone();
        mouse_back.connect_pressed(move |_, _, _, _| {
            back_sender.input(Msg::Back(WindowId::MAIN));
        });
        root.add_controller(mouse_back);
        let mouse_forward = gtk::GestureClick::new();
        mouse_forward.set_button(9);
        let forward_sender = sender.clone();
        mouse_forward.connect_pressed(move |_, _, _, _| {
            forward_sender.input(Msg::Forward(WindowId::MAIN));
        });
        root.add_controller(mouse_forward);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::PrevChapter(window) => self.alt_step(window, AltStep::Prev),
            Msg::NextChapter(window) => self.alt_step(window, AltStep::Next),
            Msg::GoTo(window, text) => {
                let text = text.trim();
                if text.is_empty() {
                    self.popdown_goto(window);
                    return;
                }
                if let Some(at) = nav::parse_ref(text, &self.books, self.at_in(window)) {
                    self.go_in(window, at, true);
                    self.popdown_goto(window);
                }
            }
            Msg::GoToBeside(window, text) => {
                let text = text.trim();
                if text.is_empty() {
                    self.popdown_goto(window);
                    return;
                }
                if let Some(at) = nav::parse_ref(text, &self.books, self.at_in(window)) {
                    self.go_beside_in(window, at);
                    self.popdown_goto(window);
                }
            }
            Msg::Escape(window) => self.escape(window),
            Msg::SetSearch(window, open) => self.set_search(window, open),
            Msg::Search(id, query) => {
                if let Some(pane) = self.search_mut(id) {
                    pane.query = query;
                    pane.chip = search::BookChip::Auto;
                }
                self.schedule_search(id, false);
            }
            Msg::SetSearchScope(id, scope) => {
                let changed = self.search(id).is_some_and(|pane| pane.scope != scope);
                if changed {
                    if let Some(pane) = self.search_mut(id) {
                        pane.scope = scope;
                        pane.chip = search::BookChip::Auto;
                        pane.sync_placeholder();
                        pane.range_dd
                            .set_visible(search::SearchRange::applies(scope));
                    }
                    self.schedule_search(id, false);
                }
            }
            Msg::SetSearchMode(id, mode) => {
                let changed = self.search(id).is_some_and(|pane| pane.mode != mode);
                if changed {
                    if let Some(pane) = self.search_mut(id) {
                        pane.mode = mode;
                        pane.chip = search::BookChip::Auto;
                    }
                    self.search_mode = mode;
                    self.save_state();
                    self.schedule_search(id, false);
                }
            }
            Msg::SetSearchRange(id, range) => {
                let changed = self.search(id).is_some_and(|pane| pane.range != range);
                if changed {
                    if let Some(pane) = self.search_mut(id) {
                        pane.range = range;
                        pane.chip = search::BookChip::Auto;
                    }
                    self.schedule_search(id, false);
                }
            }
            Msg::SelectSearchBook(id, book) => {
                if let Some(pane) = self.search_mut(id) {
                    pane.chip = match book {
                        Some(book) => search::BookChip::Book(book),
                        None => search::BookChip::AllBooks,
                    };
                }
                self.schedule_search(id, false);
            }
            Msg::SearchReady(id, gen, outcome) => {
                if self.search(id).is_some_and(|pane| pane.gen.get() == gen) {
                    self.apply_outcome(id, outcome);
                }
            }
            Msg::SearchMore(id) => {
                let Some(pane) = self.search(id) else { return };
                let shown = pane
                    .hits
                    .iter()
                    .filter(|hit| search::hit_ref(hit).is_some())
                    .count() as i64;
                if pane.query.trim().is_empty() || shown >= pane.total {
                    return;
                }
                self.schedule_search(id, true);
            }
            Msg::PreviewHit(id, idx) => self.preview_hit(id, idx),
            Msg::SearchActivate(id) => {
                let idx = self
                    .search(id)
                    .and_then(|pane| pane.list.selected_row())
                    .map(|row| row.index())
                    .unwrap_or(0);
                self.open_hit(id, idx, false, true);
            }
            Msg::OpenHit(id, idx) => self.open_hit(id, idx, false, false),
            Msg::OpenHitBeside(id, idx) => {
                let idx = if idx < 0 {
                    self.search(id)
                        .and_then(|pane| pane.list.selected_row())
                        .map(|row| row.index())
                        .unwrap_or(0)
                } else {
                    idx
                };
                self.open_hit(id, idx, true, true);
            }
            Msg::OpenMhc => {
                self.in_current_tabs(|app| app.ensure_mhc());
            }
            Msg::OpenTsk => {
                self.in_current_tabs(|app| app.ensure_tsk());
            }
            Msg::OpenTskDest(at) => {
                self.popdown_passage_popovers();
                self.go(at, true);
            }
            Msg::OpenTskDestBeside(at) => {
                self.popdown_passage_popovers();
                self.go_beside(at);
            }
            Msg::ClickCite { id, offset } => {
                let Some(at) = self.cite_at(id, offset) else {
                    return;
                };
                self.open_cite_beside(id, at);
            }
            Msg::CiteMenu { id, offset, x, y } => {
                let Some(at) = self.cite_at(id, offset) else {
                    return;
                };
                if let Some(widgets) = self.tsk_widgets_for(id) {
                    tsk::popup_cite_menu(widgets, x, y, at, id, self.msg_tx.clone());
                }
            }
            Msg::OpenCiteTab { id, at } => self.open_cite_tab(id, at),
            Msg::OpenCiteWindow { id, at } => self.open_cite_window(id, at),
            Msg::OpenStrongsCode(code) => {
                self.open_strongs_code(&code);
            }
            Msg::ClickWord { id, offset } => {
                self.focus_tab(id);
                self.handle_click(id, offset);
            }
            Msg::OpenDict(id) => {
                self.in_current_tabs(|app| app.open_library(&id, None));
            }
            Msg::OpenDictWord { module, headword } => {
                self.popdown_passage_popovers();
                let _ = self.open_library(&module, Some(&headword));
            }
            Msg::DictSearch(id, query) => {
                if let Some(TabContent::Library(widgets)) =
                    self.hosted.get_mut(&id).map(|h| &mut h.content)
                {
                    widgets.query = query;
                    if let Some(conn) = &self.conn {
                        dict::search(widgets, conn);
                    }
                }
            }
            Msg::DictOpen(id, idx) => self.open_dict_hit(id, idx),
            Msg::OpenStrongsOccurrences(code) => {
                self.popdown_passage_popovers();
                let _ = self.open_occurrences(&code);
            }
            Msg::OpenOccurrenceHit(id, idx) => {
                let Some(at) = self
                    .occ_widgets(id)
                    .and_then(|w| occurrences::hit_at(w, idx))
                else {
                    return;
                };
                self.go(at, true);
            }
            Msg::Back(window) => self.alt_step(window, AltStep::Back),
            Msg::Forward(window) => self.alt_step(window, AltStep::Forward),
            Msg::SelectPassageBook(id, idx) => {
                let Some(book) = picker::book_id_at(&self.books, idx) else {
                    return;
                };
                let Some(at) = self.passage(id).map(|p| p.at) else {
                    return;
                };
                if at.book == book {
                    return;
                }
                self.apply_passage_ref(
                    id,
                    Ref {
                        book,
                        chapter: 1,
                        verse: 1,
                    },
                    false,
                    true,
                );
            }
            Msg::SelectPassageChapter(id, idx) => {
                let Some(chapter) = picker::chapter_from_index(idx) else {
                    return;
                };
                let Some(at) = self.passage(id).map(|p| p.at) else {
                    return;
                };
                if at.chapter == chapter {
                    return;
                }
                self.apply_passage_ref(
                    id,
                    Ref {
                        book: at.book,
                        chapter,
                        verse: 1,
                    },
                    false,
                    true,
                );
            }
            Msg::PassagePrev(id) => {
                let Some(at) = self.passage(id).map(|p| p.at) else {
                    return;
                };
                let Some(conn) = &self.conn else { return };
                let Ok(at) = nav::prev_chapter(conn, &self.books, at) else {
                    return;
                };
                self.apply_passage_ref(id, at, false, true);
            }
            Msg::PassageNext(id) => {
                let Some(at) = self.passage(id).map(|p| p.at) else {
                    return;
                };
                let Some(conn) = &self.conn else { return };
                let Ok(at) = nav::next_chapter(conn, &self.books, at) else {
                    return;
                };
                self.apply_passage_ref(id, at, false, true);
            }
            Msg::PassageBack(id) => {
                let Some(at) = self.passage_mut(id).and_then(|p| p.history.back()) else {
                    return;
                };
                self.workspace.focus(id);
                self.apply_passage_ref(id, at, true, false);
            }
            Msg::PassageForward(id) => {
                let Some(at) = self.passage_mut(id).and_then(|p| p.history.forward()) else {
                    return;
                };
                self.workspace.focus(id);
                self.apply_passage_ref(id, at, true, false);
            }
            Msg::StudyBook(id, idx) => self.study_book(id, idx),
            Msg::StudyChapter(id, idx) => self.study_chapter(id, idx),
            Msg::StudyPrev(id) => self.study_prev(id),
            Msg::StudyNext(id) => self.study_next(id),
            Msg::StudyBack(id) => self.study_history(id, false),
            Msg::StudyForward(id) => self.study_history(id, true),
            Msg::LibraryPrev(id) => self.library_step(id, false),
            Msg::LibraryNext(id) => self.library_step(id, true),
            Msg::LibraryBack(id) => self.library_history(id, false),
            Msg::LibraryForward(id) => self.library_history(id, true),
            Msg::CopyVerses => {
                self.copy_from_selection_or_current();
            }
            Msg::CopyAtOffset(offset) => {
                let Some(p) = self.focused_passage() else {
                    return;
                };
                let verse =
                    layout::verse_at_offset(&p.layout.verse_start, offset).unwrap_or(p.at.verse);
                self.copy_verse_range(p.at, verse, verse);
            }
            Msg::FontSmaller => {
                self.font_size = layout::smaller_font(self.font_size);
                self.apply_font();
                self.save_state();
            }
            Msg::FontLarger => {
                self.font_size = layout::larger_font(self.font_size);
                self.apply_font();
                self.save_state();
            }
            Msg::SetColumnWidth(px) => {
                let px = layout::clamp_column_px(px);
                if self.column_width == px {
                    return;
                }
                self.column_width = px;
                self.apply_column();
            }
            Msg::PersistColumnWidth => self.save_state(),
            Msg::SetParagraphs(on) => {
                if self.paragraphs == on {
                    return;
                }
                self.paragraphs = on;
                self.save_state();
                self.reload_all_passages(false);
            }
            Msg::OpenBookmarks => {
                self.in_current_tabs(|app| app.ensure_bookmarks());
            }
            Msg::OpenNotes => {
                self.in_current_tabs(|app| app.ensure_notes());
            }
            Msg::MarksBookmarkActivated(id, idx) => {
                if let Some(at) = self
                    .bookmarks_widgets(id)
                    .and_then(|w| marks::bookmark_at(w, idx))
                {
                    self.go(at, true);
                }
            }
            Msg::MarksBookmarkSelected(id) => {
                if let Some(w) = self.bookmarks_widgets(id) {
                    w.remove.set_sensitive(w.list.selected_row().is_some());
                }
            }
            Msg::MarksNoteActivated(id, idx) => {
                if let Some(at) = self.notes_widgets(id).and_then(|w| marks::note_at(w, idx)) {
                    self.go(at, true);
                }
            }
            Msg::MarksNoteSelected(id, idx) => {
                if self.notes_widgets(id).is_some_and(|w| w.syncing.get()) {
                    return;
                }
                let Some(at) = self.notes_widgets(id).and_then(|w| marks::note_at(w, idx)) else {
                    return;
                };
                let text = self
                    .user
                    .as_ref()
                    .and_then(|u| user_db::get_note(u, at).ok().flatten())
                    .unwrap_or_default();
                if let Some(TabContent::Notes(w)) = self.hosted.get_mut(&id).map(|h| &mut h.content)
                {
                    marks::load_note(w, &self.books, at, &text);
                }
            }
            Msg::SaveNote(id) => {
                let Some(TabContent::Notes(widgets)) = self.hosted.get(&id).map(|h| &h.content)
                else {
                    return;
                };
                if widgets.syncing.get() {
                    return;
                }
                let Some(at) = widgets.editing else { return };
                let text = marks::editor_text(widgets);
                let was_present = widgets.notes.iter().any(|n| n.at() == at);
                let now_present = !text.trim().is_empty();
                if let Some(user) = &self.user {
                    let _ = user_db::upsert_note(user, at, &text);
                }
                self.reload_user_marks(was_present != now_present);
                if let Some(TabContent::Notes(w)) = self.hosted.get(&id).map(|h| &h.content) {
                    w.delete.set_sensitive(now_present);
                }
            }
            Msg::DeleteEditingNote(id) => {
                let Some(at) = self.notes_widgets(id).and_then(|w| w.editing) else {
                    return;
                };
                if let Some(user) = &self.user {
                    let _ = user_db::delete_note(user, at);
                }
                if let Some(TabContent::Notes(w)) = self.hosted.get_mut(&id).map(|h| &mut h.content)
                {
                    marks::clear_editor(w);
                }
                self.reload_user_marks(true);
            }
            Msg::RemoveSelectedBookmark(id) => {
                let Some(at) = self
                    .bookmarks_widgets(id)
                    .and_then(marks::selected_bookmark)
                else {
                    return;
                };
                if let Some(user) = &self.user {
                    let _ = user_db::delete_bookmark(user, at);
                }
                self.reload_user_marks(true);
            }
            Msg::ToggleBookmark => self.toggle_bookmark(),
            Msg::SetHighlight(color) => self.set_highlight(&color),
            Msg::AddNote => self.add_note(),
            Msg::VerseContext { id, offset, x, y } => {
                self.copy_offset.set(offset);
                self.show_verse_menu(id, offset, x, y);
            }
            Msg::ExportNotes => self.export_notes(&sender),
            Msg::ExportWrite(path) => self.write_export(&path),
            Msg::TabSelected(id) => {
                self.workspace.focus(id);
                self.sync_pickers();
                if self.workspace.tab(id).is_some_and(|t| t.kind.is_blank()) {
                    self.focus_blank_entry(id);
                }
            }
            Msg::TabClosed(id) => {
                if !self.embedding.remove(&id) {
                    self.forget_tab(id);
                }
            }
            Msg::CloseGuest(id) => self.forget_tab(id),
            Msg::TabAttached { id, window } => self.tab_attached(id, window),
            Msg::SplitTab(id) => self.split_tab(id),
            Msg::WireSide(id) => self.wire_side(id),
            Msg::SetupTabMenu(id) => {
                self.menu_tab.set(id);
                let tab = id.and_then(|id| self.workspace.tab(id));
                let followable = tab
                    .is_some_and(|t| matches!(t.kind, TabKind::Mhc { .. } | TabKind::Tsk { .. }));
                self.follow_action.set_enabled(followable);
                self.follow_action
                    .set_state(&tab.is_some_and(|t| t.kind.follows_verse()).to_variant());
            }
            Msg::DetachMenuTab => {
                if let Some(id) = self.menu_tab.get() {
                    self.detach_tab(id);
                }
            }
            Msg::BesideMenuTab => {
                if let Some(id) = self.menu_tab.get() {
                    self.move_tab_beside(id);
                }
            }
            Msg::SetTabFollow(on) => {
                if let Some(id) = self.menu_tab.get() {
                    self.workspace.set_follow(id, on);
                    if on {
                        let at = self
                            .workspace
                            .tab(id)
                            .and_then(|tab| tab.kind.at())
                            .unwrap_or_else(|| self.at());
                        self.present_study(id, at, PlaceMemory::Navigate);
                    } else {
                        self.sync_study_bar(id);
                    }
                }
            }
            Msg::ThemeChanged => self.recolor_passages(),
            Msg::NewTab(window) => self.new_tab(window),
            Msg::BlankSelectBook(id, idx) => self.blank_select_book(id, idx),
            Msg::BlankSelectChapter(id, idx) => self.blank_select_chapter(id, idx),
            Msg::BlankLaunch { id, choice } => self.launch_from_blank(id, choice),
        }
        self.apply_column_mode();
        self.sync_shells();
        let _ = sender;
    }
}

impl App {
    fn at(&self) -> Ref {
        self.workspace
            .focused_passage_ref()
            .unwrap_or_else(|| self.workspace.last_at())
    }

    fn at_in(&self, window: WindowId) -> Ref {
        self.workspace
            .focused_passage_ref_in(window)
            .unwrap_or_else(|| self.at())
    }

    fn passage(&self, id: TabId) -> Option<&PassageView> {
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Passage(p)) => Some(p),
            _ => None,
        }
    }

    fn passage_mut(&mut self, id: TabId) -> Option<&mut PassageView> {
        match self.hosted.get_mut(&id).map(|h| &mut h.content) {
            Some(TabContent::Passage(p)) => Some(p),
            _ => None,
        }
    }

    fn focused_passage(&self) -> Option<&PassageView> {
        self.workspace
            .focused_passage_id()
            .and_then(|id| self.passage(id))
    }

    fn bookmarks_widgets(&self, id: TabId) -> Option<&marks::BookmarksWidgets> {
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Bookmarks(w)) => Some(w),
            _ => None,
        }
    }

    fn notes_widgets(&self, id: TabId) -> Option<&marks::NotesWidgets> {
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Notes(w)) => Some(w),
            _ => None,
        }
    }

    fn tsk_widgets_for(&self, id: TabId) -> Option<&tsk::TskWidgets> {
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Tsk(w)) => Some(w),
            _ => None,
        }
    }

    fn cite_at(&self, id: TabId, offset: i32) -> Option<Ref> {
        self.tsk_widgets_for(id)
            .and_then(|widgets| tsk::cite_at(widgets, offset))
    }

    fn occ_widgets(&self, id: TabId) -> Option<&occurrences::OccWidgets> {
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Occurrences(w)) => Some(w),
            _ => None,
        }
    }

    fn search(&self, id: TabId) -> Option<&search::Pane> {
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Search(pane)) => Some(pane),
            _ => None,
        }
    }

    fn search_mut(&mut self, id: TabId) -> Option<&mut search::Pane> {
        match self.hosted.get_mut(&id).map(|h| &mut h.content) {
            Some(TabContent::Search(pane)) => Some(pane),
            _ => None,
        }
    }

    fn window_of(&self, id: TabId) -> WindowId {
        self.workspace
            .tab(id)
            .map(|tab| tab.window)
            .unwrap_or(WindowId::MAIN)
    }

    fn escape(&mut self, window: WindowId) {
        if self.goto_open(window) {
            self.popdown_goto(window);
            return;
        }
        let Some(id) = self.workspace.focused_in(window) else {
            return;
        };
        if self.workspace.tab(id).is_some_and(|t| t.kind.is_search()) {
            self.request_close(id);
        }
    }

    fn goto_open(&self, window: WindowId) -> bool {
        if window == WindowId::MAIN {
            return self.goto_popover.is_visible();
        }
        self.sides
            .borrow()
            .iter()
            .any(|side| side.id == window && side.chrome.goto_popover.is_visible())
    }

    fn popdown_goto(&self, window: WindowId) {
        if window == WindowId::MAIN {
            self.goto_popover.popdown();
            return;
        }
        if let Some(side) = self.sides.borrow().iter().find(|side| side.id == window) {
            side.chrome.goto_popover.popdown();
        }
    }

    fn set_search(&mut self, window: WindowId, open: bool) {
        if self.error.is_some() {
            return;
        }
        if open {
            let _ = self.open_search_tab(window);
            return;
        }
        let id = self
            .workspace
            .focused_in(window)
            .filter(|&id| self.workspace.tab(id).is_some_and(|t| t.kind.is_search()))
            .or_else(|| self.workspace.find_search(window));
        if let Some(id) = id {
            self.request_close(id);
        }
    }

    fn open_search_tab(&mut self, window: WindowId) -> Option<TabId> {
        if self.error.is_some() {
            return None;
        }
        let was_split = self.workspace.is_window_split(window);
        let at = self.at_in(window);
        let opened = self.workspace.open_search(window);
        let mut pane = search::build_pane(self.search_mode);
        pane.origin = Some(at);
        self.wire_search(opened.id, &pane);
        let root = pane.root.clone();
        self.add_page(opened.id, &root, "Search", TabContent::Search(pane));
        if self.open_beside && !was_split {
            self.move_tab_beside(opened.id);
        }
        self.focus_search(opened.id);
        Some(opened.id)
    }

    fn wire_search(&self, id: TabId, pane: &search::Pane) {
        let window = self.window_of(id);
        let tx = self.msg_tx.clone();
        pane.entry.connect_search_changed(move |entry| {
            tx.emit(Msg::Search(id, entry.text().to_string()));
        });
        let tx = self.msg_tx.clone();
        pane.entry.connect_activate(move |_| {
            tx.emit(Msg::SearchActivate(id));
        });
        let tx = self.msg_tx.clone();
        pane.entry.connect_stop_search(move |_| {
            tx.emit(Msg::SetSearch(window, false));
        });
        let tx = self.msg_tx.clone();
        pane.list.connect_row_activated(move |_, row| {
            tx.emit(Msg::OpenHit(id, row.index()));
        });
        let tx = self.msg_tx.clone();
        let hold = pane.hold.clone();
        pane.list.connect_row_selected(move |_, row| {
            if hold.get() {
                return;
            }
            let Some(row) = row else { return };
            tx.emit(Msg::PreviewHit(id, row.index()));
        });
        let groups = pane.groups.clone();
        pane.list.set_header_func(move |row, before| {
            let groups = groups.borrow();
            let idx = row.index();
            if idx < 0 {
                row.set_header(None::<&gtk::Widget>);
                return;
            }
            let Some(group) = groups.get(idx as usize) else {
                row.set_header(None::<&gtk::Widget>);
                return;
            };
            let prev = before.and_then(|row| {
                let prev_idx = row.index();
                (prev_idx >= 0)
                    .then(|| groups.get(prev_idx as usize))
                    .flatten()
            });
            if prev.is_some_and(|prev| prev == group) {
                row.set_header(None::<&gtk::Widget>);
                return;
            }
            let label = gtk::Label::new(Some(group));
            label.set_xalign(0.0);
            label.add_css_class("heading");
            label.set_margin_top(10);
            label.set_margin_bottom(2);
            label.set_margin_start(12);
            row.set_header(Some(&label));
        });
        if let Some(scroll) = pane.list.parent().and_downcast::<gtk::ScrolledWindow>() {
            let tx = self.msg_tx.clone();
            scroll.connect_edge_reached(move |_, pos| {
                if pos == gtk::PositionType::Bottom {
                    tx.emit(Msg::SearchMore(id));
                }
            });
        }
        let tx = self.msg_tx.clone();
        pane.scope_dd.connect_selected_notify(move |dd| {
            let pos = dd.selected();
            if pos != gtk::INVALID_LIST_POSITION {
                tx.emit(Msg::SetSearchScope(id, SearchScope::from_index(pos)));
            }
        });
        let tx = self.msg_tx.clone();
        pane.mode_dd.connect_selected_notify(move |dd| {
            let pos = dd.selected();
            if pos != gtk::INVALID_LIST_POSITION {
                tx.emit(Msg::SetSearchMode(id, MatchMode::from_index(pos)));
            }
        });
        let tx = self.msg_tx.clone();
        pane.range_dd.connect_selected_notify(move |dd| {
            let pos = dd.selected();
            if pos != gtk::INVALID_LIST_POSITION {
                tx.emit(Msg::SetSearchRange(
                    id,
                    search::SearchRange::from_index(pos),
                ));
            }
        });
        let down = gtk::EventControllerKey::new();
        let list = pane.list.clone();
        down.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Down || keyval == gtk::gdk::Key::KP_Down {
                if let Some(row) = list.row_at_index(0) {
                    list.select_row(Some(&row));
                    row.grab_focus();
                }
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        pane.entry.add_controller(down);
        let shift = gtk::EventControllerKey::new();
        shift.set_propagation_phase(gtk::PropagationPhase::Capture);
        let tx = self.msg_tx.clone();
        let list = pane.list.clone();
        shift.connect_key_pressed(move |_, keyval, _, mods| {
            let shifted = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shifted && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                let idx = list.selected_row().map(|row| row.index()).unwrap_or(0);
                tx.emit(Msg::OpenHitBeside(id, idx));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        pane.entry.add_controller(shift);
        let list_shift = gtk::EventControllerKey::new();
        list_shift.set_propagation_phase(gtk::PropagationPhase::Capture);
        let tx = self.msg_tx.clone();
        list_shift.connect_key_pressed(move |_, keyval, _, mods| {
            let shifted = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shifted && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                tx.emit(Msg::OpenHitBeside(id, -1));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        pane.list.add_controller(list_shift);
        let esc = gtk::EventControllerKey::new();
        let tx = self.msg_tx.clone();
        esc.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                tx.emit(Msg::SetSearch(window, false));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        pane.list.add_controller(esc);
    }

    fn focus_search(&self, id: TabId) {
        let Some(entry) = self.search(id).map(|pane| pane.entry.clone()) else {
            return;
        };
        glib::timeout_add_local_once(Duration::from_millis(100), move || {
            entry.grab_focus();
        });
    }

    fn schedule_search(&mut self, id: TabId, append: bool) {
        let fallback = self.at();
        let Some((next, query, origin, mode, range, chip, scope, gen_cell)) =
            self.search_mut(id).map(|pane| {
                let next = pane.gen.get().saturating_add(1);
                pane.gen.set(next);
                (
                    next,
                    pane.query.clone(),
                    pane.origin.unwrap_or(fallback),
                    pane.mode,
                    pane.range,
                    pane.chip,
                    pane.scope,
                    pane.gen.clone(),
                )
            })
        else {
            return;
        };
        let plan = search::plan(&query, &self.books, origin, mode, range, chip, scope);
        match plan {
            search::Plan::Idle => {
                self.clear_search_results(id);
                if let Some(pane) = self.search(id) {
                    pane.show_status(&search::status("", 0, 0, scope));
                    pane.sync_empty();
                }
            }
            search::Plan::Short => {
                self.clear_search_results(id);
                if let Some(pane) = self.search(id) {
                    pane.show_status(search::short_status());
                    pane.sync_empty();
                }
            }
            search::Plan::Goto(at) => {
                let hit = search::goto_hit(at, &self.books);
                let label = format!("Go to {}", nav::format_ref(&self.books, at));
                if let Some(pane) = self.search_mut(id) {
                    pane.hits = vec![hit];
                    pane.total = 1;
                    pane.tokens.clear();
                    pane.strongs = None;
                    pane.counts.clear();
                    pane.chosen = None;
                    pane.show_status(&label);
                    pane.sync_empty();
                }
                self.refill_hits(id, false);
                self.refill_chips(id);
            }
            search::Plan::Ready(mut prepared) => {
                prepared.db_path = self.search_db_path.clone().unwrap_or_default();
                prepared.user_path = user_db::user_db_path();
                let after = if append {
                    self.search(id).and_then(|pane| {
                        pane.hits.iter().rev().find_map(|hit| {
                            let at = search::hit_ref(hit)?;
                            Some((at.book, at.chapter, at.verse))
                        })
                    })
                } else {
                    None
                };
                let gen = next;
                let tx = self.msg_tx.clone();
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    if gen_cell.get() != gen {
                        return;
                    }
                    std::thread::spawn(move || {
                        let outcome = search::execute(&prepared, after, append);
                        glib::MainContext::default().invoke(move || {
                            tx.emit(Msg::SearchReady(id, gen, outcome));
                        });
                    });
                });
            }
        }
    }

    fn clear_search_results(&mut self, id: TabId) {
        if let Some(pane) = self.search_mut(id) {
            pane.hits.clear();
            pane.total = 0;
            pane.tokens.clear();
            pane.strongs = None;
            pane.counts.clear();
            pane.chosen = None;
        }
        self.refill_hits(id, false);
        self.refill_chips(id);
    }

    fn apply_outcome(&mut self, id: TabId, outcome: search::Outcome) {
        let append = outcome.append;
        let Some(pane) = self.search_mut(id) else {
            return;
        };
        if append {
            pane.hits.extend(outcome.hits);
        } else {
            pane.hits = outcome.hits;
            pane.counts = outcome.by_book;
            pane.chosen = outcome.chosen_book;
        }
        pane.total = outcome.total;
        pane.tokens = outcome.tokens;
        pane.strongs = outcome.strongs;
        let scope = pane.scope;
        let query = pane.query.clone();
        self.maybe_lexicon_row(id);
        let Some(pane) = self.search_mut(id) else {
            return;
        };
        let shown = pane
            .hits
            .iter()
            .filter(|hit| search::hit_ref(hit).is_some())
            .count();
        let total = pane.total;
        pane.show_status(&search::status(&query, shown, total, scope));
        pane.sync_empty();
        let selected = if !append {
            pane.list.selected_row().map(|row| row.index())
        } else {
            None
        };
        self.refill_hits(id, append);
        if !append {
            self.refill_chips(id);
            if let Some(idx) = selected {
                self.preview_hit(id, idx);
            }
        }
    }

    fn maybe_lexicon_row(&mut self, id: TabId) {
        let Some(pane) = self.search(id) else {
            return;
        };
        let Some(code) = pane.strongs.clone() else {
            return;
        };
        if pane.hits.iter().any(|hit| hit.kind == LibraryKind::Lexicon) {
            return;
        }
        let shown = pane
            .hits
            .iter()
            .filter(|hit| search::hit_ref(hit).is_some())
            .count() as i64;
        if shown < pane.total {
            return;
        }
        let module = if code.starts_with('G') {
            "Thayer"
        } else {
            "BDB"
        };
        if self
            .dict_modules
            .iter()
            .any(|module_row| module_row.id == module)
        {
            if let Some(pane) = self.search_mut(id) {
                pane.hits.push(search::lexicon_hit(&code, module));
            }
        }
    }

    fn refill_hits(&mut self, id: TabId, append: bool) {
        let Some(pane) = self.search_mut(id) else {
            return;
        };
        let hits = if append {
            let previous = pane.groups.borrow().len();
            pane.hits[previous.min(pane.hits.len())..].to_vec()
        } else {
            pane.hits.clone()
        };
        let list = pane.list.clone();
        let groups = pane.groups.clone();
        let hold = pane.hold.clone();
        let scope = pane.scope;
        let tokens = pane.tokens.clone();
        hold.set(true);
        search::refill_list(&list, &hits, &self.books, scope, &tokens, &groups, append);
        hold.set(false);
    }

    fn refill_chips(&self, id: TabId) {
        let Some(pane) = self.search(id) else {
            return;
        };
        while let Some(child) = pane.chips.first_child() {
            pane.chips.remove(&child);
        }
        if !search::show_chips(pane.scope, pane.strongs.is_some()) || pane.counts.len() < 2 {
            return;
        }
        let sender = self.msg_tx.clone();
        if pane.chosen.is_some() {
            let all = gtk::ToggleButton::with_label("All");
            all.add_css_class("flat");
            all.set_active(false);
            let tx = sender.clone();
            all.connect_clicked(move |btn| {
                if btn.is_active() {
                    tx.emit(Msg::SelectSearchBook(id, None));
                }
            });
            pane.chips.append(&all);
        }
        let mut leader: Option<gtk::ToggleButton> = None;
        for count in &pane.counts {
            let button = gtk::ToggleButton::with_label(&search::chip_text(&self.books, count));
            button.add_css_class("flat");
            button.set_active(Some(count.book) == pane.chosen);
            if let Some(first) = &leader {
                button.set_group(Some(first));
            } else {
                leader = Some(button.clone());
            }
            let tx = sender.clone();
            let book = count.book;
            button.connect_clicked(move |btn| {
                if btn.is_active() {
                    tx.emit(Msg::SelectSearchBook(id, Some(book)));
                }
            });
            pane.chips.append(&button);
        }
    }

    fn preview_hit(&mut self, id: TabId, idx: i32) {
        if self.search(id).is_some_and(|pane| pane.hold.get()) {
            return;
        }
        let Ok(idx) = usize::try_from(idx) else {
            return;
        };
        let Some(hit) = self.search(id).and_then(|pane| pane.hits.get(idx).cloned()) else {
            return;
        };
        let Some(at) = search::hit_ref(&hit) else {
            return;
        };
        let tokens = self
            .search(id)
            .map(|pane| pane.tokens.clone())
            .unwrap_or_default();
        let strongs = self.search(id).and_then(|pane| pane.strongs.clone());
        self.search_mark = Some(SearchMark {
            at,
            tokens,
            strongs,
        });
        self.preview_at_in(self.window_of(id), at);
    }

    fn preview_at_in(&mut self, window: WindowId, at: Ref) {
        let id = match self.workspace.focused_passage_in(window) {
            Some(id) => id,
            None => {
                let opened = self.workspace.open_passage_in(window, at);
                self.spawn_passage(opened.id, at);
                opened.id
            }
        };
        let same = self
            .passage(id)
            .is_some_and(|p| p.at.book == at.book && p.at.chapter == at.chapter);
        if same {
            let mark = self.search_mark.clone();
            if let Some(p) = self.passage_mut(id) {
                p.at = at;
                p.highlight_verse(at.verse);
                if let Some(mark) = mark {
                    p.paint_search(mark.at.verse, &mark.tokens, mark.strongs.as_deref());
                }
            }
            self.workspace.navigate_passage(id, at);
            self.refresh_followers();
            self.sync_pickers();
            self.sync_tab_title(id);
            return;
        }
        self.apply_passage_ref(id, at, true, false);
    }

    fn open_hit(&mut self, id: TabId, idx: i32, beside: bool, dismiss: bool) {
        let Ok(idx) = usize::try_from(idx) else {
            return;
        };
        let Some(hit) = self.search(id).and_then(|pane| pane.hits.get(idx).cloned()) else {
            return;
        };
        let window = self.window_of(id);
        let at = search::hit_ref(&hit);
        if let Some(at) = at {
            let tokens = self
                .search(id)
                .map(|pane| pane.tokens.clone())
                .unwrap_or_default();
            let strongs = self.search(id).and_then(|pane| pane.strongs.clone());
            self.search_mark = Some(SearchMark {
                at,
                tokens,
                strongs,
            });
        }
        if dismiss {
            if let Some(pane) = self.search_mut(id) {
                pane.origin = None;
            }
            self.request_close(id);
        }
        match hit.kind {
            LibraryKind::Verse | LibraryKind::Note => {
                let Some(at) = at else { return };
                if !dismiss {
                    self.preview_at_in(window, at);
                } else if beside {
                    self.go_beside_in(window, at);
                } else {
                    self.go_in(window, at, true);
                }
            }
            LibraryKind::Commentary => {
                let Some(at) = at else { return };
                if dismiss {
                    self.go_in(window, at, true);
                } else {
                    self.preview_at_in(window, at);
                }
                if hit.module == "TSK" {
                    let _ = self.ensure_tsk();
                } else {
                    let _ = self.ensure_mhc();
                }
            }
            LibraryKind::Dictionary | LibraryKind::Topic | LibraryKind::Lexicon => {
                let Some(headword) = hit.headword.as_deref() else {
                    return;
                };
                let _ = self.open_library(&hit.module, Some(headword));
            }
        }
    }

    fn go(&mut self, at: Ref, highlight: bool) {
        let window = self
            .workspace
            .focused_tab()
            .map(|tab| tab.window)
            .unwrap_or(WindowId::MAIN);
        self.go_in(window, at, highlight);
    }

    fn go_in(&mut self, window: WindowId, at: Ref, highlight: bool) {
        let id = match self.workspace.focused_passage_in(window) {
            Some(id) => id,
            None => {
                let opened = self.workspace.open_passage_in(window, at);
                self.spawn_passage(opened.id, at);
                opened.id
            }
        };
        self.apply_passage_ref(id, at, highlight, true);
    }

    /// Open `at` beside the Treasury. Later clicks reuse that same passage.
    fn open_cite_beside(&mut self, source: TabId, at: Ref) {
        if let Some(id) = self.workspace.passage_beside(source) {
            self.show_cite(source, id, at);
            return;
        }
        if self.workspace.can_split(source) {
            let opened = self.workspace.open_passage_beside(source, at);
            self.spawn_passage(opened.id, at);
            self.finish_new_cite(source, opened.id, at);
            return;
        }
        if let Some(id) = self.cite_companion(source) {
            self.show_cite(source, id, at);
            return;
        }
        let opened = self.workspace.open_passage_in(self.window_of(source), at);
        self.spawn_passage(opened.id, at);
        self.finish_new_cite(source, opened.id, at);
    }

    fn finish_new_cite(&mut self, source: TabId, id: TabId, at: Ref) {
        self.workspace.navigate_passage_except(id, at, source);
        self.refresh_followers_except(Some(source));
        self.set_cite_companion(source, id);
        self.sync_pickers();
        self.save_state();
    }

    fn open_cite_tab(&mut self, source: TabId, at: Ref) {
        let opened = self.workspace.open_passage_in(self.window_of(source), at);
        self.spawn_passage(opened.id, at);
        self.select_tab(opened.id);
        self.save_state();
    }

    fn open_cite_window(&mut self, source: TabId, at: Ref) {
        let opened = self.workspace.open_passage_in(self.window_of(source), at);
        self.spawn_passage(opened.id, at);
        self.detach_tab(opened.id);
        self.save_state();
    }

    fn show_cite(&mut self, source: TabId, id: TabId, at: Ref) {
        if let Some(passage) = self.passage_mut(id) {
            passage.history.navigate(at);
            passage.at = at;
        }
        self.workspace.navigate_passage_except(id, at, source);
        self.reload_passage(id, true);
        self.refresh_followers_except(Some(source));
        self.sync_pickers();
        self.set_cite_companion(source, id);
        self.save_state();
    }

    fn set_cite_companion(&self, source: TabId, passage: TabId) {
        if let Some(widgets) = self.tsk_widgets_for(source) {
            widgets.companion.set(Some(passage));
        }
    }

    fn cite_companion(&self, source: TabId) -> Option<TabId> {
        let id = self.tsk_widgets_for(source)?.companion.get()?;
        self.workspace
            .tab(id)
            .filter(|tab| tab.kind.is_passage())
            .map(|tab| tab.id)
    }

    fn go_beside(&mut self, at: Ref) {
        let window = self
            .workspace
            .focused_tab()
            .map(|tab| tab.window)
            .unwrap_or(WindowId::MAIN);
        self.go_beside_in(window, at);
    }

    fn go_beside_in(&mut self, window: WindowId, at: Ref) {
        let from = self
            .workspace
            .focused_passage_in(window)
            .unwrap_or_else(|| self.workspace.focused());
        let opened = self.workspace.open_passage_beside(from, at);
        self.spawn_passage(opened.id, at);
        self.save_state();
    }

    fn apply_passage_ref(&mut self, id: TabId, at: Ref, highlight: bool, record_history: bool) {
        if record_history {
            if let Some(p) = self.passage_mut(id) {
                p.history.navigate(at);
            }
        }
        if let Some(p) = self.passage_mut(id) {
            p.at = at;
        }
        self.workspace.navigate_passage(id, at);
        self.reload_passage(id, highlight);
        self.refresh_followers();
        self.sync_pickers();
        self.save_state();
    }

    fn save_state(&self) {
        let mut state = config::State::from_ref(self.at(), self.font_size, self.paragraphs);
        state.search_mode = self.search_mode.index();
        state.column_width = self.column_width;
        config::save_state(&state);
    }

    fn apply_font(&self) {
        self.font_provider.load_from_string(&format!(
            "textview.chapter-view {{ font-size: {}pt; }}",
            self.font_size
        ));
        self.apply_column();
    }

    fn effective_column_px(&self) -> i32 {
        if self.column_width > 0 {
            layout::clamp_column_px(self.column_width)
        } else {
            layout::column_width_px(self.font_size)
        }
    }

    fn apply_column(&self) {
        let px = self.effective_column_px();
        for hosted in self.hosted.values() {
            if let TabContent::Passage(p) = &hosted.content {
                p.set_column_px(px);
            }
        }
    }

    /// Column edges belong to a chapter that fills its tab by itself.
    fn column_resize_for(&self, id: TabId) -> bool {
        let Some(tab) = self.workspace.tab(id) else {
            return true;
        };
        tab.kind.is_passage() && tab.host.is_none() && self.workspace.guest_of(id).is_none()
    }

    fn apply_column_mode(&self) {
        let ids: Vec<TabId> = self.hosted.keys().copied().collect();
        for id in ids {
            let on = self.column_resize_for(id);
            if let Some(TabContent::Passage(passage)) = self.hosted.get(&id).map(|h| &h.content) {
                passage.set_column_resize(on);
            }
        }
    }

    fn recolor_passages(&self) {
        for hosted in self.hosted.values() {
            match &hosted.content {
                TabContent::Passage(passage) => theme::paint_buffer(&passage.buffer),
                TabContent::Tsk(widgets) => theme::paint_buffer(&widgets.buffer),
                _ => {}
            }
        }
    }

    fn copy_from_selection_or_current(&self) {
        let Some(p) = self.focused_passage() else {
            return;
        };
        if let Some((from, to)) = p.selected_verse_range() {
            self.copy_verse_range(p.at, from, to);
            return;
        }
        self.copy_verse_range(p.at, p.at.verse, p.at.verse);
    }

    fn copy_verse_range(&self, at: Ref, from: u8, to: u8) {
        let Some(conn) = &self.conn else { return };
        let Ok(chapter) = bible_app_db::chapter(conn, at.book, at.chapter) else {
            return;
        };
        let (lo, hi) = if from <= to { (from, to) } else { (to, from) };
        let verses: Vec<_> = chapter
            .into_iter()
            .filter(|v| v.verse >= lo && v.verse <= hi)
            .collect();
        if verses.is_empty() {
            return;
        }
        let text = layout::copy_verses(&self.books, &verses);
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&text);
        }
    }

    fn handle_click(&mut self, id: TabId, offset: i32) {
        let tsk_hit = self.passage(id).and_then(|p| {
            p.tsk_at(offset)
                .map(|m| (m.span.start, m.heading.clone(), m.dests.clone()))
        });
        if let Some((start, heading, dests)) = tsk_hit {
            if let Some(p) = self.passage(id) {
                p.strongs_popover.popdown();
                tsk::present_phrase(
                    &p.tsk_popover,
                    &p.view,
                    start,
                    &heading,
                    &dests,
                    &self.books,
                    self.msg_tx.clone(),
                );
            }
            return;
        }
        if let Some(at) = self.passage(id).and_then(|p| p.xref_at(offset)) {
            self.apply_passage_ref(id, at, true, true);
            return;
        }
        if let Some(verse) = self.passage(id).and_then(|p| p.mhc_at(offset)) {
            let at = Ref {
                verse,
                ..self.passage(id).map(|p| p.at).unwrap_or_else(|| self.at())
            };
            self.apply_passage_ref(id, at, false, false);
            if let Some(p) = self.passage_mut(id) {
                p.select_verse(verse);
            }
            let _ = self.ensure_mhc();
            return;
        }
        let note = self
            .passage(id)
            .and_then(|p| p.note_at(offset).map(|n| (n.span.start, n.text.clone())));
        if let Some((start, text)) = note {
            if let Some(p) = self.passage(id) {
                strongs::present_text(&p.strongs_popover, &p.view, start, &text);
            }
            return;
        }
        let word = self
            .passage(id)
            .and_then(|p| layout::word_at_offset(&p.layout.words, offset).cloned());
        if word.is_some() {
            if let Some(verse) = self
                .passage(id)
                .and_then(|p| layout::verse_at_offset(&p.layout.verse_start, offset))
            {
                self.select_verse(id, verse);
            }
            self.open_strongs(id, offset);
            return;
        }
        if let Some(verse) = self
            .passage(id)
            .and_then(|p| layout::verse_at_offset(&p.layout.verse_start, offset))
        {
            if let Some(p) = self.passage(id) {
                p.strongs_popover.popdown();
            }
            self.select_verse(id, verse);
        }
    }

    fn select_verse(&mut self, id: TabId, verse: u8) {
        let changed = self
            .passage_mut(id)
            .map(|p| p.select_verse(verse))
            .unwrap_or(false);
        if changed {
            self.workspace.set_passage_verse(id, verse);
            self.refresh_followers();
            self.save_state();
        }
    }

    /// App-menu study links stay in the current tab bar instead of splitting.
    fn in_current_tabs<T>(&mut self, open: impl FnOnce(&mut Self) -> T) -> T {
        let beside = self.open_beside;
        self.open_beside = false;
        let opened = open(self);
        self.open_beside = beside;
        opened
    }

    fn ensure_mhc(&mut self) -> Option<TabId> {
        if self.error.is_some() {
            return None;
        }
        let at = self.at();
        let opened = self.workspace.open_mhc(at);
        let widgets = mhc::build();
        mhc::wire(&widgets, opened.id, self.msg_tx.clone(), &self.books);
        let root = widgets.root.clone();
        self.add_page(opened.id, &root, "Matthew Henry", TabContent::Mhc(widgets));
        if self.open_beside {
            self.move_tab_beside(opened.id);
        }
        self.present_study(opened.id, at, PlaceMemory::Restart);
        self.sync_tab_title(opened.id);
        Some(opened.id)
    }

    fn ensure_tsk(&mut self) -> Option<TabId> {
        if self.error.is_some() {
            return None;
        }
        let at = self.at();
        let opened = self.workspace.open_tsk(at);
        let widgets = tsk::build();
        tsk::wire(&widgets, opened.id, self.msg_tx.clone(), &self.books);
        let root = widgets.root.clone();
        self.add_page(opened.id, &root, "TSK", TabContent::Tsk(widgets));
        if self.open_beside {
            self.move_tab_beside(opened.id);
        }
        self.present_study(opened.id, at, PlaceMemory::Restart);
        self.sync_tab_title(opened.id);
        Some(opened.id)
    }

    fn open_library(&mut self, module: &str, headword: Option<&str>) -> Option<TabId> {
        if self.error.is_some() {
            return None;
        }
        let opened = self
            .workspace
            .open_library(module.to_string(), headword.map(str::to_string));
        let mut widgets = dict::build(opened.id, self.msg_tx.clone());
        dict::wire(&widgets, opened.id, self.msg_tx.clone());
        if let Some(conn) = &self.conn {
            dict::load_modules(&mut widgets, conn);
        }
        if let (Some(conn), Some(head)) = (&self.conn, headword) {
            dict::open_headword(&mut widgets, conn, module, head);
        } else {
            dict::select_module(&mut widgets, module);
        }
        let title = dict::tab_title(&widgets);
        let root = widgets.root.clone();
        self.add_page(opened.id, &root, &title, TabContent::Library(widgets));
        if self.open_beside {
            self.move_tab_beside(opened.id);
        }
        Some(opened.id)
    }

    fn open_occurrences(&mut self, code: &str) -> Option<TabId> {
        if self.error.is_some() {
            return None;
        }
        let opened = self.workspace.open_occurrences(code.to_string());
        let mut widgets = occurrences::build(opened.id, self.msg_tx.clone());
        if let Some(conn) = &self.conn {
            occurrences::fill(&mut widgets, conn, &self.books, code);
        }
        let title = format!("{code} in the KJV");
        let root = widgets.root.clone();
        self.add_page(opened.id, &root, &title, TabContent::Occurrences(widgets));
        self.move_tab_beside(opened.id);
        Some(opened.id)
    }

    fn ensure_bookmarks(&mut self) -> Option<TabId> {
        if self.error.is_some() || self.user.is_none() {
            return None;
        }
        let opened = self.workspace.open_bookmarks();
        let mut widgets = marks::build_bookmarks(opened.id, self.msg_tx.clone());
        if let Some(user) = &self.user {
            marks::fill_bookmarks(&mut widgets, user, &self.books);
        }
        let root = widgets.root.clone();
        self.add_page(
            opened.id,
            &root,
            "Bookmarks",
            TabContent::Bookmarks(widgets),
        );
        if self.open_beside {
            self.move_tab_beside(opened.id);
        }
        Some(opened.id)
    }

    fn ensure_notes(&mut self) -> Option<TabId> {
        if self.error.is_some() || self.user.is_none() {
            return None;
        }
        let opened = self.workspace.open_notes();
        let mut widgets = marks::build_notes(opened.id, self.msg_tx.clone());
        if let Some(user) = &self.user {
            marks::fill_notes(&mut widgets, user, &self.books);
        }
        let root = widgets.root.clone();
        self.add_page(opened.id, &root, "Notes", TabContent::Notes(widgets));
        if self.open_beside {
            self.move_tab_beside(opened.id);
        }
        Some(opened.id)
    }

    fn open_strongs(&mut self, id: TabId, offset: i32) {
        let (word, surface) = {
            let Some(p) = self.passage(id) else { return };
            let Some(word) = layout::word_at_offset(&p.layout.words, offset).cloned() else {
                return;
            };
            let mut surface = layout::token_at(&p.layout.text, offset);
            if surface.is_empty() {
                surface = layout::word_surface(&p.layout.text, word.span);
            }
            (word, surface)
        };
        let Some(conn) = &self.conn else { return };
        let defs: Vec<_> = word
            .codes
            .iter()
            .filter_map(|c| bible_app_db::lookup_strongs(conn, c).ok().flatten())
            .collect();
        let counts = strongs_counts(conn, &defs);
        let mut dict = bible_app_db::lookup_clicked_word(conn, &surface).unwrap_or_default();
        dict.lexicons = bible_app_db::lookup_lexicons_for_defs(conn, &defs).unwrap_or_default();
        if defs.is_empty() && dict.is_empty() {
            return;
        }
        let tx = self.msg_tx.clone();
        if let Some(TabContent::Passage(p)) = self.hosted.get_mut(&id).map(|h| &mut h.content) {
            p.strongs_at = word.span.start;
            p.tsk_popover.popdown();
            strongs::present(
                &p.strongs_popover,
                &p.view,
                p.strongs_at,
                &defs,
                &counts,
                &dict,
                tx,
            );
        }
    }

    fn open_strongs_code(&mut self, code: &str) {
        let Some(id) = self.workspace.focused_passage_id() else {
            return;
        };
        let Some(conn) = &self.conn else { return };
        let Some(def) = bible_app_db::lookup_strongs(conn, code).ok().flatten() else {
            return;
        };
        let counts = strongs_counts(conn, std::slice::from_ref(&def));
        let dict = bible_app_db::ClickedDict {
            lexicons: bible_app_db::lookup_lexicons_for_defs(conn, std::slice::from_ref(&def))
                .unwrap_or_default(),
            ..Default::default()
        };
        if let Some(p) = self.passage(id) {
            p.tsk_popover.popdown();
            strongs::present(
                &p.strongs_popover,
                &p.view,
                p.strongs_at,
                &[def],
                &counts,
                &dict,
                self.msg_tx.clone(),
            );
        }
    }

    fn present_study(&mut self, id: TabId, at: Ref, memory: PlaceMemory) {
        self.workspace.set_study_at(id, at);
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Mhc(widgets)) => {
                mhc::show(widgets, conn, &self.books, at);
                remember_study(&widgets.history, &widgets.placed, at, memory);
            }
            Some(TabContent::Tsk(widgets)) => {
                tsk::show(widgets, conn, &self.books, at);
                remember_study(&widgets.history, &widgets.placed, at, memory);
            }
            _ => return,
        }
        if let Some(page) = self.page_of(id) {
            page.set_tooltip(&nav::format_ref(&self.books, at));
        }
        self.sync_study_bar(id);
    }

    fn refresh_followers(&mut self) {
        self.refresh_followers_except(None);
    }

    fn refresh_followers_except(&mut self, except: Option<TabId>) {
        let following: Vec<(TabId, Ref)> = self
            .workspace
            .tabs()
            .iter()
            .filter(|tab| Some(tab.id) != except && tab.kind.follows_verse())
            .filter_map(|tab| tab.kind.at().map(|at| (tab.id, at)))
            .collect();
        for (id, at) in following {
            self.present_study(id, at, PlaceMemory::Retarget);
        }
    }

    fn reload_passage(&mut self, id: TabId, highlight: bool) {
        let paragraphs = self.paragraphs;
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let mark = self.search_mark.clone();
        if let Some(TabContent::Passage(p)) = self.hosted.get_mut(&id).map(|h| &mut h.content) {
            p.load(
                &passage::ChapterCtx {
                    conn,
                    books: &self.books,
                    user: self.user.as_ref(),
                    paragraphs,
                },
                highlight,
            );
            if let Some(mark) = mark {
                if p.at.book == mark.at.book && p.at.chapter == mark.at.chapter {
                    p.paint_search(mark.at.verse, &mark.tokens, mark.strongs.as_deref());
                }
            }
        }
        self.sync_tab_title(id);
    }

    fn reload_all_passages(&mut self, highlight: bool) {
        let ids: Vec<TabId> = self
            .hosted
            .iter()
            .filter_map(|(id, h)| match h.content {
                TabContent::Passage(_) => Some(*id),
                _ => None,
            })
            .collect();
        for id in ids {
            self.reload_passage(id, highlight);
        }
    }

    fn spawn_passage(&mut self, id: TabId, at: Ref) {
        let p = PassageView::new(at, &self.actions, self.effective_column_px());
        p.wire(id, self.msg_tx.clone());
        p.wire_nav(id, self.msg_tx.clone(), &self.books);
        let title = nav::format_chapter(&self.books, at.book, at.chapter);
        let page = p.page.clone();
        self.add_page(id, &page, &title, TabContent::Passage(Box::new(p)));
        self.reload_passage(id, false);
        self.sync_passage_bar(id);
    }

    fn add_page(
        &mut self,
        id: TabId,
        child: &impl IsA<gtk::Widget>,
        title: &str,
        content: TabContent,
    ) {
        let body = child.clone().upcast::<gtk::Widget>();
        let window = self
            .workspace
            .tab(id)
            .map(|tab| tab.window)
            .unwrap_or(WindowId::MAIN);
        let host = self.workspace.tab(id).and_then(|tab| tab.host);
        if let Some(host) = host {
            self.hosted.insert(
                id,
                HostedTab {
                    page: None,
                    slot: None,
                    body,
                    content,
                },
            );
            self.layout_tab(host);
            self.select_tab(host);
            self.sync_shells();
            return;
        }
        let Some(view) = self.view_of(window) else {
            return;
        };
        let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        slot.set_hexpand(true);
        slot.set_vexpand(true);
        let page = view.append(&slot);
        page.set_keyword(&id.keyword());
        page.set_title(title);
        if let Some(at) = self.workspace.tab(id).and_then(|t| t.kind.at()) {
            if !self.workspace.tab(id).is_some_and(|t| t.kind.is_passage()) {
                page.set_tooltip(&nav::format_ref(&self.books, at));
            }
        }
        self.hosted.insert(
            id,
            HostedTab {
                page: Some(page),
                slot: Some(slot),
                body,
                content,
            },
        );
        self.layout_tab(id);
        self.select_tab(id);
        self.sync_shells();
    }

    fn page_of(&self, id: TabId) -> Option<adw::TabPage> {
        self.hosted.get(&id).and_then(|hosted| hosted.page.clone())
    }

    fn new_tab(&mut self, window: WindowId) {
        if self.error.is_some() {
            return;
        }
        let opened = self.workspace.open_blank(window);
        let page = launcher::build(
            opened.id,
            self.msg_tx.clone(),
            !self.dict_modules.is_empty(),
            self.user.is_some(),
            &self.books,
        );
        page.syncing.set(true);
        if let Some(book) = picker::book_id_at(&self.books, page.book.selected()) {
            let chapters = self
                .conn
                .as_ref()
                .and_then(|conn| bible_app_db::max_chapter(conn, book).ok())
                .unwrap_or(1);
            picker::sync_chapters(&page.chapter, chapters, 1);
        }
        page.syncing.set(false);
        let root = page.root.clone();
        self.add_page(opened.id, &root, "New", TabContent::Blank(page));
    }

    fn focus_blank_entry(&self, id: TabId) {
        let Some(TabContent::Blank(page)) = self.hosted.get(&id).map(|h| &h.content) else {
            return;
        };
        let book = page.book.clone();
        glib::idle_add_local_once(move || {
            if book.parent().is_some() {
                book.grab_focus();
            }
        });
    }

    /// Put `id`'s page where `before` is, when both are in the same pane.
    fn reorder_before(&self, id: TabId, before: TabId) {
        if id == before {
            return;
        }
        let Some(page) = self.page_of(id) else {
            return;
        };
        let Some(sibling) = self.page_of(before) else {
            return;
        };
        let Some(view) = self.view_holding(&page) else {
            return;
        };
        let Some(other) = self.view_holding(&sibling) else {
            return;
        };
        if view != other {
            return;
        };
        let pos = view.page_position(&sibling);
        if pos >= 0 {
            view.reorder_page(&page, pos);
        }
    }

    fn blank_select_book(&mut self, id: TabId, idx: u32) {
        let Some(book) = picker::book_id_at(&self.books, idx) else {
            return;
        };
        self.blank_open(
            id,
            Ref {
                book,
                chapter: 1,
                verse: 1,
            },
        );
    }

    fn blank_select_chapter(&mut self, id: TabId, idx: u32) {
        let Some(chapter) = picker::chapter_from_index(idx) else {
            return;
        };
        let Some(book) = self.blank_selected_book(id) else {
            return;
        };
        self.blank_open(
            id,
            Ref {
                book,
                chapter,
                verse: 1,
            },
        );
    }

    fn blank_selected_book(&self, id: TabId) -> Option<u8> {
        let TabContent::Blank(page) = self.hosted.get(&id).map(|h| &h.content)? else {
            return None;
        };
        picker::book_id_at(&self.books, page.book.selected())
    }

    fn blank_open(&mut self, id: TabId, at: Ref) {
        if !self.workspace.tab(id).is_some_and(|t| t.kind.is_blank()) {
            return;
        }
        self.workspace.focus(id);
        let window = self.window_of(id);
        let opened = self.workspace.open_passage_in(window, at);
        self.spawn_passage(opened.id, at);
        self.reorder_before(opened.id, id);
        self.request_close(id);
        self.save_state();
    }

    fn launch_from_blank(&mut self, id: TabId, choice: launcher::Launch) {
        if self.error.is_some() || !self.workspace.tab(id).is_some_and(|t| t.kind.is_blank()) {
            return;
        }
        self.workspace.focus(id);
        let window = self.window_of(id);
        self.open_beside = false;
        let target = match choice {
            launcher::Launch::Search => self.open_search_tab(window),
            launcher::Launch::Mhc => self.ensure_mhc(),
            launcher::Launch::Tsk => self.ensure_tsk(),
            launcher::Launch::Library => {
                let module = self
                    .dict_modules
                    .iter()
                    .find(|module| module.kind == "dictionary")
                    .or_else(|| self.dict_modules.first())
                    .map(|module| module.id.clone());
                module.and_then(|module| self.open_library(&module, None))
            }
            launcher::Launch::Notes => self.ensure_notes(),
            launcher::Launch::Bookmarks => self.ensure_bookmarks(),
        };
        self.open_beside = true;
        let Some(target) = target else {
            return;
        };
        if target != id {
            self.reorder_before(target, id);
            self.request_close(id);
        }
    }

    fn sync_tab_title(&self, id: TabId) {
        let Some(tab) = self.workspace.tab(id) else {
            return;
        };
        let Some(hosted) = self.hosted.get(&id) else {
            return;
        };
        let title = match &tab.kind {
            TabKind::Passage { at } => nav::format_chapter(&self.books, at.book, at.chapter),
            TabKind::Mhc { .. } => "Matthew Henry".into(),
            TabKind::Tsk { .. } => "TSK".into(),
            TabKind::Library { .. } => match &hosted.content {
                TabContent::Library(w) => dict::tab_title(w),
                _ => "Library".into(),
            },
            TabKind::Bookmarks => "Bookmarks".into(),
            TabKind::Notes => "Notes".into(),
            TabKind::Occurrences { code } => format!("{code} in the KJV"),
            TabKind::Search => "Search".into(),
            TabKind::Blank => "New".into(),
        };
        let Some(page) = hosted.page.clone() else {
            return;
        };
        page.set_title(&title);
        if let Some(at) = tab.kind.at() {
            if !tab.kind.is_passage() {
                page.set_tooltip(&nav::format_ref(&self.books, at));
            }
        }
    }

    fn select_tab(&self, id: TabId) {
        let shown = self
            .workspace
            .tab(id)
            .and_then(|tab| tab.host)
            .unwrap_or(id);
        let Some(page) = self.page_of(shown) else {
            return;
        };
        let window = self.window_of(shown);
        let Some(view) = self.view_of(window) else {
            return;
        };
        view.set_selected_page(&page);
        if let Some(win) = view.root().and_downcast::<gtk::Window>() {
            win.present();
        }
    }

    fn focus_tab(&mut self, id: TabId) {
        self.workspace.focus(id);
        self.select_tab(id);
        self.sync_pickers();
    }

    fn request_close(&self, id: TabId) {
        let Some(page) = self.page_of(id) else {
            self.msg_tx.emit(Msg::CloseGuest(id));
            return;
        };
        if let Some(view) = self.view_holding(&page) {
            view.close_page(&page);
        }
    }

    fn forget_tab(&mut self, id: TabId) {
        let restore = self.search(id).and_then(|pane| pane.origin);
        let window = self.window_of(id);
        let guest = self.workspace.guest_of(id);
        let host = self.workspace.tab(id).and_then(|tab| tab.host);
        if let Some(body) = self.hosted.get(&id).map(|hosted| hosted.body.clone()) {
            unparent(&body);
        }
        if let Some(guest) = guest {
            if let Some(body) = self.hosted.get(&guest).map(|hosted| hosted.body.clone()) {
                unparent(&body);
            }
        }
        if self.hosted.remove(&id).is_none() {
            return;
        }
        let _ = self.workspace.close(id);
        if let Some(guest) = guest {
            self.mount_real_tab(guest);
        }
        if let Some(host) = host {
            self.layout_tab(host);
        }
        if let Some(at) = restore {
            self.search_mark = None;
            if let Some(passage) = self.workspace.focused_passage_in(window) {
                self.apply_passage_ref(passage, at, true, false);
            }
        }
        self.sync_shells();
        self.sync_pickers();
    }

    fn view_of(&self, window: WindowId) -> Option<adw::TabView> {
        if window == WindowId::MAIN {
            return self.shell.as_ref().map(|shell| shell.host.view.clone());
        }
        self.sides
            .borrow()
            .iter()
            .find(|side| side.id == window)
            .map(|side| side.chrome.shell.host.view.clone())
    }

    fn view_holding(&self, page: &adw::TabPage) -> Option<adw::TabView> {
        let mut views = Vec::new();
        if let Some(shell) = &self.shell {
            views.push(shell.host.view.clone());
        }
        for side in self.sides.borrow().iter() {
            views.push(side.chrome.shell.host.view.clone());
        }
        views
            .into_iter()
            .find(|view| (0..view.n_pages()).any(|i| view.nth_page(i) == *page))
    }

    fn sync_shells(&self) {
        if let Some(shell) = &self.shell {
            self.sync_split_buttons(shell);
        }
        for side in self.sides.borrow().iter() {
            self.sync_split_buttons(&side.chrome.shell);
        }
    }

    fn sync_split_buttons(&self, shell: &SplitShell) {
        let icon = gio::ThemedIcon::new("view-dual-symbolic");
        let n = shell.host.view.n_pages();
        for i in 0..n {
            let page = shell.host.view.nth_page(i);
            let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) else {
                continue;
            };
            let on = self.workspace.can_split(id);
            page.set_indicator_icon(Some(&icon));
            page.set_indicator_activatable(on);
            page.set_indicator_tooltip(if self.workspace.guest_of(id).is_some() {
                "This tab is split"
            } else if on {
                "Split this view"
            } else {
                "Already beside another view"
            });
        }
    }

    /// Rebuild one tab's page: the view alone, or that view beside its guest.
    fn layout_tab(&mut self, id: TabId) {
        // Collapse column insets before the paned measures the chapter.
        self.apply_column_mode();
        let Some(slot) = self.hosted.get(&id).and_then(|hosted| hosted.slot.clone()) else {
            return;
        };
        let host_body = self.hosted.get(&id).map(|hosted| hosted.body.clone());
        let guest = self.workspace.guest_of(id);
        let guest_body = guest.and_then(|guest| self.hosted.get(&guest).map(|h| h.body.clone()));
        if let Some(body) = &host_body {
            unparent(body);
        }
        if let Some(body) = &guest_body {
            unparent(body);
        }
        while let Some(child) = slot.first_child() {
            child.unparent();
        }
        match (host_body, guest, guest_body) {
            (Some(host_body), Some(guest_id), Some(guest_body)) => {
                host_body.set_hexpand(true);
                host_body.set_vexpand(true);
                guest_body.set_hexpand(true);
                guest_body.set_vexpand(true);
                let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
                paned.add_css_class("pane-split");
                paned.set_hexpand(true);
                paned.set_vexpand(true);
                paned.set_wide_handle(true);
                paned.set_resize_start_child(true);
                paned.set_resize_end_child(true);
                // Chapter margins can still report a wide minimum on the first
                // measure. Allow the handle to sit at half the tab anyway.
                paned.set_shrink_start_child(true);
                paned.set_shrink_end_child(true);
                paned.set_start_child(Some(&host_body));
                paned.set_end_child(Some(&guest_frame(guest_id, &guest_body, &self.msg_tx)));
                shell::mark_split_handle(&paned);
                center_split(&paned);
                slot.append(&paned);
                // The slot already has the tab's size. Nothing upstream
                // reallocates it for a new child, so the paned would stay
                // 0×0 and the old chapter allocation would keep painting.
                allocate_to_parent(&paned);
            }
            (Some(host_body), _, _) => {
                slot.append(&host_body);
                allocate_to_parent(&host_body);
                // The chapter was measured at the split width. Fit the column
                // to the full tab now that the body has that size.
                self.apply_column_mode();
            }
            _ => {}
        }
    }

    /// Give a promoted view its own tab again.
    fn mount_real_tab(&mut self, id: TabId) {
        let Some(tab) = self.workspace.tab(id).cloned() else {
            return;
        };
        if tab.host.is_some() || self.page_of(id).is_some() {
            return;
        }
        let Some(body) = self.hosted.get(&id).map(|hosted| hosted.body.clone()) else {
            return;
        };
        let Some(view) = self.view_of(tab.window) else {
            return;
        };
        unparent(&body);
        let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        slot.set_hexpand(true);
        slot.set_vexpand(true);
        let page = view.append(&slot);
        page.set_keyword(&id.keyword());
        if let Some(hosted) = self.hosted.get_mut(&id) {
            hosted.page = Some(page);
            hosted.slot = Some(slot);
        }
        self.sync_tab_title(id);
        self.layout_tab(id);
        self.select_tab(id);
    }

    /// Drop `id` out of the tab bar and draw it inside its host.
    fn embed_in_host(&mut self, id: TabId) {
        let Some(host) = self.workspace.tab(id).and_then(|tab| tab.host) else {
            return;
        };
        let Some(page) = self.page_of(id) else {
            self.layout_tab(host);
            self.select_tab(host);
            return;
        };
        let Some(view) = self.view_holding(&page) else {
            return;
        };
        if let Some(body) = self.hosted.get(&id).map(|hosted| hosted.body.clone()) {
            unparent(&body);
        }
        if let Some(hosted) = self.hosted.get_mut(&id) {
            hosted.page = None;
            hosted.slot = None;
        }
        self.embedding.insert(id);
        let pages = view.n_pages();
        view.close_page(&page);
        // close-page confirms through its default handler. If that did not
        // remove the page, finish the request so the view leaves the tab bar.
        if view.n_pages() == pages {
            view.close_page_finish(&page, true);
        }
        self.layout_tab(host);
        self.select_tab(host);
        self.sync_shells();
    }

    fn tab_attached(&mut self, id: TabId, window: WindowId) {
        let carried = self.workspace.guest_of(id);
        self.workspace.place(id, window);
        if let Some(guest) = carried.filter(|_| self.workspace.guest_of(id).is_none()) {
            if let Some(body) = self.hosted.get(&guest).map(|hosted| hosted.body.clone()) {
                unparent(&body);
            }
            self.layout_tab(id);
            self.mount_real_tab(guest);
            self.workspace.focus(id);
            self.select_tab(id);
        }
        self.sync_shells();
    }

    fn ensure_side(&mut self, id: WindowId) {
        if self.sides.borrow().iter().any(|side| side.id == id) {
            return;
        }
        let chrome = shell::open_side_window(&build_app_menu(&self.dict_modules));
        chrome
            .window
            .insert_action_group("win", Some(&self.actions));
        let ctx = self.wire_ctx();
        wire_host(&chrome.shell.host, id, &ctx);
        self.sides.borrow_mut().push(SideWindow { id, chrome });
        self.wire_side(id);
    }

    fn wire_ctx(&self) -> WireCtx {
        WireCtx {
            sender: self.msg_tx.clone(),
            sides: self.sides.clone(),
            counter: self.workspace.window_counter(),
            actions: self.actions.clone(),
            menu: shell::tab_menu_model(),
            app_menu: build_app_menu(&self.dict_modules),
        }
    }

    fn wire_side(&mut self, id: WindowId) {
        let Some((goto_entry, goto_popover, window)) = ({
            let sides = self.sides.borrow();
            let Some(side) = sides.iter().find(|side| side.id == id) else {
                return;
            };
            Some((
                side.chrome.goto_entry.clone(),
                side.chrome.goto_popover.clone(),
                side.chrome.window.clone(),
            ))
        }) else {
            return;
        };
        let tx = self.msg_tx.clone();
        goto_entry.connect_activate(move |entry| tx.emit(Msg::GoTo(id, entry.text().to_string())));
        let shift = gtk::EventControllerKey::new();
        shift.set_propagation_phase(gtk::PropagationPhase::Capture);
        let tx = self.msg_tx.clone();
        let entry = goto_entry.clone();
        shift.connect_key_pressed(move |_, keyval, _, mods| {
            let shifted = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shifted && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                tx.emit(Msg::GoToBeside(id, entry.text().to_string()));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        goto_entry.add_controller(shift);
        let keys = gtk::EventControllerKey::new();
        let tx = self.msg_tx.clone();
        let pop = goto_popover.clone();
        let entry = goto_entry.clone();
        keys.connect_key_pressed(move |_, keyval, _, mods| {
            let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let alt = mods.contains(gtk::gdk::ModifierType::ALT_MASK);
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if ctrl && (keyval == gtk::gdk::Key::f || keyval == gtk::gdk::Key::F) {
                tx.emit(Msg::SetSearch(id, true));
                return glib::Propagation::Stop;
            }
            if ctrl && !shift && (keyval == gtk::gdk::Key::t || keyval == gtk::gdk::Key::T) {
                tx.emit(Msg::NewTab(id));
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Escape {
                if pop.is_visible() {
                    pop.popdown();
                    return glib::Propagation::Stop;
                }
                tx.emit(Msg::Escape(id));
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::l || keyval == gtk::gdk::Key::L) {
                pop.popup();
                entry.grab_focus();
                entry.select_region(0, -1);
                return glib::Propagation::Stop;
            }
            if alt && shift && keyval == gtk::gdk::Key::Left {
                tx.emit(Msg::Back(id));
                return glib::Propagation::Stop;
            }
            if alt && shift && keyval == gtk::gdk::Key::Right {
                tx.emit(Msg::Forward(id));
                return glib::Propagation::Stop;
            }
            if alt && keyval == gtk::gdk::Key::Left {
                tx.emit(Msg::PrevChapter(id));
                return glib::Propagation::Stop;
            }
            if alt && keyval == gtk::gdk::Key::Right {
                tx.emit(Msg::NextChapter(id));
                return glib::Propagation::Stop;
            }
            if ctrl
                && (keyval == gtk::gdk::Key::plus
                    || keyval == gtk::gdk::Key::equal
                    || keyval == gtk::gdk::Key::KP_Add)
            {
                tx.emit(Msg::FontLarger);
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::minus || keyval == gtk::gdk::Key::KP_Subtract) {
                tx.emit(Msg::FontSmaller);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        window.add_controller(keys);
        let pop = goto_popover.clone();
        window.connect_destroy(move |_| {
            pop.unparent();
        });
        self.sync_pickers();
    }

    fn detach_tab(&mut self, id: TabId) {
        let Some(page) = self.page_of(id) else {
            return;
        };
        let Some(from) = self.view_holding(&page) else {
            return;
        };
        let Some(outcome) = self.workspace.detach(id) else {
            return;
        };
        self.ensure_side(outcome.window);
        let Some(dest) = self.view_of(outcome.window) else {
            return;
        };
        from.transfer_page(&page, &dest, 0);
        self.sync_shells();
        self.select_tab(id);
    }

    fn move_tab_beside(&mut self, id: TabId) {
        if !self.workspace.move_beside(id) {
            return;
        }
        self.embed_in_host(id);
        self.sync_shells();
    }

    fn split_tab(&mut self, id: TabId) {
        match self.workspace.split_tab(id) {
            SplitOutcome::Duplicated { id: new_id, at } => {
                self.spawn_passage(new_id, at);
            }
            SplitOutcome::Moved => self.embed_in_host(id),
            SplitOutcome::AlreadyBeside => {}
        }
        self.sync_shells();
    }

    fn popdown_passage_popovers(&self) {
        for h in self.hosted.values() {
            if let TabContent::Passage(p) = &h.content {
                p.popdown();
            }
        }
    }

    fn reload_user_marks(&mut self, refresh_lists: bool) {
        let ids: Vec<TabId> = self
            .hosted
            .iter()
            .filter_map(|(id, h)| match h.content {
                TabContent::Passage(_) => Some(*id),
                _ => None,
            })
            .collect();
        for id in ids {
            if let Some(TabContent::Passage(p)) = self.hosted.get_mut(&id).map(|h| &mut h.content) {
                p.reload_marks(self.user.as_ref(), &self.books, self.conn.as_ref());
            }
        }
        if refresh_lists {
            let ids: Vec<TabId> = self
                .hosted
                .iter()
                .filter_map(|(id, h)| match h.content {
                    TabContent::Bookmarks(_) | TabContent::Notes(_) => Some(*id),
                    _ => None,
                })
                .collect();
            for id in ids {
                match (
                    self.hosted.get_mut(&id).map(|h| &mut h.content),
                    self.user.as_ref(),
                ) {
                    (Some(TabContent::Bookmarks(widgets)), Some(user)) => {
                        marks::fill_bookmarks(widgets, user, &self.books);
                    }
                    (Some(TabContent::Notes(widgets)), Some(user)) => {
                        marks::fill_notes(widgets, user, &self.books);
                    }
                    _ => {}
                }
            }
        }
    }

    fn toggle_bookmark(&mut self) {
        let at = self.at();
        if let Some(user) = &self.user {
            let _ = user_db::toggle_bookmark(user, at);
        }
        self.reload_user_marks(true);
    }

    fn set_highlight(&mut self, color: &str) {
        let Some(p) = self.focused_passage() else {
            return;
        };
        let at = p.at;
        let spans = p.selected_highlight_spans();
        let spans = if spans.is_empty() {
            vec![(at.verse, 0, user_db::WHOLE_VERSE)]
        } else {
            spans
        };
        let color = if color.is_empty() { None } else { Some(color) };
        if let Some(user) = &self.user {
            let _ = user_db::apply_highlights(user, at.book, at.chapter, &spans, color);
        }
        self.reload_user_marks(false);
    }

    fn add_note(&mut self) {
        let Some(id) = self.ensure_notes() else {
            return;
        };
        let at = self.at();
        let text = self
            .user
            .as_ref()
            .and_then(|u| user_db::get_note(u, at).ok().flatten())
            .unwrap_or_default();
        if let Some(TabContent::Notes(w)) = self.hosted.get_mut(&id).map(|h| &mut h.content) {
            marks::edit_note(w, &self.books, at, &text);
        }
    }

    fn show_verse_menu(&mut self, id: TabId, offset: i32, x: i32, y: i32) {
        if self.error.is_some() {
            return;
        }
        self.focus_tab(id);
        if let Some(p) = self.passage_mut(id) {
            p.capture_menu_sel();
            p.tsk_popover.popdown();
            p.strongs_popover.popdown();
        }
        if let Some(verse) = self
            .passage(id)
            .and_then(|p| layout::verse_at_offset(&p.layout.verse_start, offset))
        {
            self.select_verse(id, verse);
        }
        let Some(p) = self.passage_mut(id) else {
            return;
        };
        let mark = p.chapter_marks.get(&p.at.verse);
        let bookmarked = mark.is_some_and(|m| m.bookmark);
        let has_note = mark.is_some_and(|m| m.note);
        p.verse_menu
            .set_menu_model(Some(&marks::verse_menu_model(bookmarked, has_note)));
        p.verse_menu
            .set_pointing_to(Some(&gtk::gdk::Rectangle::new(x, y, 1, 1)));
        p.verse_menu.popup();
    }

    fn export_notes(&self, sender: &ComponentSender<Self>) {
        if self.user.is_none() || self.conn.is_none() {
            return;
        }
        let dialog = gtk::FileDialog::new();
        dialog.set_title("Export notes");
        dialog.set_initial_name(Some("bible-app-notes.md"));
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Markdown"));
        filter.add_suffix("md");
        filter.add_mime_type("text/markdown");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        dialog.set_default_filter(Some(&filter));
        let parent = self
            .focused_passage()
            .and_then(|p| p.view.root().and_downcast::<gtk::Window>());
        let sender = sender.clone();
        dialog.save(parent.as_ref(), None::<&gio::Cancellable>, move |result| {
            if let Ok(file) = result {
                if let Some(path) = file.path() {
                    sender.input(Msg::ExportWrite(path));
                }
            }
        });
    }

    fn write_export(&self, path: &std::path::Path) {
        let Some(user) = &self.user else { return };
        let Some(conn) = &self.conn else { return };
        match user_db::export_markdown(user, conn, &self.books) {
            Ok(md) => {
                if let Err(e) = std::fs::write(path, md) {
                    self.alert("Could not export notes", &e.to_string());
                }
            }
            Err(e) => self.alert("Could not export notes", &e.to_string()),
        }
    }

    fn alert(&self, title: &str, body: &str) {
        let dlg = adw::AlertDialog::new(Some(title), Some(body));
        dlg.add_response("ok", "OK");
        let parent = self
            .focused_passage()
            .and_then(|p| p.view.root().and_downcast::<gtk::Window>());
        dlg.present(parent.as_ref());
    }

    fn sync_pickers(&self) {
        let ids: Vec<TabId> = self.hosted.keys().copied().collect();
        for id in ids {
            self.sync_passage_bar(id);
        }
    }

    fn sync_passage_bar(&self, id: TabId) {
        let Some(at) = self.passage(id).map(|p| p.at) else {
            return;
        };
        let chapters = self
            .conn
            .as_ref()
            .and_then(|conn| bible_app_db::max_chapter(conn, at.book).ok())
            .unwrap_or(1);
        let Some(p) = self.passage(id) else {
            return;
        };
        p.bar_syncing.set(true);
        picker::select_book(&p.book, &self.books, at.book);
        picker::sync_chapters(&p.chapter, chapters, at.chapter);
        p.bar_syncing.set(false);
        p.sync_history_buttons();
    }

    fn alt_step(&mut self, window: WindowId, step: AltStep) {
        let focused = self.workspace.focused_in(window);
        let kind = focused.and_then(|id| self.workspace.tab(id).map(|tab| tab.kind.clone()));
        match (focused, kind, step) {
            (Some(id), Some(TabKind::Mhc { .. } | TabKind::Tsk { .. }), AltStep::Prev) => {
                self.study_chapter_step(id, false);
            }
            (Some(id), Some(TabKind::Mhc { .. } | TabKind::Tsk { .. }), AltStep::Next) => {
                self.study_chapter_step(id, true);
            }
            (Some(id), Some(TabKind::Mhc { .. } | TabKind::Tsk { .. }), AltStep::Back) => {
                self.study_history(id, false);
            }
            (Some(id), Some(TabKind::Mhc { .. } | TabKind::Tsk { .. }), AltStep::Forward) => {
                self.study_history(id, true);
            }
            (Some(id), Some(TabKind::Library { .. }), AltStep::Prev) => {
                self.library_step(id, false)
            }
            (Some(id), Some(TabKind::Library { .. }), AltStep::Next) => self.library_step(id, true),
            (Some(id), Some(TabKind::Library { .. }), AltStep::Back) => {
                self.library_history(id, false);
            }
            (Some(id), Some(TabKind::Library { .. }), AltStep::Forward) => {
                self.library_history(id, true);
            }
            (_, _, AltStep::Prev) => {
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::prev_chapter(conn, &self.books, self.at_in(window)) {
                        self.go_in(window, at, false);
                    }
                }
            }
            (_, _, AltStep::Next) => {
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::next_chapter(conn, &self.books, self.at_in(window)) {
                        self.go_in(window, at, false);
                    }
                }
            }
            (_, _, AltStep::Back) => self.passage_history(window, false),
            (_, _, AltStep::Forward) => self.passage_history(window, true),
        }
    }

    fn passage_history(&mut self, window: WindowId, forward: bool) {
        let Some(id) = self.workspace.focused_passage_in(window) else {
            return;
        };
        let at = if forward {
            self.passage_mut(id)
                .and_then(|passage| passage.history.forward())
        } else {
            self.passage_mut(id)
                .and_then(|passage| passage.history.back())
        };
        if let Some(at) = at {
            self.apply_passage_ref(id, at, true, false);
        }
    }

    fn study_book(&mut self, id: TabId, idx: u32) {
        let Some(book) = picker::book_id_at(&self.books, idx) else {
            return;
        };
        let Some(at) = self.workspace.tab(id).and_then(|tab| tab.kind.at()) else {
            return;
        };
        if at.book == book {
            return;
        }
        self.move_study(
            id,
            Ref {
                book,
                chapter: 1,
                verse: 1,
            },
        );
    }

    fn study_chapter(&mut self, id: TabId, idx: u32) {
        let Some(chapter) = picker::chapter_from_index(idx) else {
            return;
        };
        let Some(at) = self.workspace.tab(id).and_then(|tab| tab.kind.at()) else {
            return;
        };
        if at.chapter == chapter {
            return;
        }
        self.move_study(
            id,
            Ref {
                book: at.book,
                chapter,
                verse: 1,
            },
        );
    }

    fn study_prev(&mut self, id: TabId) {
        self.study_chapter_step(id, false);
    }

    fn study_next(&mut self, id: TabId) {
        self.study_chapter_step(id, true);
    }

    fn study_chapter_step(&mut self, id: TabId, forward: bool) {
        let Some(at) = self.workspace.tab(id).and_then(|tab| tab.kind.at()) else {
            return;
        };
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let stepped = if forward {
            nav::next_chapter(conn, &self.books, at)
        } else {
            nav::prev_chapter(conn, &self.books, at)
        };
        let Ok(at) = stepped else {
            return;
        };
        self.move_study(id, at);
    }

    fn move_study(&mut self, id: TabId, at: Ref) {
        if self
            .workspace
            .tab(id)
            .is_some_and(|tab| tab.kind.follows_verse())
        {
            let window = self.window_of(id);
            if let Some(passage) = self.workspace.focused_passage_in(window) {
                self.apply_passage_ref(passage, at, false, true);
            } else {
                self.go_in(window, at, false);
            }
            return;
        }
        self.present_study(id, at, PlaceMemory::Navigate);
    }

    fn study_history(&mut self, id: TabId, forward: bool) {
        if self
            .workspace
            .tab(id)
            .is_some_and(|tab| tab.kind.follows_verse())
        {
            self.passage_history(self.window_of(id), forward);
            return;
        }
        let at = study_history_move(self.hosted.get(&id).map(|hosted| &hosted.content), forward);
        if let Some(at) = at {
            self.present_study(id, at, PlaceMemory::Keep);
        }
    }

    fn library_step(&mut self, id: TabId, forward: bool) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let key = match self.hosted.get(&id).map(|hosted| &hosted.content) {
            Some(TabContent::Library(widgets)) => dict::step(widgets, conn, forward),
            _ => None,
        };
        if let Some(key) = key {
            self.workspace.set_library_headword(id, Some(key));
        }
    }

    fn library_history(&mut self, id: TabId, forward: bool) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let key = match self.hosted.get(&id).map(|hosted| &hosted.content) {
            Some(TabContent::Library(widgets)) => dict::history_step(widgets, conn, forward),
            _ => None,
        };
        if let Some(key) = key {
            self.workspace.set_library_headword(id, Some(key));
        }
    }

    fn open_dict_hit(&mut self, id: TabId, idx: i32) {
        let Some(conn) = self.conn.as_ref() else {
            return;
        };
        let key = match self.hosted.get(&id).map(|hosted| &hosted.content) {
            Some(TabContent::Library(widgets)) => dict::open_hit(widgets, conn, idx),
            _ => None,
        };
        if let Some(key) = key {
            self.workspace.set_library_headword(id, Some(key));
        }
        self.sync_tab_title(id);
    }

    fn sync_study_bar(&self, id: TabId) {
        let Some(at) = self.workspace.tab(id).and_then(|tab| tab.kind.at()) else {
            return;
        };
        let follow = self
            .workspace
            .tab(id)
            .is_some_and(|tab| tab.kind.follows_verse());
        let chapters = self
            .conn
            .as_ref()
            .and_then(|conn| bible_app_db::max_chapter(conn, at.book).ok())
            .unwrap_or(1);
        let passage_hist = if follow {
            self.workspace
                .focused_passage_in(self.window_of(id))
                .and_then(|passage_id| self.passage(passage_id))
                .map(|passage| (passage.history.can_back(), passage.history.can_forward()))
        } else {
            None
        };
        let Some(hosted) = self.hosted.get(&id) else {
            return;
        };
        let (book, chapter, back, forward, syncing, history) = match &hosted.content {
            TabContent::Mhc(widgets) => (
                &widgets.book,
                &widgets.chapter,
                &widgets.back,
                &widgets.forward,
                &widgets.syncing,
                &widgets.history,
            ),
            TabContent::Tsk(widgets) => (
                &widgets.book,
                &widgets.chapter,
                &widgets.back,
                &widgets.forward,
                &widgets.syncing,
                &widgets.history,
            ),
            _ => return,
        };
        syncing.set(true);
        picker::select_book(book, &self.books, at.book);
        picker::sync_chapters(chapter, chapters, at.chapter);
        syncing.set(false);
        let (can_back, can_forward) = passage_hist.unwrap_or_else(|| {
            let history = history.borrow();
            (history.can_back(), history.can_forward())
        });
        back.set_sensitive(can_back);
        forward.set_sensitive(can_forward);
    }
}

enum AltStep {
    Prev,
    Next,
    Back,
    Forward,
}

enum PlaceMemory {
    Restart,
    Retarget,
    Navigate,
    Keep,
}

fn study_history_move(content: Option<&TabContent>, forward: bool) -> Option<Ref> {
    let history = match content {
        Some(TabContent::Mhc(widgets)) => &widgets.history,
        Some(TabContent::Tsk(widgets)) => &widgets.history,
        _ => return None,
    };
    let mut history = history.borrow_mut();
    if forward {
        history.forward()
    } else {
        history.back()
    }
}

fn remember_study(
    history: &RefCell<crate::history::History<Ref>>,
    placed: &Cell<bool>,
    at: Ref,
    memory: PlaceMemory,
) {
    let mut history = history.borrow_mut();
    match memory {
        PlaceMemory::Restart => {
            history.restart(at);
            placed.set(true);
        }
        PlaceMemory::Retarget => history.retarget(at),
        PlaceMemory::Navigate => {
            if placed.get() {
                history.navigate(at);
            } else {
                history.restart(at);
                placed.set(true);
            }
        }
        PlaceMemory::Keep => {}
    }
}

fn loaded_column_width() -> i32 {
    let width = config::load_state().column_width;
    if width > 0 {
        layout::clamp_column_px(width)
    } else {
        0
    }
}

type LoadedLibrary = (Connection, PathBuf, Vec<Book>, Ref, i32, bool, MatchMode);

fn strongs_counts(conn: &Connection, defs: &[bible_app_db::StrongDef]) -> Vec<usize> {
    defs.iter()
        .map(|d| {
            let code = format!("{}{}", d.lang, d.num);
            bible_app_db::strongs_occurrence_count(conn, &code).unwrap_or(0)
        })
        .collect()
}

fn load_library() -> Result<LoadedLibrary, String> {
    let path = config::locate_database()?;
    let conn = bible_app_db::open(&path)
        .map_err(|e| format!("Could not open {}:\n{e}", path.display()))?;
    let books = bible_app_db::books(&conn).map_err(|e| e.to_string())?;
    if books.is_empty() {
        return Err("The database has no books.".into());
    }
    let state = config::load_state();
    let font_size = state.font_size.clamp(layout::MIN_FONT, layout::MAX_FONT);
    let paragraphs = state.paragraphs;
    let search_mode = MatchMode::from_index(state.search_mode);
    let mut at = Ref::from(state);
    if bible_app_db::chapter(&conn, at.book, at.chapter)
        .map(|v| v.is_empty())
        .unwrap_or(true)
    {
        at = Ref {
            book: 1,
            chapter: 1,
            verse: 1,
        };
    }
    Ok((conn, path, books, at, font_size, paragraphs, search_mode))
}

fn build_app_menu(modules: &[DictModule]) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Paragraphs"), Some("win.paragraphs"));

    let marks = gio::Menu::new();
    marks.append(Some("Bookmarks"), Some("win.bookmarks"));
    marks.append(Some("Notes"), Some("win.notes"));
    menu.append_section(None, &marks);

    let dictionaries = gio::Menu::new();
    let lexicons = gio::Menu::new();
    let topics = gio::Menu::new();
    for module in modules {
        let action = format!(
            "win.open-dict('{}')",
            module.id.replace('\\', "\\\\").replace('\'', "\\'")
        );
        if module.kind == "dictionary" {
            dictionaries.append(Some(module.title.as_str()), Some(action.as_str()));
        } else if module.kind == "lexicon" {
            lexicons.append(Some(module.title.as_str()), Some(action.as_str()));
        } else {
            topics.append(Some(module.title.as_str()), Some(action.as_str()));
        }
    }
    let study = gio::Menu::new();
    study.append(Some("Matthew Henry"), Some("win.mhc"));
    study.append(Some("Treasury of Scripture Knowledge"), Some("win.tsk"));
    if lexicons.n_items() > 0 {
        study.append_submenu(Some("Lexicon"), &lexicons);
    }
    if dictionaries.n_items() > 0 {
        study.append_submenu(Some("Dictionary"), &dictionaries);
    }
    if topics.n_items() > 0 {
        study.append_submenu(Some("Topics"), &topics);
    }
    menu.append_section(None, &study);
    menu
}

#[derive(Clone)]
struct WireCtx {
    sender: relm4::Sender<Msg>,
    sides: Rc<RefCell<Vec<SideWindow>>>,
    counter: Rc<Cell<u64>>,
    actions: gio::SimpleActionGroup,
    menu: gio::Menu,
    app_menu: gio::Menu,
}

fn wire_host(host: &shell::PaneHost, window: WindowId, ctx: &WireCtx) {
    let view = &host.view;
    view.set_menu_model(Some(&ctx.menu));
    let send = ctx.sender.clone();
    view.connect_setup_menu(move |_, page| {
        let id = page.and_then(|page| page.keyword().as_deref().and_then(TabId::from_keyword));
        send.emit(Msg::SetupTabMenu(id));
    });
    let send = ctx.sender.clone();
    view.connect_selected_page_notify(move |view| {
        if let Some(page) = view.selected_page() {
            if let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) {
                send.emit(Msg::TabSelected(id));
            }
        }
    });
    view.connect_close_page(|view, page| {
        // Pull an embedded view out before the page is destroyed, then confirm
        // the close. Returning Proceed does not run the default handler, so
        // the page would stay on the bar with its closing flag stuck.
        rescue_split_guest(page);
        view.close_page_finish(page, true);
        glib::Propagation::Stop
    });
    let send = ctx.sender.clone();
    view.connect_page_detached(move |view, page, _| {
        if view.is_transferring_page() {
            return;
        }
        if let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) {
            send.emit(Msg::TabClosed(id));
        }
    });
    let send = ctx.sender.clone();
    view.connect_page_attached(move |_view, page, _| {
        let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) else {
            return;
        };
        send.emit(Msg::TabAttached { id, window });
    });
    let send = ctx.sender.clone();
    host.new_btn.connect_clicked(move |_| {
        send.emit(Msg::NewTab(window));
    });
    let send = ctx.sender.clone();
    view.connect_indicator_activated(move |_view, page| {
        if let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) {
            send.emit(Msg::SplitTab(id));
        }
    });
    let ctx = ctx.clone();
    view.connect_create_window(move |_| Some(open_drag_window(&ctx)));
}

fn unparent(widget: &gtk::Widget) {
    let Some(parent) = widget.parent() else {
        return;
    };
    // Paned and Overlay keep their own child pointer. Unparenting the widget
    // directly leaves that pointer set, and destroying the container later
    // frees it again.
    if let Some(overlay) = parent.downcast_ref::<gtk::Overlay>() {
        if overlay.child().as_ref() == Some(widget) {
            overlay.set_child(None::<&gtk::Widget>);
            return;
        }
    }
    if let Some(paned) = parent.downcast_ref::<gtk::Paned>() {
        if paned.start_child().as_ref() == Some(widget) {
            paned.set_start_child(None::<&gtk::Widget>);
            return;
        }
        if paned.end_child().as_ref() == Some(widget) {
            paned.set_end_child(None::<&gtk::Widget>);
            return;
        }
    }
    widget.unparent();
}

fn guest_frame(id: TabId, body: &gtk::Widget, tx: &relm4::Sender<Msg>) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    overlay.set_child(Some(body));
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("flat");
    close.add_css_class("split-close");
    close.set_tooltip_text(Some("Close"));
    close.set_halign(gtk::Align::End);
    close.set_valign(gtk::Align::Start);
    close.set_margin_top(8);
    close.set_margin_end(8);
    close.update_property(&[gtk::accessible::Property::Label("Close")]);
    let tx = tx.clone();
    close.connect_clicked(move |_| tx.emit(Msg::CloseGuest(id)));
    overlay.add_overlay(&close);
    overlay
}

/// Give a widget the size its parent already has.
///
/// The page slot is allocated before its child is swapped. GTK does not
/// allocate that new child, so a split stays 0×0 and a closed split keeps
/// the half-width chapter. Queueing an allocate does not reach the window.
fn allocate_to_parent(child: &impl IsA<gtk::Widget>) {
    let child = child.as_ref();
    let Some(parent) = child.parent() else {
        return;
    };
    let width = parent.width();
    let height = parent.height();
    if width > 1 && height > 1 {
        child.allocate(width, height, -1, None);
    }
}

fn give_paned_parent_size(paned: &gtk::Paned) -> i32 {
    if paned.width() <= 1 {
        allocate_to_parent(paned);
    }
    paned.width()
}

fn center_split(paned: &gtk::Paned) {
    // Both children resizing keeps the first position as a ratio of the pane.
    // A chapter's minimum is wide, so that ratio pins the handle to the right
    // edge. Hold an absolute position until the end side is actually showing.
    paned.set_resize_start_child(false);
    paned.set_resize_end_child(true);
    paned.set_position(480);
    let waits = Rc::new(Cell::new(0u8));
    let tries = Rc::new(Cell::new(0u8));
    paned.add_tick_callback(move |paned, _| {
        if paned.parent().is_none() {
            return glib::ControlFlow::Break;
        }
        let width = give_paned_parent_size(paned);
        if width <= 1 {
            if waits.get() >= 90 {
                return glib::ControlFlow::Break;
            }
            waits.set(waits.get().saturating_add(1));
            return glib::ControlFlow::Continue;
        }
        let target = width / 2;
        let end_w = paned.end_child().map(|child| child.width()).unwrap_or(0);
        if (paned.position() - target).abs() <= 16 && end_w >= width / 5 {
            paned.set_resize_start_child(true);
            paned.set_resize_end_child(true);
            return glib::ControlFlow::Break;
        }
        if tries.get() >= 30 {
            return glib::ControlFlow::Break;
        }
        tries.set(tries.get().saturating_add(1));
        paned.set_position(target);
        // set_position only queues an allocate, and that queue does not run.
        // Pass the slot's size, not the paned's content size: allocate()
        // takes the outside size and content size is smaller once CSS is applied.
        if let Some(parent) = paned.parent() {
            let parent_width = parent.width();
            let parent_height = parent.height();
            if parent_width > 1 && parent_height > 1 {
                paned.allocate(parent_width, parent_height, -1, None);
            }
        }
        glib::ControlFlow::Continue
    });
}

/// Keep the right-hand view alive when its tab is closed. The page owns the
/// paned, so the guest has to leave that paned before the page is destroyed.
fn rescue_split_guest(page: &adw::TabPage) {
    let Some(slot) = page.child().downcast::<gtk::Box>().ok() else {
        return;
    };
    let Some(paned) = slot
        .first_child()
        .and_then(|child| child.downcast::<gtk::Paned>().ok())
    else {
        return;
    };
    if !paned.has_css_class("pane-split") {
        return;
    }
    let Some(end) = paned.end_child() else {
        return;
    };
    if let Some(overlay) = end.downcast_ref::<gtk::Overlay>() {
        if let Some(child) = overlay.child() {
            unparent(&child);
        }
    }
}

fn open_drag_window(ctx: &WireCtx) -> adw::TabView {
    let id = WindowId::from_raw(ctx.counter.get());
    ctx.counter.set(id.raw() + 1);
    let chrome = shell::open_side_window(&ctx.app_menu);
    chrome.window.insert_action_group("win", Some(&ctx.actions));
    wire_host(&chrome.shell.host, id, ctx);
    let view = chrome.shell.host.view.clone();
    ctx.sides.borrow_mut().push(SideWindow { id, chrome });
    ctx.sender.emit(Msg::WireSide(id));
    view
}

relm4::new_action_group!(WindowActionGroup, "win");
relm4::new_stateful_action!(ParagraphsAction, WindowActionGroup, "paragraphs", (), bool);
relm4::new_stateless_action!(CopyVerseAction, WindowActionGroup, "copy-verse");
relm4::new_stateless_action!(FontLargerAction, WindowActionGroup, "font-larger");
relm4::new_stateless_action!(FontSmallerAction, WindowActionGroup, "font-smaller");
relm4::new_stateless_action!(MhcAction, WindowActionGroup, "mhc");
relm4::new_stateless_action!(TskAction, WindowActionGroup, "tsk");
relm4::new_stateless_action!(BookmarksAction, WindowActionGroup, "bookmarks");
relm4::new_stateless_action!(NotesAction, WindowActionGroup, "notes");
relm4::new_stateless_action!(ExportNotesAction, WindowActionGroup, "export-notes");
relm4::new_stateless_action!(ToggleBookmarkAction, WindowActionGroup, "toggle-bookmark");
relm4::new_stateless_action!(AddNoteAction, WindowActionGroup, "add-note");
relm4::new_stateless_action!(DetachTabAction, WindowActionGroup, "tab-detach");
relm4::new_stateless_action!(BesideTabAction, WindowActionGroup, "tab-open-beside");
relm4::new_stateful_action!(FollowTabAction, WindowActionGroup, "tab-follow", (), bool);
