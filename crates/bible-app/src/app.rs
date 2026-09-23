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
use crate::shell::{self, DetachedHost, SplitShell};
use crate::strongs;
use crate::tsk;
use crate::user_db;
use crate::workspace::{CloseOutcome, MarksPage, Pane, TabId, TabKind, Workspace};
use adw::prelude::*;
use bible_app_db::{self, Book, DictModule, LibraryHit, LibraryKind, MatchMode, SearchScope};
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
}

#[derive(Clone)]
struct MainViews {
    left: adw::TabView,
    right: adw::TabView,
}

pub struct App {
    conn: Option<Connection>,
    books: Vec<Book>,
    error: Option<String>,
    book_dropdown: gtk::DropDown,
    chapter_dropdown: gtk::DropDown,
    picker_syncing: Rc<Cell<bool>>,
    search_open: bool,
    search_query: String,
    search_scope: SearchScope,
    search_mode: MatchMode,
    search_range: search::SearchRange,
    search_chip: search::BookChip,
    search_hits: Vec<LibraryHit>,
    search_status: String,
    search_total: i64,
    search_tokens: Vec<String>,
    search_strongs: Option<String>,
    search_chosen: Option<u8>,
    search_book_counts: Vec<bible_app_db::BookCount>,
    search_origin: Option<Ref>,
    search_mark: Option<SearchMark>,
    search_gen: Rc<Cell<u64>>,
    search_hold: Rc<Cell<bool>>,
    search_groups: Rc<RefCell<Vec<String>>>,
    search_db_path: Option<PathBuf>,
    search_list: gtk::ListBox,
    search_entry: gtk::SearchEntry,
    search_chips: gtk::Box,
    dict_modules: Vec<DictModule>,
    font_size: i32,
    paragraphs: bool,
    font_provider: gtk::CssProvider,
    mhc_action: gio::SimpleAction,
    tsk_action: gio::SimpleAction,
    follow_action: gio::SimpleAction,
    user: Option<Connection>,
    workspace: Workspace,
    hosted: HashMap<TabId, HostedTab>,
    shell: Option<SplitShell>,
    detached: Rc<RefCell<Vec<DetachedHost>>>,
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
    SelectBookIndex(u32),
    SelectChapterIndex(u32),
    PrevChapter,
    NextChapter,
    GoTo(String),
    GoToBeside(String),
    SetSearch(bool),
    Search(String),
    SetSearchScope(SearchScope),
    SetSearchMode(MatchMode),
    SetSearchRange(search::SearchRange),
    SelectSearchBook(Option<u8>),
    SearchReady(u64, search::Outcome),
    SearchMore,
    PreviewHit(i32),
    SearchActivate,
    OpenHit(i32),
    OpenHitBeside(i32),
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
    Back,
    Forward,
    CopyVerses,
    CopyAtOffset(i32),
    FontSmaller,
    FontLarger,
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
        pane: Pane,
        detached: bool,
    },
    SetupTabMenu(Option<TabId>),
    DetachMenuTab,
    BesideMenuTab,
    SetTabFollow(bool),
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
                            set_sensitive: model.error.is_none() && !model.search_open,
                        },

                        #[local_ref]
                        chapter_dropdown -> gtk::DropDown {
                            set_enable_search: true,
                            set_search_match_mode: gtk::StringFilterMatchMode::Prefix,
                            set_tooltip_text: Some("Chapter"),
                            set_valign: gtk::Align::Center,
                            add_css_class: "chapter-picker",
                            #[watch]
                            set_sensitive: model.error.is_none() && !model.search_open,
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
                                #[watch]
                                set_sensitive: !model.search_open,
                                connect_clicked => Msg::PrevChapter,
                            },
                            gtk::Button {
                                set_icon_name: "go-next-symbolic",
                                set_tooltip_text: Some("Next chapter (Alt+Right)"),
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: !model.search_open,
                                connect_clicked => Msg::NextChapter,
                            },
                        },
                        gtk::Box {
                            add_css_class: "linked",

                            gtk::Button {
                                set_icon_name: "edit-undo-symbolic",
                                set_tooltip_text: Some("Back in history (Alt+Shift+Left)"),
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: model.can_back() && !model.search_open,
                                connect_clicked => Msg::Back,
                            },
                            gtk::Button {
                                set_icon_name: "edit-redo-symbolic",
                                set_tooltip_text: Some("Forward in history (Alt+Shift+Right)"),
                                set_valign: gtk::Align::Center,
                                #[watch]
                                set_sensitive: model.can_forward() && !model.search_open,
                                connect_clicked => Msg::Forward,
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
                        set_active: model.search_open,
                        connect_toggled[sender] => move |btn| {
                            sender.input(Msg::SetSearch(btn.is_active()));
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
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_width_request: 560,
                            set_spacing: 8,
                            set_margin_start: 12,
                            set_margin_end: 12,
                            set_margin_top: 12,
                            set_margin_bottom: 12,
                            add_css_class: "search-pane",
                            #[watch]
                            set_visible: model.search_open,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,

                                #[local_ref]
                                search_entry -> gtk::SearchEntry {
                                    #[watch]
                                    set_placeholder_text: Some(search::placeholder(model.search_scope)),
                                    set_tooltip_text: Some("Enter stays on the verse. Esc returns. Shift+Enter opens beside."),
                                    set_hexpand: true,
                                    connect_search_changed[sender] => move |entry| {
                                        sender.input(Msg::Search(entry.text().to_string()));
                                    },
                                    connect_activate => Msg::SearchActivate,
                                    connect_stop_search => Msg::SetSearch(false),
                                },

                                #[local_ref]
                                search_scope -> gtk::DropDown {
                                    set_tooltip_text: Some("Search in"),
                                    set_valign: gtk::Align::Center,
                                    set_hexpand: false,
                                }
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,

                                #[local_ref]
                                search_mode_dd -> gtk::DropDown {
                                    set_tooltip_text: Some("Match"),
                                    set_hexpand: true,
                                },

                                #[local_ref]
                                search_range_dd -> gtk::DropDown {
                                    set_tooltip_text: Some("Range"),
                                    set_hexpand: true,
                                }
                            },

                            gtk::Label {
                                #[watch]
                                set_label: &model.search_status,
                                set_xalign: 0.0,
                                set_wrap: true,
                                add_css_class: "dim-label",
                            },

                            gtk::Label {
                                #[watch]
                                set_label: search::empty_description(
                                    &model.search_query,
                                    model.search_scope,
                                )
                                .unwrap_or(""),
                                #[watch]
                                set_visible: search::empty_description(
                                    &model.search_query,
                                    model.search_scope,
                                )
                                .is_some(),
                                set_xalign: 0.0,
                                set_wrap: true,
                                add_css_class: "dim-label",
                            },

                            gtk::ScrolledWindow {
                                set_policy: (
                                    gtk::PolicyType::Automatic,
                                    gtk::PolicyType::Never,
                                ),
                                set_propagate_natural_height: true,

                                #[local_ref]
                                search_chips -> gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 6,
                                }
                            },

                            gtk::ScrolledWindow {
                                #[watch]
                                set_visible: !model.search_hits.is_empty(),
                                set_hexpand: true,
                                set_vexpand: true,
                                set_policy: (
                                    gtk::PolicyType::Never,
                                    gtk::PolicyType::Automatic,
                                ),

                                #[local_ref]
                                search_list -> gtk::ListBox {
                                    set_selection_mode: gtk::SelectionMode::Single,
                                    set_accessible_role: gtk::AccessibleRole::List,
                                    connect_row_activated[sender] => move |_, row| {
                                        sender.input(Msg::OpenHit(row.index()));
                                    }
                                }
                            }
                        },

                        gtk::Separator {
                            set_orientation: gtk::Orientation::Vertical,
                            #[watch]
                            set_visible: model.search_open,
                        },

                        #[local_ref]
                        workspace_host -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,
                            set_vexpand: true,
                        }
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
        let search_list = gtk::ListBox::new();
        let search_entry = gtk::SearchEntry::new();
        let search_chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let scope_labels = SearchScope::ALL.map(SearchScope::label);
        let search_scope = gtk::DropDown::from_strings(&scope_labels);
        search_scope.set_selected(SearchScope::Kjv.index());
        search_scope.set_enable_search(false);
        search_scope.update_property(&[gtk::accessible::Property::Label("Search in")]);
        let mode_labels = MatchMode::ALL.map(MatchMode::label);
        let search_mode_dd = gtk::DropDown::from_strings(&mode_labels);
        search_mode_dd.set_enable_search(false);
        search_mode_dd.update_property(&[gtk::accessible::Property::Label("Match")]);
        let range_labels = search::SearchRange::ALL.map(search::SearchRange::label);
        let search_range_dd = gtk::DropDown::from_strings(&range_labels);
        search_range_dd.set_selected(search::SearchRange::All.index());
        search_range_dd.set_enable_search(false);
        search_range_dd.update_property(&[gtk::accessible::Property::Label("Range")]);
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
        search_mode_dd.set_selected(search_mode.index());

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
            goto_sender.input(Msg::GoTo(entry.text().to_string()));
        });
        let goto_keys = gtk::EventControllerKey::new();
        goto_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let goto_entry_shift = goto_entry.clone();
        let goto_shift_sender = sender.clone();
        goto_keys.connect_key_pressed(move |_, keyval, _, mods| {
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shift && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                goto_shift_sender.input(Msg::GoToBeside(goto_entry_shift.text().to_string()));
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

        let mut model = App {
            conn,
            books,
            error,
            book_dropdown: book_dropdown.clone(),
            chapter_dropdown: chapter_dropdown.clone(),
            picker_syncing: Rc::new(Cell::new(false)),
            search_open: false,
            search_query: String::new(),
            search_scope: SearchScope::Kjv,
            search_mode,
            search_range: search::SearchRange::All,
            search_chip: search::BookChip::Auto,
            search_hits: Vec::new(),
            search_status: search::status("", 0, 0, SearchScope::Kjv),
            search_total: 0,
            search_tokens: Vec::new(),
            search_strongs: None,
            search_chosen: None,
            search_book_counts: Vec::new(),
            search_origin: None,
            search_mark: None,
            search_gen: Rc::new(Cell::new(0)),
            search_hold: Rc::new(Cell::new(false)),
            search_groups: Rc::new(RefCell::new(Vec::new())),
            search_db_path,
            search_list: search_list.clone(),
            search_entry: search_entry.clone(),
            search_chips: search_chips.clone(),
            dict_modules,
            font_size,
            paragraphs,
            font_provider,
            mhc_action: mhc_gio,
            tsk_action: tsk_gio,
            follow_action: follow_gio,
            user,
            workspace: Workspace::new(at),
            hosted: HashMap::new(),
            shell: None,
            detached: Rc::new(RefCell::new(Vec::new())),
            menu_tab: Rc::new(Cell::new(None)),
            goto_entry: goto_entry.clone(),
            goto_popover: goto_popover.clone(),
            actions: gio::SimpleActionGroup::new(),
            msg_tx: sender.input_sender().clone(),
            copy_offset,
        };
        model.apply_font();
        picker::install_css();
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
            let mains = MainViews {
                left: shell.left.view.clone(),
                right: shell.right.view.clone(),
            };
            let menu = shell::tab_menu_model();
            wire_tab_view(
                &mains.left,
                mains.clone(),
                sender.input_sender().clone(),
                model.detached.clone(),
                model.actions.clone(),
                &menu,
            );
            wire_tab_view(
                &mains.right,
                mains.clone(),
                sender.input_sender().clone(),
                model.detached.clone(),
                model.actions.clone(),
                &menu,
            );
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
                sender_keys.input(Msg::SetSearch(true));
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Escape {
                if goto_popover_keys.is_visible() {
                    goto_popover_keys.popdown();
                    return glib::Propagation::Stop;
                }
                sender_keys.input(Msg::SetSearch(false));
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::l || keyval == gtk::gdk::Key::L) {
                goto_popover_keys.popup();
                goto_entry_keys.grab_focus();
                goto_entry_keys.select_region(0, -1);
                return glib::Propagation::Stop;
            }
            if alt && shift && keyval == gtk::gdk::Key::Left {
                sender_keys.input(Msg::Back);
                return glib::Propagation::Stop;
            }
            if alt && shift && keyval == gtk::gdk::Key::Right {
                sender_keys.input(Msg::Forward);
                return glib::Propagation::Stop;
            }
            if alt && keyval == gtk::gdk::Key::Left {
                sender_keys.input(Msg::PrevChapter);
                return glib::Propagation::Stop;
            }
            if alt && keyval == gtk::gdk::Key::Right {
                sender_keys.input(Msg::NextChapter);
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Back {
                sender_keys.input(Msg::Back);
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Forward {
                sender_keys.input(Msg::Forward);
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
            back_sender.input(Msg::Back);
        });
        root.add_controller(mouse_back);
        let mouse_forward = gtk::GestureClick::new();
        mouse_forward.set_button(9);
        let forward_sender = sender.clone();
        mouse_forward.connect_pressed(move |_, _, _, _| {
            forward_sender.input(Msg::Forward);
        });
        root.add_controller(mouse_forward);

        let down = gtk::EventControllerKey::new();
        let list = model.search_list.clone();
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
        model.search_entry.add_controller(down);

        let search_shift = gtk::EventControllerKey::new();
        search_shift.set_propagation_phase(gtk::PropagationPhase::Capture);
        let search_shift_sender = sender.clone();
        let search_list_shift = model.search_list.clone();
        search_shift.connect_key_pressed(move |_, keyval, _, mods| {
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shift && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                let idx = search_list_shift
                    .selected_row()
                    .map(|r| r.index())
                    .unwrap_or(0);
                search_shift_sender.input(Msg::OpenHitBeside(idx));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        model.search_entry.add_controller(search_shift);

        let list_shift = gtk::EventControllerKey::new();
        list_shift.set_propagation_phase(gtk::PropagationPhase::Capture);
        let list_shift_sender = sender.clone();
        list_shift.connect_key_pressed(move |_, keyval, _, mods| {
            let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
            if shift && (keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter) {
                list_shift_sender.input(Msg::OpenHitBeside(-1));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        model.search_list.add_controller(list_shift);

        let scope_sender = sender.clone();
        search_scope.connect_selected_notify(move |dd| {
            let pos = dd.selected();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }
            scope_sender.input(Msg::SetSearchScope(SearchScope::from_index(pos)));
        });
        let mode_sender = sender.clone();
        search_mode_dd.connect_selected_notify(move |dd| {
            let pos = dd.selected();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }
            mode_sender.input(Msg::SetSearchMode(MatchMode::from_index(pos)));
        });
        let range_sender = sender.clone();
        search_range_dd.connect_selected_notify(move |dd| {
            let pos = dd.selected();
            if pos == gtk::INVALID_LIST_POSITION {
                return;
            }
            range_sender.input(Msg::SetSearchRange(search::SearchRange::from_index(pos)));
        });

        let groups = model.search_groups.clone();
        model.search_list.set_header_func(move |row, before| {
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
            let prev = before.and_then(|b| {
                let prev_idx = b.index();
                (prev_idx >= 0)
                    .then(|| groups.get(prev_idx as usize))
                    .flatten()
            });
            if prev.is_some_and(|p| p == group) {
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
        let preview_sender = sender.clone();
        let preview_hold = model.search_hold.clone();
        model.search_list.connect_row_selected(move |_, row| {
            if preview_hold.get() {
                return;
            }
            let Some(row) = row else { return };
            preview_sender.input(Msg::PreviewHit(row.index()));
        });
        if let Some(scroll) = model
            .search_list
            .parent()
            .and_downcast::<gtk::ScrolledWindow>()
        {
            let more = sender.clone();
            scroll.connect_edge_reached(move |_, pos| {
                if pos == gtk::PositionType::Bottom {
                    more.input(Msg::SearchMore);
                }
            });
        }
        let list_esc = gtk::EventControllerKey::new();
        let esc_sender = sender.clone();
        list_esc.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                esc_sender.input(Msg::SetSearch(false));
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        model.search_list.add_controller(list_esc);

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
            book_sender.input(Msg::SelectBookIndex(pos));
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
            chapter_sender.input(Msg::SelectChapterIndex(pos));
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::SelectBookIndex(idx) => {
                let Some(id) = picker::book_id_at(&self.books, idx) else {
                    return;
                };
                if self.at().book == id {
                    return;
                }
                self.go(
                    Ref {
                        book: id,
                        chapter: 1,
                        verse: 1,
                    },
                    false,
                );
            }
            Msg::SelectChapterIndex(idx) => {
                let Some(chapter) = picker::chapter_from_index(idx) else {
                    return;
                };
                if self.at().chapter == chapter {
                    return;
                }
                self.go(
                    Ref {
                        book: self.at().book,
                        chapter,
                        verse: 1,
                    },
                    false,
                );
            }
            Msg::PrevChapter => {
                if self.search_open {
                    return;
                }
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::prev_chapter(conn, &self.books, self.at()) {
                        self.go(at, false);
                    }
                }
            }
            Msg::NextChapter => {
                if self.search_open {
                    return;
                }
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::next_chapter(conn, &self.books, self.at()) {
                        self.go(at, false);
                    }
                }
            }
            Msg::GoTo(text) => {
                let text = text.trim();
                if text.is_empty() {
                    self.goto_popover.popdown();
                    return;
                }
                if let Some(at) = nav::parse_ref(text, &self.books, self.at()) {
                    self.search_open = false;
                    self.go(at, true);
                    self.goto_popover.popdown();
                }
            }
            Msg::GoToBeside(text) => {
                let text = text.trim();
                if text.is_empty() {
                    self.goto_popover.popdown();
                    return;
                }
                if let Some(at) = nav::parse_ref(text, &self.books, self.at()) {
                    self.search_open = false;
                    self.go_beside(at);
                    self.goto_popover.popdown();
                }
            }
            Msg::SetSearch(open) => {
                if self.error.is_some() {
                    return;
                }
                if self.search_open == open {
                    if open {
                        self.focus_search();
                    }
                    return;
                }
                if open {
                    self.search_origin = Some(self.at());
                    self.search_open = true;
                    self.focus_search();
                    if !self.search_query.trim().is_empty() {
                        self.schedule_search(false);
                    }
                } else {
                    self.close_search(true);
                }
            }
            Msg::Search(query) => {
                self.search_query = query;
                self.search_chip = search::BookChip::Auto;
                self.schedule_search(false);
            }
            Msg::SetSearchScope(scope) => {
                if self.search_scope != scope {
                    self.search_scope = scope;
                    self.search_chip = search::BookChip::Auto;
                    self.schedule_search(false);
                }
            }
            Msg::SetSearchMode(mode) => {
                if self.search_mode != mode {
                    self.search_mode = mode;
                    self.search_chip = search::BookChip::Auto;
                    self.save_state();
                    self.schedule_search(false);
                }
            }
            Msg::SetSearchRange(range) => {
                if self.search_range != range {
                    self.search_range = range;
                    self.search_chip = search::BookChip::Auto;
                    self.schedule_search(false);
                }
            }
            Msg::SelectSearchBook(book) => {
                self.search_chip = match book {
                    Some(id) => search::BookChip::Book(id),
                    None => search::BookChip::AllBooks,
                };
                self.schedule_search(false);
            }
            Msg::SearchReady(gen, outcome) => {
                if gen != self.search_gen.get() {
                    return;
                }
                self.apply_outcome(outcome);
            }
            Msg::SearchMore => {
                let shown = self
                    .search_hits
                    .iter()
                    .filter(|hit| search::hit_ref(hit).is_some())
                    .count() as i64;
                if self.search_query.trim().is_empty() || shown >= self.search_total {
                    return;
                }
                self.schedule_search(true);
            }
            Msg::PreviewHit(idx) => {
                self.preview_hit(idx);
            }
            Msg::SearchActivate => {
                let idx = self
                    .search_list
                    .selected_row()
                    .map(|r| r.index())
                    .unwrap_or(0);
                self.open_hit(idx, false, true);
            }
            Msg::OpenHit(idx) => {
                self.open_hit(idx, false, false);
            }
            Msg::OpenHitBeside(idx) => {
                let idx = if idx < 0 {
                    self.search_list
                        .selected_row()
                        .map(|r| r.index())
                        .unwrap_or(0)
                } else {
                    idx
                };
                self.open_hit(idx, true, true);
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
                self.search_open = false;
                self.go(at, true);
            }
            Msg::OpenTskDest(at) => {
                self.popdown_passage_popovers();
                self.search_open = false;
                self.go(at, true);
            }
            Msg::OpenTskDestBeside(at) => {
                self.popdown_passage_popovers();
                self.search_open = false;
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
                self.search_open = false;
                self.go(at, true);
            }
            Msg::Back => {
                if self.search_open {
                    return;
                }
                let Some(id) = self.workspace.focused_passage_id() else {
                    return;
                };
                let Some(at) = self.passage_mut(id).and_then(|p| p.history.back()) else {
                    return;
                };
                self.apply_passage_ref(id, at, true, false);
            }
            Msg::Forward => {
                if self.search_open {
                    return;
                }
                let Some(id) = self.workspace.focused_passage_id() else {
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
            Msg::SetParagraphs(on) => {
                if self.paragraphs == on {
                    return;
                }
                self.paragraphs = on;
                self.save_state();
                if !self.search_open {
                    self.reload_all_passages(false);
                }
            }
            Msg::OpenBookmarks => self.ensure_marks("bookmarks"),
            Msg::OpenNotes => self.ensure_marks("notes"),
            Msg::MarksBookmarkActivated(idx) => {
                if let Some(at) = self
                    .marks_widgets()
                    .and_then(|w| marks::bookmark_at(w, idx))
                {
                    self.search_open = false;
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
                    self.search_open = false;
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
            Msg::TabAttached { id, pane, detached } => {
                self.workspace.set_host(id, pane, detached);
                self.workspace.focus(id);
                self.collapse_empty_panes();
                self.sync_shell();
            }
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
        }
        self.sync_study_actions();
        let _ = sender;
    }
}

impl App {
    fn at(&self) -> Ref {
        self.workspace
            .focused_passage_ref()
            .unwrap_or_else(|| self.workspace.last_at())
    }

    fn can_back(&self) -> bool {
        self.focused_passage().is_some_and(|p| p.history.can_back())
    }

    fn can_forward(&self) -> bool {
        self.focused_passage()
            .is_some_and(|p| p.history.can_forward())
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

    fn schedule_search(&mut self, append: bool) {
        let next = self.search_gen.get().saturating_add(1);
        self.search_gen.set(next);
        let query = self.search_query.clone();
        let plan = search::plan(
            &query,
            &self.books,
            self.search_origin.unwrap_or_else(|| self.at()),
            self.search_mode,
            self.search_range,
            self.search_chip,
            self.search_scope,
        );
        match plan {
            search::Plan::Idle => {
                self.clear_search_results();
                self.search_status = search::status("", 0, 0, self.search_scope);
            }
            search::Plan::Short => {
                self.clear_search_results();
                self.search_status = search::short_status().into();
            }
            search::Plan::Goto(at) => {
                self.search_hits = vec![search::goto_hit(at, &self.books)];
                self.search_total = 1;
                self.search_tokens.clear();
                self.search_strongs = None;
                self.search_book_counts.clear();
                self.search_chosen = None;
                self.search_status = format!("Go to {}", nav::format_ref(&self.books, at));
                self.refill_hits(false);
                self.refill_chips();
            }
            search::Plan::Ready(mut prepared) => {
                prepared.db_path = self.search_db_path.clone().unwrap_or_default();
                prepared.user_path = user_db::user_db_path();
                let after = if append {
                    self.search_hits.iter().rev().find_map(|hit| {
                        let at = search::hit_ref(hit)?;
                        Some((at.book, at.chapter, at.verse))
                    })
                } else {
                    None
                };
                let gen = next;
                let gen_cell = self.search_gen.clone();
                let tx = self.msg_tx.clone();
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    if gen_cell.get() != gen {
                        return;
                    }
                    std::thread::spawn(move || {
                        let outcome = search::execute(&prepared, after, append);
                        glib::MainContext::default().invoke(move || {
                            tx.emit(Msg::SearchReady(gen, outcome));
                        });
                    });
                });
            }
        }
    }

    fn clear_search_results(&mut self) {
        self.search_hits.clear();
        self.search_total = 0;
        self.search_tokens.clear();
        self.search_strongs = None;
        self.search_book_counts.clear();
        self.search_chosen = None;
        self.refill_hits(false);
        self.refill_chips();
    }

    fn apply_outcome(&mut self, outcome: search::Outcome) {
        let append = outcome.append;
        if append {
            self.search_hits.extend(outcome.hits);
        } else {
            self.search_hits = outcome.hits;
            self.search_book_counts = outcome.by_book;
            self.search_chosen = outcome.chosen_book;
        }
        self.search_total = outcome.total;
        self.search_tokens = outcome.tokens;
        self.search_strongs = outcome.strongs;
        self.maybe_lexicon_row();
        let shown = self
            .search_hits
            .iter()
            .filter(|hit| search::hit_ref(hit).is_some())
            .count();
        self.search_status = search::status(
            &self.search_query,
            shown,
            self.search_total,
            self.search_scope,
        );
        self.refill_hits(append);
        if !append {
            self.refill_chips();
            if let Some(row) = self.search_list.selected_row() {
                self.preview_hit(row.index());
            }
        }
    }

    fn maybe_lexicon_row(&mut self) {
        let Some(code) = self.search_strongs.clone() else {
            return;
        };
        if self
            .search_hits
            .iter()
            .any(|hit| hit.kind == LibraryKind::Lexicon)
        {
            return;
        }
        let shown = self
            .search_hits
            .iter()
            .filter(|hit| search::hit_ref(hit).is_some())
            .count() as i64;
        if shown < self.search_total {
            return;
        }
        let module = if code.starts_with('G') {
            "Thayer"
        } else {
            "BDB"
        };
        if self.dict_modules.iter().any(|m| m.id == module) {
            self.search_hits.push(search::lexicon_hit(&code, module));
        }
    }

    fn refill_hits(&mut self, append: bool) {
        let hits = if append {
            let previous = self.search_groups.borrow().len();
            self.search_hits[previous.min(self.search_hits.len())..].to_vec()
        } else {
            self.search_hits.clone()
        };
        self.search_hold.set(true);
        search::refill_list(
            &self.search_list,
            &hits,
            &self.books,
            self.search_scope,
            &self.search_tokens,
            &self.search_groups,
            append,
        );
        self.search_hold.set(false);
    }

    fn refill_chips(&self) {
        while let Some(child) = self.search_chips.first_child() {
            self.search_chips.remove(&child);
        }
        if !search::show_chips(self.search_scope, self.search_strongs.is_some())
            || self.search_book_counts.len() < 2
        {
            return;
        }
        let sender = self.msg_tx.clone();
        if self.search_chosen.is_some() {
            let all = gtk::ToggleButton::with_label("All");
            all.add_css_class("flat");
            all.set_active(false);
            let tx = sender.clone();
            all.connect_clicked(move |btn| {
                if btn.is_active() {
                    tx.emit(Msg::SelectSearchBook(None));
                }
            });
            self.search_chips.append(&all);
        }
        let mut leader: Option<gtk::ToggleButton> = None;
        for count in &self.search_book_counts {
            let button = gtk::ToggleButton::with_label(&search::chip_text(&self.books, count));
            button.add_css_class("flat");
            button.set_active(Some(count.book) == self.search_chosen);
            if let Some(first) = &leader {
                button.set_group(Some(first));
            } else {
                leader = Some(button.clone());
            }
            let tx = sender.clone();
            let book = count.book;
            button.connect_clicked(move |btn| {
                if btn.is_active() {
                    tx.emit(Msg::SelectSearchBook(Some(book)));
                }
            });
            self.search_chips.append(&button);
        }
    }

    fn preview_hit(&mut self, idx: i32) {
        if self.search_hold.get() {
            return;
        }
        let Ok(idx) = usize::try_from(idx) else {
            return;
        };
        let Some(hit) = self.search_hits.get(idx) else {
            return;
        };
        let Some(at) = search::hit_ref(hit) else {
            return;
        };
        self.search_mark = Some(SearchMark {
            at,
            tokens: self.search_tokens.clone(),
            strongs: self.search_strongs.clone(),
        });
        self.preview_at(at);
    }

    fn preview_at(&mut self, at: Ref) {
        let id = match self.workspace.focused_passage_id() {
            Some(id) => id,
            None => {
                let opened = self.workspace.open_passage(at);
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

    fn close_search(&mut self, restore: bool) {
        self.search_open = false;
        self.search_gen.set(self.search_gen.get().saturating_add(1));
        if restore {
            self.search_mark = None;
            if let Some(at) = self.search_origin.take() {
                if let Some(id) = self.workspace.focused_passage_id() {
                    self.apply_passage_ref(id, at, true, false);
                }
            }
        } else {
            self.search_origin = None;
        }
    }

    fn open_hit(&mut self, idx: i32, beside: bool, dismiss: bool) {
        let Ok(idx) = usize::try_from(idx) else {
            return;
        };
        let Some(hit) = self.search_hits.get(idx) else {
            return;
        };
        let kind = hit.kind;
        let module = hit.module.clone();
        let headword = hit.headword.clone();
        let at = search::hit_ref(hit);
        if let Some(at) = at {
            self.search_mark = Some(SearchMark {
                at,
                tokens: self.search_tokens.clone(),
                strongs: self.search_strongs.clone(),
            });
        }
        if dismiss {
            self.search_origin = None;
            self.search_open = false;
        }
        match kind {
            LibraryKind::Verse | LibraryKind::Note => {
                let Some(at) = at else {
                    return;
                };
                if !dismiss {
                    self.preview_at(at);
                } else if beside {
                    self.go_beside(at);
                } else {
                    self.go(at, true);
                }
            }
            LibraryKind::Commentary => {
                let Some(at) = at else {
                    return;
                };
                if dismiss {
                    self.go(at, true);
                } else {
                    self.preview_at(at);
                }
                if module == "TSK" {
                    self.ensure_tsk();
                } else {
                    self.ensure_mhc();
                }
            }
            LibraryKind::Dictionary | LibraryKind::Topic | LibraryKind::Lexicon => {
                let Some(headword) = headword.as_deref() else {
                    return;
                };
                self.open_library(&module, Some(headword));
            }
        }
    }

    fn focus_search(&self) {
        let entry = self.search_entry.clone();
        glib::timeout_add_local_once(Duration::from_millis(100), move || {
            entry.grab_focus();
        });
    }

    fn go(&mut self, at: Ref, highlight: bool) {
        let id = match self.workspace.focused_passage_id() {
            Some(id) => id,
            None => {
                let opened = self.workspace.open_passage(at);
                self.spawn_passage(opened.id, at);
                opened.id
            }
        };
        self.apply_passage_ref(id, at, highlight, true);
    }

    fn go_beside(&mut self, at: Ref) {
        let from = self
            .workspace
            .focused_passage_id()
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
        let p = PassageView::new(at, &self.actions);
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
        let Some(shell) = &self.shell else { return };
        let pane = self.workspace.tab(id).map(|t| t.pane).unwrap_or(Pane::Left);
        let view = shell.host(pane).view.clone();
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
        self.sync_shell();
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
        if self.hosted.remove(&id).is_none() {
            return;
        }
        match self.workspace.close(id) {
            CloseOutcome::Closed => {}
            CloseOutcome::Replaced { id: new_id, at } => {
                self.spawn_passage(new_id, at);
            }
        }
        self.collapse_empty_panes();
        self.sync_shell();
        self.sync_pickers();
    }

    fn view_holding(&self, id: TabId) -> Option<adw::TabView> {
        let page = &self.hosted.get(&id)?.page;
        let mut views = Vec::new();
        if let Some(shell) = &self.shell {
            views.push(shell.left.view.clone());
            views.push(shell.right.view.clone());
        }
        for host in self.detached.borrow().iter() {
            views.push(host.view.clone());
        }
        views
            .into_iter()
            .find(|view| (0..view.n_pages()).any(|i| view.nth_page(i) == *page))
    }

    fn sync_shell(&self) {
        if let Some(shell) = &self.shell {
            shell.sync_split();
        }
    }

    fn collapse_empty_panes(&self) {
        let Some(shell) = &self.shell else { return };
        if shell.left.view.n_pages() == 0 && shell.right.view.n_pages() > 0 {
            while shell.right.view.n_pages() > 0 {
                let page = shell.right.view.nth_page(0);
                let pos = shell.left.view.n_pages();
                shell.right.view.transfer_page(&page, &shell.left.view, pos);
            }
        }
        shell.sync_split();
    }

    fn detach_tab(&mut self, id: TabId) {
        let Some(from) = self.view_holding(id) else {
            return;
        };
        let Some(hosted) = self.hosted.get(&id) else {
            return;
        };
        if let Some(shell) = &self.shell {
            if from != shell.left.view && from != shell.right.view {
                return;
            }
        }
        let title = hosted.page.title().to_string();
        let host = shell::open_detached(&title);
        if let Some(shell) = &self.shell {
            let mains = MainViews {
                left: shell.left.view.clone(),
                right: shell.right.view.clone(),
            };
            host.window.insert_action_group("win", Some(&self.actions));
            wire_tab_view(
                &host.view,
                mains,
                self.msg_tx.clone(),
                self.detached.clone(),
                self.actions.clone(),
                &shell::tab_menu_model(),
            );
        }
        from.transfer_page(&hosted.page, &host.view, 0);
        self.detached.borrow_mut().push(host);
        self.workspace.detach(id);
        self.collapse_empty_panes();
        self.sync_shell();
    }

    fn move_tab_beside(&mut self, id: TabId) {
        if !self.workspace.move_beside(id) {
            return;
        }
        let Some(tab) = self.workspace.tab(id) else {
            return;
        };
        let dest_pane = tab.pane;
        let Some(shell) = &self.shell else { return };
        let dest = shell.host(dest_pane).view.clone();
        let Some(from) = self.view_holding(id) else {
            return;
        };
        if from != dest {
            if let Some(hosted) = self.hosted.get(&id) {
                dest.set_visible(true);
                from.transfer_page(&hosted.page, &dest, dest.n_pages());
            }
        }
        self.select_tab(id);
        self.sync_shell();
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
        let at = self.at();
        self.picker_syncing.set(true);
        picker::select_book(&self.book_dropdown, &self.books, at.book);
        let n = self
            .conn
            .as_ref()
            .and_then(|c| bible_app_db::max_chapter(c, at.book).ok())
            .unwrap_or(1);
        picker::sync_chapters(&self.chapter_dropdown, n, at.chapter);
        self.picker_syncing.set(false);
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

    let text = gio::Menu::new();
    text.append(Some("Copy verse"), Some("win.copy-verse"));
    text.append(Some("Larger text"), Some("win.font-larger"));
    text.append(Some("Smaller text"), Some("win.font-smaller"));
    menu.append_section(None, &text);

    let commentary = gio::Menu::new();
    commentary.append(Some("Matthew Henry"), Some("win.mhc"));
    commentary.append(Some("Treasury of Scripture Knowledge"), Some("win.tsk"));
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
    let marks = gio::Menu::new();
    marks.append(Some("Bookmarks"), Some("win.bookmarks"));
    marks.append(Some("Notes"), Some("win.notes"));
    marks.append(Some("Export notes…"), Some("win.export-notes"));
    menu.append_section(None, &marks);

    let study = gio::Menu::new();
    study.append_submenu(Some("Commentary"), &commentary);
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

fn wire_tab_view(
    view: &adw::TabView,
    mains: MainViews,
    sender: relm4::Sender<Msg>,
    detached: Rc<RefCell<Vec<DetachedHost>>>,
    actions: gio::SimpleActionGroup,
    menu: &gio::Menu,
) {
    view.set_menu_model(Some(menu));
    let send = sender.clone();
    view.connect_setup_menu(move |_, page| {
        let id = page.and_then(|p| p.keyword().as_deref().and_then(TabId::from_keyword));
        send.emit(Msg::SetupTabMenu(id));
    });
    let send = sender.clone();
    view.connect_selected_page_notify(move |view| {
        if let Some(page) = view.selected_page() {
            if let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) {
                send.emit(Msg::TabSelected(id));
            }
        }
    });
    let send = sender.clone();
    let mains_close = mains.clone();
    view.connect_page_detached(move |view, page, _| {
        if view.is_transferring_page() {
            return;
        }
        if let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) {
            send.emit(Msg::TabClosed(id));
        }
        let is_main = view == &mains_close.left || view == &mains_close.right;
        if !is_main && view.n_pages() == 0 {
            if let Some(win) = view.root().and_downcast::<gtk::Window>() {
                glib::idle_add_local_once(move || {
                    win.close();
                });
            }
        }
    });
    let send = sender.clone();
    let mains_attach = mains.clone();
    view.connect_page_attached(move |view, page, _| {
        let Some(id) = page.keyword().as_deref().and_then(TabId::from_keyword) else {
            return;
        };
        let (pane, detached_flag) = if view == &mains_attach.left {
            (Pane::Left, false)
        } else if view == &mains_attach.right {
            (Pane::Right, false)
        } else {
            (Pane::Left, true)
        };
        send.emit(Msg::TabAttached {
            id,
            pane,
            detached: detached_flag,
        });
    });
    let send = sender.clone();
    let detached_c = detached.clone();
    let mains_create = mains;
    let menu = menu.clone();
    let actions_c = actions;
    view.connect_create_window(move |_| {
        let host = shell::open_detached("bible-app");
        host.window.insert_action_group("win", Some(&actions_c));
        wire_tab_view(
            &host.view,
            mains_create.clone(),
            send.clone(),
            detached_c.clone(),
            actions_c.clone(),
            &menu,
        );
        let view = host.view.clone();
        detached_c.borrow_mut().push(host);
        Some(view)
    });
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
