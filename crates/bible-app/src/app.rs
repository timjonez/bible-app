use crate::config;
use crate::dict;
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
use crate::workspace::{MarksPage, Pane, SplitOutcome, TabId, TabKind, WindowId, Workspace};
use adw::prelude::*;
use bible_app_db::{self, Book, DictModule, LibraryKind, MatchMode, SearchScope};
use gtk::gio;
use gtk::glib;
use relm4::actions::{RelmAction, RelmActionGroup};
use relm4::prelude::*;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

struct HostedTab {
    page: adw::TabPage,
    content: TabContent,
}

enum TabContent {
    Passage(Box<PassageView>),
    Mhc(mhc::MhcWidgets),
    Tsk(tsk::TskWidgets),
    Library(dict::DictWidgets),
    Marks(marks::MarksWidgets),
    Occurrences(occurrences::OccWidgets),
    Search(search::Pane),
}

struct SideWindow {
    id: WindowId,
    chrome: SideChrome,
    syncing: Rc<Cell<bool>>,
}

pub struct App {
    conn: Option<Connection>,
    books: Vec<Book>,
    error: Option<String>,
    book_dropdown: gtk::DropDown,
    chapter_dropdown: gtk::DropDown,
    picker_syncing: Rc<Cell<bool>>,
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
    mhc_action: gio::SimpleAction,
    tsk_action: gio::SimpleAction,
    follow_action: gio::SimpleAction,
    user: Option<Connection>,
    workspace: Workspace,
    hosted: HashMap<TabId, HostedTab>,
    shell: Option<SplitShell>,
    sides: Rc<RefCell<Vec<SideWindow>>>,
    menu_tab: Rc<Cell<Option<TabId>>>,
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
    SelectBookIndex(WindowId, u32),
    SelectChapterIndex(WindowId, u32),
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
    ToggleMhc,
    ToggleTsk,
    OpenTskXref(i32),
    OpenTskDest(Ref),
    OpenTskDestBeside(Ref),
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
    DictSearch(String),
    DictOpen(i32),
    OpenStrongsOccurrences(String),
    OpenOccurrenceHit(i32),
    Back(WindowId),
    Forward(WindowId),
    CopyVerses,
    CopyAtOffset(i32),
    FontSmaller,
    FontLarger,
    SetColumnWidth(i32),
    PersistColumnWidth,
    SetParagraphs(bool),
    OpenBookmarks,
    OpenNotes,
    MarksBookmarkActivated(i32),
    MarksBookmarkSelected,
    MarksNoteActivated(i32),
    MarksNoteSelected(i32),
    SaveNote,
    DeleteEditingNote,
    RemoveSelectedBookmark,
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
    TabAttached {
        id: TabId,
        window: WindowId,
        pane: Pane,
    },
    SplitTab(TabId),
    DetachTab(TabId),
    WireSide(WindowId),
    SetupTabMenu(Option<TabId>),
    DetachMenuTab,
    BesideMenuTab,
    SetTabFollow(bool),
    ThemeChanged,
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
                add_top_bar = &adw::HeaderBar {
                    #[wrap(Some)]
                    #[name(title_box)]
                    set_title_widget = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 6,
                        set_valign: gtk::Align::Center,
                        set_halign: gtk::Align::Center,
                        add_css_class: "passage-title",

                        #[local_ref]
                        book_dropdown -> gtk::DropDown {
                            set_enable_search: true,
                            set_search_match_mode: gtk::StringFilterMatchMode::Substring,
                            set_tooltip_text: Some("Book"),
                            set_valign: gtk::Align::Center,
                            add_css_class: "passage-picker",
                            #[watch]
                            set_sensitive: model.error.is_none(),
                        },

                        #[local_ref]
                        chapter_dropdown -> gtk::DropDown {
                            set_enable_search: true,
                            set_search_match_mode: gtk::StringFilterMatchMode::Prefix,
                            set_tooltip_text: Some("Chapter"),
                            set_valign: gtk::Align::Center,
                            add_css_class: "chapter-picker",
                            #[watch]
                            set_sensitive: model.error.is_none(),
                        },
                    },
                    pack_start = &gtk::Box {
                        set_spacing: 6,
                        set_valign: gtk::Align::Center,

                        gtk::Box {
                            add_css_class: "linked",

                            gtk::Button {
                                set_icon_name: "go-previous-symbolic",
                                set_tooltip_text: Some("Previous chapter (Alt+Left)"),
                                set_valign: gtk::Align::Center,
                                connect_clicked => Msg::PrevChapter(WindowId::MAIN),
                            },
                            gtk::Button {
                                set_icon_name: "go-next-symbolic",
                                set_tooltip_text: Some("Next chapter (Alt+Right)"),
                                set_valign: gtk::Align::Center,
                                connect_clicked => Msg::NextChapter(WindowId::MAIN),
                            },
                        },
                        gtk::Box {
                            add_css_class: "linked",
                            #[watch]
                            set_visible: model.has_history_in(WindowId::MAIN),

                            gtk::Button {
                                set_icon_name: "edit-undo-symbolic",
                                set_tooltip_text: Some("Back in history (Alt+Shift+Left)"),
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: model.can_back_in(WindowId::MAIN),
                                connect_clicked => Msg::Back(WindowId::MAIN),
                            },
                            gtk::Button {
                                set_icon_name: "edit-redo-symbolic",
                                set_tooltip_text: Some("Forward in history (Alt+Shift+Right)"),
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: model.can_forward_in(WindowId::MAIN),
                                connect_clicked => Msg::Forward(WindowId::MAIN),
                            },
                        },
                    },
                    pack_end = &gtk::MenuButton {
                        set_icon_name: "open-menu-symbolic",
                        set_tooltip_text: Some("Menu"),
                        set_primary: true,
                        set_valign: gtk::Align::Center,
                        add_css_class: "primary-menu",
                        set_menu_model: Some(&build_app_menu(&model.dict_modules)),
                    },
                    pack_end = &gtk::ToggleButton {
                        set_icon_name: "edit-find-symbolic",
                        set_tooltip_text: Some("Search (Ctrl+F)"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_active: model.workspace.has_search(WindowId::MAIN),
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::SetSearch(WindowId::MAIN, btn.is_active()));
                        }
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
        let book_dropdown = gtk::DropDown::from_strings(&[]);
        let chapter_dropdown = gtk::DropDown::from_strings(&[]);
        picker::prepare(&book_dropdown, gtk::StringFilterMatchMode::Substring);
        picker::prepare(&chapter_dropdown, gtk::StringFilterMatchMode::Prefix);
        book_dropdown.update_property(&[gtk::accessible::Property::Label("Book")]);
        chapter_dropdown.update_property(&[gtk::accessible::Property::Label("Chapter")]);
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
            RelmAction::new_stateful(&false, move |_, _state: &mut bool| {
                sender.input(Msg::ToggleMhc);
            })
        };
        let tsk_action: RelmAction<TskAction> = {
            let sender = sender.clone();
            RelmAction::new_stateful(&false, move |_, _state: &mut bool| {
                sender.input(Msg::ToggleTsk);
            })
        };
        let mhc_gio = mhc_action.gio_action().clone();
        let tsk_gio = tsk_action.gio_action().clone();
        if error.is_some() {
            mhc_gio.set_enabled(false);
            tsk_gio.set_enabled(false);
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
            book_dropdown: book_dropdown.clone(),
            chapter_dropdown: chapter_dropdown.clone(),
            picker_syncing: Rc::new(Cell::new(false)),
            search_mode,
            search_mark: None,
            search_db_path,
            dict_modules,
            font_size,
            column_width: loaded_column_width(),
            paragraphs,
            font_provider,
            _theme_watch,
            mhc_action: mhc_gio,
            tsk_action: tsk_gio,
            follow_action: follow_gio,
            user,
            workspace: Workspace::new(at),
            hosted: HashMap::new(),
            shell: None,
            sides: Rc::new(RefCell::new(Vec::new())),
            menu_tab: Rc::new(Cell::new(None)),
            goto_entry: goto_entry.clone(),
            goto_popover: goto_popover.clone(),
            actions: gio::SimpleActionGroup::new(),
            msg_tx: sender.input_sender().clone(),
            copy_offset,
        };
        model.apply_font();
        picker::install_css();
        passage::install_css();
        picker::fill_books(&model.book_dropdown, &model.books);

        let widgets = view_output!();
        let action_group = group.into_action_group();
        action_group.add_action(&dict_action);
        action_group.add_action(&highlight_action);
        action_group.add_action(&copy_here);
        root.insert_action_group("win", Some(&action_group));
        model.actions = action_group;
        model.goto_popover.set_parent(&widgets.title_box);
        let goto_on_destroy = model.goto_popover.clone();
        root.connect_destroy(move |_| {
            goto_on_destroy.unparent();
        });

        if model.error.is_none() {
            let shell = SplitShell::new();
            let ctx = model.wire_ctx();
            wire_host(&shell.left, WindowId::MAIN, Pane::Left, &ctx);
            wire_host(&shell.right, WindowId::MAIN, Pane::Right, &ctx);
            workspace_host.append(&shell.paned);
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

        let book_sender = sender.clone();
        let book_syncing = model.picker_syncing.clone();
        model.book_dropdown.connect_selected_notify(move |dd| {
            if book_syncing.get() {
                return;
            }
            let pos = dd.selected();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }
            book_sender.input(Msg::SelectBookIndex(WindowId::MAIN, pos));
        });
        let chapter_sender = sender.clone();
        let chapter_syncing = model.picker_syncing.clone();
        model.chapter_dropdown.connect_selected_notify(move |dd| {
            if chapter_syncing.get() {
                return;
            }
            let pos = dd.selected();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }
            chapter_sender.input(Msg::SelectChapterIndex(WindowId::MAIN, pos));
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::SelectBookIndex(window, idx) => {
                let Some(id) = picker::book_id_at(&self.books, idx) else {
                    return;
                };
                if self.at_in(window).book == id {
                    return;
                }
                self.go_in(
                    window,
                    Ref {
                        book: id,
                        chapter: 1,
                        verse: 1,
                    },
                    false,
                );
            }
            Msg::SelectChapterIndex(window, idx) => {
                let Some(chapter) = picker::chapter_from_index(idx) else {
                    return;
                };
                let at = self.at_in(window);
                if at.chapter == chapter {
                    return;
                }
                self.go_in(
                    window,
                    Ref {
                        book: at.book,
                        chapter,
                        verse: 1,
                    },
                    false,
                );
            }
            Msg::PrevChapter(window) => {
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::prev_chapter(conn, &self.books, self.at_in(window)) {
                        self.go_in(window, at, false);
                    }
                }
            }
            Msg::NextChapter(window) => {
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::next_chapter(conn, &self.books, self.at_in(window)) {
                        self.go_in(window, at, false);
                    }
                }
            }
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
            Msg::ToggleMhc => {
                if let Some(id) = self.workspace.find_kind(|k| k.is_mhc()) {
                    self.request_close(id);
                } else {
                    self.ensure_mhc();
                }
            }
            Msg::ToggleTsk => {
                if let Some(id) = self.workspace.find_kind(|k| k.is_tsk()) {
                    self.request_close(id);
                } else {
                    self.ensure_tsk();
                }
            }
            Msg::OpenTskXref(idx) => {
                let Some(at) = self.tsk_widgets().and_then(|w| tsk::xref_at(w, idx)) else {
                    return;
                };
                self.go(at, true);
            }
            Msg::OpenTskDest(at) => {
                self.popdown_passage_popovers();
                self.go(at, true);
            }
            Msg::OpenTskDestBeside(at) => {
                self.popdown_passage_popovers();
                self.go_beside(at);
            }
            Msg::OpenStrongsCode(code) => {
                self.open_strongs_code(&code);
            }
            Msg::ClickWord { id, offset } => {
                self.focus_tab(id);
                self.handle_click(id, offset);
            }
            Msg::OpenDict(id) => {
                self.open_library(&id, None);
            }
            Msg::OpenDictWord { module, headword } => {
                self.popdown_passage_popovers();
                self.open_library(&module, Some(&headword));
            }
            Msg::DictSearch(query) => {
                let Some(id) = self
                    .workspace
                    .find_kind(|k| matches!(k, TabKind::Library { .. }))
                else {
                    return;
                };
                if let Some(TabContent::Library(widgets)) =
                    self.hosted.get_mut(&id).map(|h| &mut h.content)
                {
                    widgets.query = query;
                    if let Some(conn) = &self.conn {
                        dict::search(widgets, conn);
                    }
                }
            }
            Msg::DictOpen(idx) => {
                let Some(conn) = &self.conn else { return };
                if let Some(TabContent::Library(widgets)) = self.library() {
                    dict::open_hit(widgets, conn, idx);
                }
                self.sync_open_tab_title();
            }
            Msg::OpenStrongsOccurrences(code) => {
                self.popdown_passage_popovers();
                self.open_occurrences(&code);
            }
            Msg::OpenOccurrenceHit(idx) => {
                let Some(at) = self.occ_widgets().and_then(|w| occurrences::hit_at(w, idx)) else {
                    return;
                };
                self.go(at, true);
            }
            Msg::Back(window) => {
                let Some(id) = self.workspace.focused_passage_in(window) else {
                    return;
                };
                let Some(at) = self.passage_mut(id).and_then(|p| p.history.back()) else {
                    return;
                };
                self.apply_passage_ref(id, at, true, false);
            }
            Msg::Forward(window) => {
                let Some(id) = self.workspace.focused_passage_in(window) else {
                    return;
                };
                let Some(at) = self.passage_mut(id).and_then(|p| p.history.forward()) else {
                    return;
                };
                self.apply_passage_ref(id, at, true, false);
            }
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
            Msg::OpenBookmarks => self.ensure_marks("bookmarks"),
            Msg::OpenNotes => self.ensure_marks("notes"),
            Msg::MarksBookmarkActivated(idx) => {
                if let Some(at) = self
                    .marks_widgets()
                    .and_then(|w| marks::bookmark_at(w, idx))
                {
                    self.go(at, true);
                }
            }
            Msg::MarksBookmarkSelected => {
                if let Some(w) = self.marks_widgets() {
                    w.remove_bookmark
                        .set_sensitive(w.bookmark_list.selected_row().is_some());
                }
            }
            Msg::MarksNoteActivated(idx) => {
                if let Some(at) = self.marks_widgets().and_then(|w| marks::note_at(w, idx)) {
                    self.go(at, true);
                }
            }
            Msg::MarksNoteSelected(idx) => {
                if self.marks_widgets().is_some_and(|w| w.syncing.get()) {
                    return;
                }
                let Some(at) = self.marks_widgets().and_then(|w| marks::note_at(w, idx)) else {
                    return;
                };
                let text = self
                    .user
                    .as_ref()
                    .and_then(|u| user_db::get_note(u, at).ok().flatten())
                    .unwrap_or_default();
                if let Some(id) = self
                    .workspace
                    .find_kind(|k| matches!(k, TabKind::Marks { .. }))
                {
                    if let Some(TabContent::Marks(w)) =
                        self.hosted.get_mut(&id).map(|h| &mut h.content)
                    {
                        marks::load_note(w, &self.books, at, &text);
                    }
                }
            }
            Msg::SaveNote => {
                let Some(TabContent::Marks(widgets)) = self.marks() else {
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
                if let Some(TabContent::Marks(w)) = self.marks() {
                    w.delete_note.set_sensitive(now_present);
                }
            }
            Msg::DeleteEditingNote => {
                let Some(at) = self.marks_widgets().and_then(|w| w.editing) else {
                    return;
                };
                if let Some(user) = &self.user {
                    let _ = user_db::delete_note(user, at);
                }
                if let Some(TabContent::Marks(w)) = self.marks_mut() {
                    marks::clear_editor(w);
                }
                self.reload_user_marks(true);
            }
            Msg::RemoveSelectedBookmark => {
                let Some(at) = self.marks_widgets().and_then(marks::selected_bookmark) else {
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
            }
            Msg::TabClosed(id) => {
                self.forget_tab(id);
            }
            Msg::TabAttached { id, window, pane } => {
                self.workspace.place(id, window, pane);
                self.collapse_empty(window);
                self.sync_shells();
            }
            Msg::SplitTab(id) => self.split_tab(id),
            Msg::DetachTab(id) => self.detach_tab(id),
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
                        self.refresh_followers();
                    }
                }
            }
            Msg::ThemeChanged => self.recolor_passages(),
        }
        self.sync_study_actions();
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

    fn can_back_in(&self, window: WindowId) -> bool {
        self.workspace
            .focused_passage_in(window)
            .and_then(|id| self.passage(id))
            .is_some_and(|p| p.history.can_back())
    }

    fn can_forward_in(&self, window: WindowId) -> bool {
        self.workspace
            .focused_passage_in(window)
            .and_then(|id| self.passage(id))
            .is_some_and(|p| p.history.can_forward())
    }

    fn has_history_in(&self, window: WindowId) -> bool {
        self.can_back_in(window) || self.can_forward_in(window)
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

    fn library(&self) -> Option<&TabContent> {
        let id = self
            .workspace
            .find_kind(|k| matches!(k, TabKind::Library { .. }))?;
        self.hosted.get(&id).map(|h| &h.content)
    }

    fn marks(&self) -> Option<&TabContent> {
        let id = self
            .workspace
            .find_kind(|k| matches!(k, TabKind::Marks { .. }))?;
        self.hosted.get(&id).map(|h| &h.content)
    }

    fn marks_mut(&mut self) -> Option<&mut TabContent> {
        let id = self
            .workspace
            .find_kind(|k| matches!(k, TabKind::Marks { .. }))?;
        self.hosted.get_mut(&id).map(|h| &mut h.content)
    }

    fn marks_widgets(&self) -> Option<&marks::MarksWidgets> {
        match self.marks() {
            Some(TabContent::Marks(w)) => Some(w),
            _ => None,
        }
    }

    fn tsk_widgets(&self) -> Option<&tsk::TskWidgets> {
        let id = self.workspace.find_kind(|k| k.is_tsk())?;
        match self.hosted.get(&id).map(|h| &h.content) {
            Some(TabContent::Tsk(w)) => Some(w),
            _ => None,
        }
    }

    fn occ_widgets(&self) -> Option<&occurrences::OccWidgets> {
        let id = self
            .workspace
            .find_kind(|k| matches!(k, TabKind::Occurrences { .. }))?;
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
        let Some(id) = self.workspace.find_search(window) else {
            return;
        };
        if self.workspace.focused() == id {
            self.set_search(window, false);
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
        if self.workspace.has_search(window) == open {
            if open {
                if let Some(id) = self.workspace.find_search(window) {
                    self.select_tab(id);
                    self.focus_search(id);
                }
            }
            return;
        }
        if open {
            self.open_search_tab(window);
        } else if let Some(id) = self.workspace.find_search(window) {
            self.request_close(id);
        }
    }

    fn open_search_tab(&mut self, window: WindowId) {
        let was_split = self.workspace.is_window_split(window);
        let at = self.at_in(window);
        let opened = self.workspace.open_search(window);
        if opened.created {
            let mut pane = search::build_pane(self.search_mode);
            pane.origin = Some(at);
            self.wire_search(opened.id, &pane);
            let root = pane.root.clone();
            self.add_page(opened.id, &root, "Search", TabContent::Search(pane));
            if !was_split {
                self.move_tab_beside(opened.id);
            }
        } else {
            self.select_tab(opened.id);
        }
        self.focus_search(opened.id);
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
                    self.ensure_tsk();
                } else {
                    self.ensure_mhc();
                }
            }
            LibraryKind::Dictionary | LibraryKind::Topic | LibraryKind::Lexicon => {
                let Some(headword) = hit.headword.as_deref() else {
                    return;
                };
                self.open_library(&hit.module, Some(headword));
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

    fn sync_study_actions(&self) {
        self.mhc_action
            .set_state(&self.workspace.has_mhc().to_variant());
        self.tsk_action
            .set_state(&self.workspace.has_tsk().to_variant());
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

    /// Column edges belong to a chapter that fills its window by itself.
    fn column_resize_for(&self, id: TabId) -> bool {
        let Some(tab) = self.workspace.tab(id) else {
            return true;
        };
        tab.kind.is_passage() && !self.workspace.is_window_split(tab.window)
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
            if let TabContent::Passage(passage) = &hosted.content {
                theme::paint_buffer(&passage.buffer);
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
            self.ensure_mhc();
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

    fn ensure_mhc(&mut self) {
        if self.error.is_some() {
            return;
        }
        let at = self.at();
        let opened = self.workspace.open_mhc(at);
        if opened.created {
            let widgets = mhc::build();
            if let Some(conn) = &self.conn {
                mhc::fill(&widgets, conn, &self.books, at);
            }
            let root = widgets.root.clone();
            self.add_page(opened.id, &root, "Matthew Henry", TabContent::Mhc(widgets));
            self.move_tab_beside(opened.id);
        } else {
            self.select_tab(opened.id);
            self.fill_mhc(opened.id, at);
        }
        self.sync_tab_title(opened.id);
    }

    fn ensure_tsk(&mut self) {
        if self.error.is_some() {
            return;
        }
        let at = self.at();
        let opened = self.workspace.open_tsk(at);
        if opened.created {
            let mut widgets = tsk::build(self.msg_tx.clone());
            if let Some(conn) = &self.conn {
                tsk::fill(&mut widgets, conn, &self.books, at, self.msg_tx.clone());
            }
            let root = widgets.root.clone();
            self.add_page(opened.id, &root, "TSK", TabContent::Tsk(widgets));
            self.move_tab_beside(opened.id);
        } else {
            self.select_tab(opened.id);
            self.fill_tsk(opened.id, at);
        }
        self.sync_tab_title(opened.id);
    }

    fn open_library(&mut self, module: &str, headword: Option<&str>) {
        if self.error.is_some() {
            return;
        }
        let opened = self
            .workspace
            .open_library(module.to_string(), headword.map(str::to_string));
        if opened.created {
            let mut widgets = dict::build(self.msg_tx.clone());
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
            self.move_tab_beside(opened.id);
        } else if let Some(TabContent::Library(widgets)) =
            self.hosted.get_mut(&opened.id).map(|h| &mut h.content)
        {
            if let (Some(conn), Some(head)) = (&self.conn, headword) {
                dict::open_headword(widgets, conn, module, head);
            } else {
                dict::select_module(widgets, module);
            }
            let title = dict::tab_title(widgets);
            if let Some(h) = self.hosted.get(&opened.id) {
                h.page.set_title(&title);
            }
            self.select_tab(opened.id);
        }
    }

    fn open_occurrences(&mut self, code: &str) {
        if self.error.is_some() {
            return;
        }
        let opened = self.workspace.open_occurrences(code.to_string());
        if opened.created {
            let mut widgets = occurrences::build(self.msg_tx.clone());
            if let Some(conn) = &self.conn {
                occurrences::fill(&mut widgets, conn, &self.books, code);
            }
            let title = format!("{code} in the KJV");
            let root = widgets.root.clone();
            self.add_page(opened.id, &root, &title, TabContent::Occurrences(widgets));
            self.move_tab_beside(opened.id);
        } else if let Some(TabContent::Occurrences(widgets)) =
            self.hosted.get_mut(&opened.id).map(|h| &mut h.content)
        {
            if let Some(conn) = &self.conn {
                occurrences::fill(widgets, conn, &self.books, code);
            }
            if let Some(h) = self.hosted.get(&opened.id) {
                h.page.set_title(&format!("{code} in the KJV"));
            }
            self.select_tab(opened.id);
        }
    }

    fn ensure_marks(&mut self, page: &str) {
        if self.error.is_some() || self.user.is_none() {
            return;
        }
        let opened = self.workspace.open_marks(MarksPage::from_str(page));
        if opened.created {
            let mut widgets = marks::build(self.msg_tx.clone());
            if let Some(user) = &self.user {
                marks::fill(&mut widgets, user, &self.books);
            }
            marks::show_page(&widgets, page);
            let title = MarksPage::from_str(page).as_str();
            let title = if title == "notes" {
                "Notes"
            } else {
                "Bookmarks"
            };
            let root = widgets.root.clone();
            self.add_page(opened.id, &root, title, TabContent::Marks(widgets));
            self.move_tab_beside(opened.id);
        } else if let Some(TabContent::Marks(widgets)) =
            self.hosted.get_mut(&opened.id).map(|h| &mut h.content)
        {
            marks::show_page(widgets, page);
            if let Some(user) = &self.user {
                marks::fill(widgets, user, &self.books);
            }
            if let Some(h) = self.hosted.get(&opened.id) {
                h.page.set_title(if page == "notes" {
                    "Notes"
                } else {
                    "Bookmarks"
                });
            }
            self.select_tab(opened.id);
        }
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

    fn fill_mhc(&mut self, id: TabId, at: Ref) {
        let Some(conn) = &self.conn else { return };
        if let Some(TabContent::Mhc(w)) = self.hosted.get(&id).map(|h| &h.content) {
            mhc::fill(w, conn, &self.books, at);
        }
        if let Some(h) = self.hosted.get(&id) {
            h.page.set_tooltip(&nav::format_ref(&self.books, at));
        }
    }

    fn fill_tsk(&mut self, id: TabId, at: Ref) {
        let Some(conn) = &self.conn else { return };
        let tx = self.msg_tx.clone();
        if let Some(TabContent::Tsk(w)) = self.hosted.get_mut(&id).map(|h| &mut h.content) {
            tsk::fill(w, conn, &self.books, at, tx);
        }
        if let Some(h) = self.hosted.get(&id) {
            h.page.set_tooltip(&nav::format_ref(&self.books, at));
        }
    }

    fn refresh_followers(&mut self) {
        let following: Vec<(TabId, TabKind)> = self
            .workspace
            .tabs()
            .iter()
            .filter(|t| t.kind.follows_verse())
            .map(|t| (t.id, t.kind.clone()))
            .collect();
        for (id, kind) in following {
            match kind {
                TabKind::Mhc { at, .. } => self.fill_mhc(id, at),
                TabKind::Tsk { at, .. } => self.fill_tsk(id, at),
                _ => {}
            }
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
        let title = nav::format_chapter(&self.books, at.book, at.chapter);
        let root = p.root.clone();
        self.add_page(id, &root, &title, TabContent::Passage(Box::new(p)));
        self.reload_passage(id, false);
    }

    fn add_page(
        &mut self,
        id: TabId,
        child: &impl IsA<gtk::Widget>,
        title: &str,
        content: TabContent,
    ) {
        let (window, pane) = self
            .workspace
            .tab(id)
            .map(|tab| (tab.window, tab.pane))
            .unwrap_or((WindowId::MAIN, Pane::Left));
        let Some(view) = self.view_in(window, pane) else {
            return;
        };
        let page = view.append(child);
        page.set_keyword(&id.keyword());
        page.set_title(title);
        if let Some(at) = self.workspace.tab(id).and_then(|t| t.kind.at()) {
            if !self.workspace.tab(id).is_some_and(|t| t.kind.is_passage()) {
                page.set_tooltip(&nav::format_ref(&self.books, at));
            }
        }
        self.hosted.insert(id, HostedTab { page, content });
        if let Some(h) = self.hosted.get(&id) {
            view.set_selected_page(&h.page);
        }
        self.sync_shells();
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
            TabKind::Marks { page } => match page {
                MarksPage::Notes => "Notes".into(),
                MarksPage::Bookmarks => "Bookmarks".into(),
            },
            TabKind::Occurrences { code } => format!("{code} in the KJV"),
            TabKind::Search => "Search".into(),
        };
        hosted.page.set_title(&title);
        if let Some(at) = tab.kind.at() {
            if !tab.kind.is_passage() {
                hosted.page.set_tooltip(&nav::format_ref(&self.books, at));
            }
        }
    }

    fn sync_open_tab_title(&self) {
        if let Some(id) = self
            .workspace
            .find_kind(|k| matches!(k, TabKind::Library { .. }))
        {
            self.sync_tab_title(id);
        }
    }

    fn select_tab(&self, id: TabId) {
        let Some(hosted) = self.hosted.get(&id) else {
            return;
        };
        if let Some(view) = self.view_holding(id) {
            view.set_selected_page(&hosted.page);
            if let Some(win) = view.root().and_downcast::<gtk::Window>() {
                win.present();
            }
        }
    }

    fn focus_tab(&mut self, id: TabId) {
        self.workspace.focus(id);
        self.select_tab(id);
        self.sync_pickers();
    }

    fn request_close(&self, id: TabId) {
        let Some(hosted) = self.hosted.get(&id) else {
            return;
        };
        if let Some(view) = self.view_holding(id) {
            view.close_page(&hosted.page);
        }
    }

    fn forget_tab(&mut self, id: TabId) {
        let restore = self.search(id).and_then(|pane| pane.origin);
        let window = self.window_of(id);
        if self.hosted.remove(&id).is_none() {
            return;
        }
        let _ = self.workspace.close(id);
        if let Some(at) = restore {
            self.search_mark = None;
            if let Some(passage) = self.workspace.focused_passage_in(window) {
                self.apply_passage_ref(passage, at, true, false);
            }
        }
        self.collapse_empty(window);
        self.sync_shells();
        self.sync_pickers();
    }

    fn view_in(&self, window: WindowId, pane: Pane) -> Option<adw::TabView> {
        if window == WindowId::MAIN {
            return self
                .shell
                .as_ref()
                .map(|shell| shell.host(pane).view.clone());
        }
        self.sides
            .borrow()
            .iter()
            .find(|side| side.id == window)
            .map(|side| side.chrome.shell.host(pane).view.clone())
    }

    fn view_holding(&self, id: TabId) -> Option<adw::TabView> {
        let page = &self.hosted.get(&id)?.page;
        let mut views = Vec::new();
        if let Some(shell) = &self.shell {
            views.push(shell.left.view.clone());
            views.push(shell.right.view.clone());
        }
        for side in self.sides.borrow().iter() {
            views.push(side.chrome.shell.left.view.clone());
            views.push(side.chrome.shell.right.view.clone());
        }
        views
            .into_iter()
            .find(|view| (0..view.n_pages()).any(|i| view.nth_page(i) == *page))
    }

    fn sync_shells(&self) {
        if let Some(shell) = &self.shell {
            shell.sync_split();
            self.sync_split_buttons(shell);
        }
        for side in self.sides.borrow().iter() {
            side.chrome.shell.sync_split();
            self.sync_split_buttons(&side.chrome.shell);
        }
    }

    fn sync_split_buttons(&self, shell: &SplitShell) {
        for host in [&shell.left, &shell.right] {
            let selected = shell::selected_tab_id(&host.view);
            host.set_split_sensitive(selected.is_some_and(|id| self.workspace.can_split(id)));
            host.popout_btn.set_sensitive(selected.is_some());
        }
    }

    fn collapse_empty(&self, window: WindowId) {
        let Some(left) = self.view_in(window, Pane::Left) else {
            return;
        };
        let Some(right) = self.view_in(window, Pane::Right) else {
            return;
        };
        if left.n_pages() == 0 && right.n_pages() > 0 {
            while right.n_pages() > 0 {
                let page = right.nth_page(0);
                let pos = left.n_pages();
                right.transfer_page(&page, &left, pos);
            }
        }
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
        wire_host(&chrome.shell.left, id, Pane::Left, &ctx);
        wire_host(&chrome.shell.right, id, Pane::Right, &ctx);
        self.sides.borrow_mut().push(SideWindow {
            id,
            chrome,
            syncing: Rc::new(Cell::new(false)),
        });
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
        let Some((
            book,
            chapter,
            search_btn,
            prev,
            next,
            back,
            forward,
            goto_entry,
            goto_popover,
            window,
            syncing,
        )) = ({
            let sides = self.sides.borrow();
            let Some(side) = sides.iter().find(|side| side.id == id) else {
                return;
            };
            Some((
                side.chrome.book.clone(),
                side.chrome.chapter.clone(),
                side.chrome.search_btn.clone(),
                side.chrome.prev.clone(),
                side.chrome.next.clone(),
                side.chrome.back.clone(),
                side.chrome.forward.clone(),
                side.chrome.goto_entry.clone(),
                side.chrome.goto_popover.clone(),
                side.chrome.window.clone(),
                side.syncing.clone(),
            ))
        })
        else {
            return;
        };
        picker::prepare(&book, gtk::StringFilterMatchMode::Substring);
        picker::prepare(&chapter, gtk::StringFilterMatchMode::Prefix);
        picker::fill_books(&book, &self.books);
        let tx = self.msg_tx.clone();
        let sync = syncing.clone();
        book.connect_selected_notify(move |dd| {
            if sync.get() {
                return;
            }
            let pos = dd.selected();
            if pos != gtk::INVALID_LIST_POSITION {
                tx.emit(Msg::SelectBookIndex(id, pos));
            }
        });
        let tx = self.msg_tx.clone();
        let sync = syncing.clone();
        chapter.connect_selected_notify(move |dd| {
            if sync.get() {
                return;
            }
            let pos = dd.selected();
            if pos != gtk::INVALID_LIST_POSITION {
                tx.emit(Msg::SelectChapterIndex(id, pos));
            }
        });
        let tx = self.msg_tx.clone();
        prev.connect_clicked(move |_| tx.emit(Msg::PrevChapter(id)));
        let tx = self.msg_tx.clone();
        next.connect_clicked(move |_| tx.emit(Msg::NextChapter(id)));
        let tx = self.msg_tx.clone();
        back.connect_clicked(move |_| tx.emit(Msg::Back(id)));
        let tx = self.msg_tx.clone();
        forward.connect_clicked(move |_| tx.emit(Msg::Forward(id)));
        let tx = self.msg_tx.clone();
        search_btn.connect_toggled(move |btn| tx.emit(Msg::SetSearch(id, btn.is_active())));
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
        let Some(from) = self.view_holding(id) else {
            return;
        };
        let Some(hosted) = self.hosted.get(&id) else {
            return;
        };
        let page = hosted.page.clone();
        let Some(outcome) = self.workspace.detach(id) else {
            return;
        };
        self.ensure_side(outcome.window);
        let Some(dest) = self.view_in(outcome.window, Pane::Left) else {
            return;
        };
        from.transfer_page(&page, &dest, 0);
        self.collapse_empty(outcome.source);
        self.sync_shells();
        self.select_tab(id);
    }

    fn move_tab_beside(&mut self, id: TabId) {
        let Some(from) = self.view_holding(id) else {
            return;
        };
        if !self.workspace.move_beside(id) {
            return;
        }
        let Some(tab) = self.workspace.tab(id) else {
            return;
        };
        let window = tab.window;
        let dest_pane = tab.pane;
        let Some(dest) = self.view_in(window, dest_pane) else {
            return;
        };
        if from != dest {
            if let Some(hosted) = self.hosted.get(&id) {
                from.transfer_page(&hosted.page, &dest, dest.n_pages());
            }
        }
        self.select_tab(id);
        self.sync_shells();
    }

    fn split_tab(&mut self, id: TabId) {
        match self.workspace.split_tab(id) {
            SplitOutcome::Duplicated { id: new_id, at } => {
                self.spawn_passage(new_id, at);
            }
            SplitOutcome::Moved => {
                let Some(from) = self.view_holding(id) else {
                    return;
                };
                let Some(tab) = self.workspace.tab(id) else {
                    return;
                };
                let window = tab.window;
                let pane = tab.pane;
                let Some(dest) = self.view_in(window, pane) else {
                    return;
                };
                if from != dest {
                    if let Some(hosted) = self.hosted.get(&id) {
                        from.transfer_page(&hosted.page, &dest, dest.n_pages());
                    }
                }
                self.select_tab(id);
            }
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
            if let Some(id) = self
                .workspace
                .find_kind(|k| matches!(k, TabKind::Marks { .. }))
            {
                if let (Some(TabContent::Marks(widgets)), Some(user)) = (
                    self.hosted.get_mut(&id).map(|h| &mut h.content),
                    self.user.as_ref(),
                ) {
                    marks::refresh_lists(widgets, user, &self.books);
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
        self.ensure_marks("notes");
        let at = self.at();
        let text = self
            .user
            .as_ref()
            .and_then(|u| user_db::get_note(u, at).ok().flatten())
            .unwrap_or_default();
        if let Some(id) = self
            .workspace
            .find_kind(|k| matches!(k, TabKind::Marks { .. }))
        {
            if let Some(TabContent::Marks(w)) = self.hosted.get_mut(&id).map(|h| &mut h.content) {
                marks::edit_note(w, &self.books, at, &text);
            }
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
        self.sync_header(WindowId::MAIN);
        let ids: Vec<WindowId> = self.sides.borrow().iter().map(|side| side.id).collect();
        for id in ids {
            self.sync_header(id);
        }
    }

    fn sync_header(&self, window: WindowId) {
        let at = self.at_in(window);
        let chapters = self
            .conn
            .as_ref()
            .and_then(|conn| bible_app_db::max_chapter(conn, at.book).ok())
            .unwrap_or(1);
        if window == WindowId::MAIN {
            self.picker_syncing.set(true);
            picker::select_book(&self.book_dropdown, &self.books, at.book);
            picker::sync_chapters(&self.chapter_dropdown, chapters, at.chapter);
            self.picker_syncing.set(false);
            return;
        }
        let history = self.has_history_in(window);
        let back = self.can_back_in(window);
        let forward = self.can_forward_in(window);
        let searching = self.workspace.has_search(window);
        let sides = self.sides.borrow();
        let Some(side) = sides.iter().find(|side| side.id == window) else {
            return;
        };
        side.syncing.set(true);
        picker::select_book(&side.chrome.book, &self.books, at.book);
        picker::sync_chapters(&side.chrome.chapter, chapters, at.chapter);
        side.syncing.set(false);
        side.chrome.history.set_visible(history);
        side.chrome.back.set_sensitive(back);
        side.chrome.forward.set_sensitive(forward);
        if side.chrome.search_btn.is_active() != searching {
            side.chrome.search_btn.set_active(searching);
        }
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

fn wire_host(host: &shell::PaneHost, window: WindowId, pane: Pane, ctx: &WireCtx) {
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
        send.emit(Msg::TabAttached { id, window, pane });
    });
    let split_view = host.view.clone();
    let send = ctx.sender.clone();
    host.split_btn.connect_clicked(move |_| {
        if let Some(id) = shell::selected_tab_id(&split_view) {
            send.emit(Msg::SplitTab(id));
        }
    });
    let pop_view = host.view.clone();
    let send = ctx.sender.clone();
    host.popout_btn.connect_clicked(move |_| {
        if let Some(id) = shell::selected_tab_id(&pop_view) {
            send.emit(Msg::DetachTab(id));
        }
    });
    let ctx = ctx.clone();
    view.connect_create_window(move |_| Some(open_drag_window(&ctx)));
}

fn open_drag_window(ctx: &WireCtx) -> adw::TabView {
    let id = WindowId::from_raw(ctx.counter.get());
    ctx.counter.set(id.raw() + 1);
    let chrome = shell::open_side_window(&ctx.app_menu);
    chrome.window.insert_action_group("win", Some(&ctx.actions));
    wire_host(&chrome.shell.left, id, Pane::Left, ctx);
    wire_host(&chrome.shell.right, id, Pane::Right, ctx);
    let view = chrome.shell.left.view.clone();
    ctx.sides.borrow_mut().push(SideWindow {
        id,
        chrome,
        syncing: Rc::new(Cell::new(false)),
    });
    ctx.sender.emit(Msg::WireSide(id));
    view
}

relm4::new_action_group!(WindowActionGroup, "win");
relm4::new_stateful_action!(ParagraphsAction, WindowActionGroup, "paragraphs", (), bool);
relm4::new_stateless_action!(CopyVerseAction, WindowActionGroup, "copy-verse");
relm4::new_stateless_action!(FontLargerAction, WindowActionGroup, "font-larger");
relm4::new_stateless_action!(FontSmallerAction, WindowActionGroup, "font-smaller");
relm4::new_stateful_action!(MhcAction, WindowActionGroup, "mhc", (), bool);
relm4::new_stateful_action!(TskAction, WindowActionGroup, "tsk", (), bool);
relm4::new_stateless_action!(BookmarksAction, WindowActionGroup, "bookmarks");
relm4::new_stateless_action!(NotesAction, WindowActionGroup, "notes");
relm4::new_stateless_action!(ExportNotesAction, WindowActionGroup, "export-notes");
relm4::new_stateless_action!(ToggleBookmarkAction, WindowActionGroup, "toggle-bookmark");
relm4::new_stateless_action!(AddNoteAction, WindowActionGroup, "add-note");
relm4::new_stateless_action!(DetachTabAction, WindowActionGroup, "tab-detach");
relm4::new_stateless_action!(BesideTabAction, WindowActionGroup, "tab-open-beside");
relm4::new_stateful_action!(FollowTabAction, WindowActionGroup, "tab-follow", (), bool);
