use crate::config;
use crate::dict;
use crate::history::History;
use crate::layout::{self, ChapterLayout};
use crate::mhc;
use crate::nav::{self, Ref};
use crate::search;
use crate::sidebar;
use crate::strongs;
use crate::tsk;
use adw::prelude::*;
use bible_app_db::{self, Book, SearchHit};
use gtk::glib;
use relm4::prelude::*;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub struct App {
    conn: Option<Connection>,
    books: Vec<Book>,
    at: Ref,
    title: String,
    error: Option<String>,
    buffer: gtk::TextBuffer,
    book_list: gtk::ListBox,
    chapter_grid: gtk::FlowBox,
    chapter_view: gtk::TextView,
    search_open: bool,
    search_query: String,
    search_hits: Vec<SearchHit>,
    search_status: String,
    search_list: gtk::ListBox,
    search_entry: gtk::SearchEntry,
    mhc: Option<mhc::MhcWidgets>,
    tsk: Option<tsk::TskWidgets>,
    dict: Option<dict::DictWidgets>,
    strongs_popover: gtk::Popover,
    tsk_popover: gtk::Popover,
    history: History,
    layout: ChapterLayout,
    font_size: i32,
    interlinear: bool,
    paragraphs: bool,
    font_provider: gtk::CssProvider,
    xref_tips: Rc<RefCell<Vec<(i32, i32, String)>>>,
}

#[derive(Debug)]
pub enum Msg {
    SelectBook(u8),
    SelectChapter(u8),
    PrevChapter,
    NextChapter,
    GoTo(String),
    SetSearch(bool),
    Search(String),
    SearchActivate,
    OpenHit(i32),
    ToggleMhc,
    MhcClosed,
    ToggleTsk,
    TskClosed,
    OpenTskXref(i32),
    OpenTskDest(Ref),
    ClickWord(i32),
    ToggleDict,
    DictClosed,
    DictSearch(String),
    DictModule,
    DictOpen(i32),
    Back,
    Forward,
    CopyVerses,
    FontSmaller,
    FontLarger,
    SetInterlinear(bool),
    SetParagraphs(bool),
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
                    set_title_widget = &adw::WindowTitle {
                        #[watch]
                        set_title: &model.title,
                        set_subtitle: "King James Version",
                    },
                    pack_start = &gtk::Button {
                        set_icon_name: "go-previous-symbolic",
                        set_tooltip_text: Some("Previous chapter (Alt+Left)"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: !model.search_open,
                        connect_clicked => Msg::PrevChapter,
                    },
                    pack_start = &gtk::Button {
                        set_icon_name: "go-next-symbolic",
                        set_tooltip_text: Some("Next chapter (Alt+Right)"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: !model.search_open,
                        connect_clicked => Msg::NextChapter,
                    },
                    pack_start = &gtk::Button {
                        set_label: "Back",
                        set_tooltip_text: Some("Back in history"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: model.history.can_back() && !model.search_open,
                        connect_clicked => Msg::Back,
                    },
                    pack_start = &gtk::Button {
                        set_label: "Forward",
                        set_tooltip_text: Some("Forward in history"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: model.history.can_forward() && !model.search_open,
                        connect_clicked => Msg::Forward,
                    },
                    pack_end = &gtk::ToggleButton {
                        set_label: "Dict",
                        set_tooltip_text: Some("Dictionaries and topics (opens a second window)"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_active: model.dict.is_some(),
                        #[watch]
                        set_sensitive: model.error.is_none(),
                        connect_clicked => Msg::ToggleDict,
                    },
                    pack_end = &gtk::ToggleButton {
                        set_label: "TSK",
                        set_tooltip_text: Some("Treasury of Scripture Knowledge (opens a second window)"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_active: model.tsk.is_some(),
                        #[watch]
                        set_sensitive: model.error.is_none(),
                        connect_clicked => Msg::ToggleTsk,
                    },
                    pack_end = &gtk::ToggleButton {
                        set_label: "MHC",
                        set_tooltip_text: Some("Matthew Henry (opens a second window)"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_active: model.mhc.is_some(),
                        #[watch]
                        set_sensitive: model.error.is_none(),
                        connect_clicked => Msg::ToggleMhc,
                    },
                    pack_end = &gtk::ToggleButton {
                        set_label: "Interlinear",
                        set_tooltip_text: Some("Show Strong's lemmas beside tagged words"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_active: model.interlinear,
                        #[watch]
                        set_sensitive: model.error.is_none() && !model.search_open,
                        connect_clicked[sender] => move |btn| {
                            sender.input(Msg::SetInterlinear(btn.is_active()));
                        }
                    },
                    pack_end = &gtk::ToggleButton {
                        set_label: "Paragraphs",
                        set_tooltip_text: Some("Flow consecutive verses as prose"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_active: model.paragraphs,
                        #[watch]
                        set_sensitive: model.error.is_none() && !model.search_open,
                        connect_clicked[sender] => move |btn| {
                            sender.input(Msg::SetParagraphs(btn.is_active()));
                        }
                    },
                    pack_end = &gtk::Button {
                        set_icon_name: "edit-copy-symbolic",
                        set_tooltip_text: Some("Copy current verse with citation"),
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: model.error.is_none() && !model.search_open,
                        connect_clicked => Msg::CopyVerses,
                    },
                    pack_end = &gtk::Button {
                        set_icon_name: "zoom-out-symbolic",
                        set_tooltip_text: Some("Smaller text (Ctrl+-)"),
                        set_valign: gtk::Align::Center,
                        connect_clicked => Msg::FontSmaller,
                    },
                    pack_end = &gtk::Button {
                        set_icon_name: "zoom-in-symbolic",
                        set_tooltip_text: Some("Larger text (Ctrl++)"),
                        set_valign: gtk::Align::Center,
                        connect_clicked => Msg::FontLarger,
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
                    #[name(goto_entry)]
                    pack_end = &gtk::Entry {
                        set_placeholder_text: Some("John 3:16"),
                        set_tooltip_text: Some("Go to a reference, then press Enter (Ctrl+L)"),
                        set_width_chars: 18,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: !model.search_open,
                        connect_activate[sender] => move |entry| {
                            sender.input(Msg::GoTo(entry.text().to_string()));
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
                    gtk::Overlay {
                        add_overlay = &gtk::Revealer {
                            #[watch]
                            set_reveal_child: model.search_open,
                            set_transition_type: gtk::RevealerTransitionType::SlideDown,
                            set_halign: gtk::Align::Fill,
                            set_valign: gtk::Align::Start,
                            set_hexpand: true,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                add_css_class: "background",

                                adw::Clamp {
                                    set_maximum_size: 720,
                                    set_tightening_threshold: 480,

                                    gtk::Box {
                                        set_orientation: gtk::Orientation::Vertical,
                                        set_spacing: 8,
                                        set_margin_start: 16,
                                        set_margin_end: 16,
                                        set_margin_top: 12,
                                        set_margin_bottom: 12,

                                        #[local_ref]
                                        search_entry -> gtk::SearchEntry {
                                            set_placeholder_text: Some("Search the KJV"),
                                            set_tooltip_text: Some("Search the King James Version"),
                                            set_hexpand: true,
                                            connect_search_changed[sender] => move |entry| {
                                                sender.input(Msg::Search(entry.text().to_string()));
                                            },
                                            connect_activate => Msg::SearchActivate,
                                            connect_stop_search => Msg::SetSearch(false),
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
                                            set_label: search::empty_description(&model.search_query)
                                                .unwrap_or(""),
                                            #[watch]
                                            set_visible: search::empty_description(&model.search_query)
                                                .is_some(),
                                            set_xalign: 0.0,
                                            set_wrap: true,
                                            add_css_class: "dim-label",
                                        },

                                        gtk::ScrolledWindow {
                                            #[watch]
                                            set_visible: !model.search_hits.is_empty(),
                                            set_hexpand: true,
                                            set_propagate_natural_height: true,
                                            set_max_content_height: 360,
                                            set_policy: (
                                                gtk::PolicyType::Never,
                                                gtk::PolicyType::Automatic,
                                            ),

                                            #[local_ref]
                                            search_list -> gtk::ListBox {
                                                set_selection_mode: gtk::SelectionMode::Single,
                                                add_css_class: "boxed-list",
                                                set_accessible_role: gtk::AccessibleRole::List,
                                                connect_row_activated[sender] => move |_, row| {
                                                    sender.input(Msg::OpenHit(row.index()));
                                                }
                                            }
                                        }
                                    }
                                },

                                gtk::Separator {
                                    set_orientation: gtk::Orientation::Horizontal,
                                }
                            }
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_width_request: 200,
                                add_css_class: "sidebar",

                                gtk::ScrolledWindow {
                                    set_vexpand: true,
                                    set_hscrollbar_policy: gtk::PolicyType::Never,
                                    set_vscrollbar_policy: gtk::PolicyType::Automatic,

                                    #[local_ref]
                                    book_list -> gtk::ListBox {
                                        set_selection_mode: gtk::SelectionMode::Single,
                                        add_css_class: "navigation-sidebar",
                                        set_accessible_role: gtk::AccessibleRole::List,
                                        connect_row_activated[sender] => move |_, row| {
                                            if let Some(id) = sidebar::book_id_from_row(row) {
                                                sender.input(Msg::SelectBook(id));
                                            }
                                        }
                                    },
                                },

                                gtk::Separator {
                                    set_orientation: gtk::Orientation::Horizontal,
                                },

                                gtk::ScrolledWindow {
                                    set_propagate_natural_height: true,
                                    set_max_content_height: 220,
                                    set_hscrollbar_policy: gtk::PolicyType::Never,
                                    set_vscrollbar_policy: gtk::PolicyType::Automatic,

                                    #[local_ref]
                                    chapter_grid -> gtk::FlowBox {
                                        set_selection_mode: gtk::SelectionMode::Single,
                                        set_min_children_per_line: 5,
                                        set_max_children_per_line: 6,
                                        set_column_spacing: 2,
                                        set_row_spacing: 2,
                                        set_homogeneous: false,
                                        set_halign: gtk::Align::Fill,
                                        set_valign: gtk::Align::Start,
                                        set_activate_on_single_click: true,
                                        add_css_class: "chapter-grid",
                                        connect_child_activated[sender] => move |_, child| {
                                            if let Some(ch) = sidebar::chapter_from_child(child) {
                                                sender.input(Msg::SelectChapter(ch));
                                            }
                                        }
                                    },
                                },
                            },

                            gtk::Separator {
                                set_orientation: gtk::Orientation::Vertical,
                            },

                            gtk::ScrolledWindow {
                                set_hexpand: true,
                                set_vexpand: true,
                                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                                #[local_ref]
                                chapter_view -> gtk::TextView {
                                    set_buffer: Some(&model.buffer),
                                    set_editable: false,
                                    set_cursor_visible: false,
                                    set_wrap_mode: gtk::WrapMode::WordChar,
                                    set_left_margin: 28,
                                    set_right_margin: 28,
                                    set_top_margin: 20,
                                    set_bottom_margin: 24,
                                    set_pixels_above_lines: 1,
                                    set_pixels_below_lines: 1,
                                    set_has_tooltip: true,
                                    add_css_class: "chapter-view",
                                    set_accessible_role: gtk::AccessibleRole::Document,
                                }
                            }
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
        let book_list = gtk::ListBox::new();
        let chapter_grid = gtk::FlowBox::new();
        let search_list = gtk::ListBox::new();
        let search_entry = gtk::SearchEntry::new();
        let chapter_view = gtk::TextView::new();
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.tag_table().add(&strongs::make_tag());
        let italic = gtk::TextTag::new(Some("italic"));
        italic.set_style(gtk::pango::Style::Italic);
        buffer.tag_table().add(&italic);
        let verse_num = gtk::TextTag::new(Some("verse-num"));
        verse_num.set_weight(700);
        verse_num.set_scale(0.75);
        verse_num.set_rise(gtk::pango::SCALE * 4);
        verse_num.set_foreground(Some("#9a9996"));
        buffer.tag_table().add(&verse_num);
        let note = gtk::TextTag::new(Some("note"));
        note.set_style(gtk::pango::Style::Italic);
        note.set_foreground(Some("#77767b"));
        note.set_scale(0.85);
        buffer.tag_table().add(&note);
        let note_mark = gtk::TextTag::new(Some("note-mark"));
        note_mark.set_foreground(Some("#77767b"));
        note_mark.set_scale(0.7);
        note_mark.set_rise(4 * gtk::pango::SCALE);
        buffer.tag_table().add(&note_mark);
        let apparatus = gtk::TextTag::new(Some("apparatus"));
        apparatus.set_foreground(Some("#9a9996"));
        apparatus.set_scale(0.8);
        apparatus.set_left_margin(44);
        apparatus.set_pixels_above_lines(2);
        buffer.tag_table().add(&apparatus);
        let xref = gtk::TextTag::new(Some("xref"));
        xref.set_underline(gtk::pango::Underline::Single);
        xref.set_foreground(Some("#1c71d8"));
        xref.set_scale(0.8);
        buffer.tag_table().add(&xref);
        let mhc_tag = gtk::TextTag::new(Some("mhc-sup"));
        mhc_tag.set_weight(700);
        mhc_tag.set_foreground(Some("#1c71d8"));
        mhc_tag.set_scale(0.7);
        mhc_tag.set_rise(4 * gtk::pango::SCALE);
        buffer.tag_table().add(&mhc_tag);
        let tsk_sup = gtk::TextTag::new(Some("tsk-sup"));
        tsk_sup.set_foreground(Some("#1c71d8"));
        tsk_sup.set_scale(0.7);
        tsk_sup.set_rise(4 * 1024);
        buffer.tag_table().add(&tsk_sup);
        let lemma = gtk::TextTag::new(Some("lemma"));
        lemma.set_foreground(Some("#77767b"));
        lemma.set_scale(0.85);
        buffer.tag_table().add(&lemma);
        let current_verse = gtk::TextTag::new(Some("current-verse"));
        current_verse.set_background(Some("#eceae6"));
        current_verse.set_background_full_height(true);
        buffer.tag_table().add(&current_verse);
        current_verse.set_priority(0);
        apparatus.set_priority(0);
        note.set_priority(0);
        verse_num.set_priority(1);
        italic.set_priority(1);
        lemma.set_priority(1);
        xref.set_priority(2);
        mhc_tag.set_priority(2);
        tsk_sup.set_priority(2);
        note_mark.set_priority(2);
        let strongs_popover = strongs::create(&chapter_view);
        let tsk_popover = tsk::create_popover(&chapter_view);
        let font_provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &font_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let (conn, books, at, font_size, interlinear, paragraphs, error) = match load_library() {
            Ok((conn, books, at, font_size, interlinear, paragraphs)) => (
                Some(conn),
                books,
                at,
                font_size,
                interlinear,
                paragraphs,
                None,
            ),
            Err(e) => (
                None,
                Vec::new(),
                Ref {
                    book: 1,
                    chapter: 1,
                    verse: 1,
                },
                layout::DEFAULT_FONT,
                false,
                true,
                Some(e),
            ),
        };

        let mut model = App {
            history: History::new(at),
            conn,
            books,
            at,
            title: "bible-app".into(),
            error,
            buffer,
            book_list: book_list.clone(),
            chapter_grid: chapter_grid.clone(),
            chapter_view: chapter_view.clone(),
            search_open: false,
            search_query: String::new(),
            search_hits: Vec::new(),
            search_status: search::status("", 0, bible_app_db::DEFAULT_LIMIT),
            search_list: search_list.clone(),
            search_entry: search_entry.clone(),
            mhc: None,
            tsk: None,
            dict: None,
            strongs_popover,
            tsk_popover,
            layout: ChapterLayout::default(),
            font_size,
            interlinear,
            paragraphs,
            font_provider,
            xref_tips: Rc::new(RefCell::new(Vec::new())),
        };
        model.apply_font();
        model.refresh_chapter(false);

        sidebar::install_css();
        sidebar::fill_books(&model.book_list, &model.books);
        sidebar::select_book(&model.book_list, model.at.book);
        let n = model
            .conn
            .as_ref()
            .and_then(|c| bible_app_db::max_chapter(c, model.at.book).ok())
            .unwrap_or(1);
        sidebar::sync_chapters(&model.chapter_grid, model.at.book, n, model.at.chapter);

        let widgets = view_output!();

        let key = gtk::EventControllerKey::new();
        let goto = widgets.goto_entry.clone();
        let sender_keys = sender.clone();
        key.connect_key_pressed(move |_, keyval, _, mods| {
            let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let alt = mods.contains(gtk::gdk::ModifierType::ALT_MASK);
            if ctrl && (keyval == gtk::gdk::Key::f || keyval == gtk::gdk::Key::F) {
                sender_keys.input(Msg::SetSearch(true));
                return glib::Propagation::Stop;
            }
            if keyval == gtk::gdk::Key::Escape {
                sender_keys.input(Msg::SetSearch(false));
                return glib::Propagation::Stop;
            }
            if ctrl && (keyval == gtk::gdk::Key::l || keyval == gtk::gdk::Key::L) {
                goto.grab_focus();
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
            glib::Propagation::Proceed
        });
        root.add_controller(key);

        let tips = model.xref_tips.clone();
        model
            .chapter_view
            .connect_query_tooltip(move |view, x, y, keyboard, tooltip| {
                let offset = if keyboard {
                    view.buffer().cursor_position()
                } else {
                    let (bx, by) = view.window_to_buffer_coords(gtk::TextWindowType::Widget, x, y);
                    match view.iter_at_location(bx, by) {
                        Some(iter) => iter.offset(),
                        None => return false,
                    }
                };
                let tips = tips.borrow();
                if let Some((_, _, text)) =
                    tips.iter().find(|(s, e, _)| offset >= *s && offset < *e)
                {
                    tooltip.set_text(Some(text));
                    true
                } else {
                    false
                }
            });

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

        let click = gtk::GestureClick::new();
        click.set_button(1);
        let view = model.chapter_view.clone();
        let click_sender = sender.clone();
        click.connect_released(move |_, _, x, y| {
            let (bx, by) =
                view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            if let Some(iter) = view.iter_at_location(bx, by) {
                click_sender.input(Msg::ClickWord(iter.offset()));
            }
        });
        model.chapter_view.add_controller(click);

        let motion = gtk::EventControllerMotion::new();
        let view = model.chapter_view.clone();
        let tips = model.xref_tips.clone();
        motion.connect_motion(move |_, x, y| {
            let (bx, by) =
                view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            let over = view
                .iter_at_location(bx, by)
                .map(|iter| {
                    let off = iter.offset();
                    tips.borrow().iter().any(|(s, e, _)| off >= *s && off < *e)
                })
                .unwrap_or(false);
            let cursor = if over {
                gtk::gdk::Cursor::from_name("pointer", None)
            } else {
                None
            };
            view.set_cursor(cursor.as_ref());
        });
        let view = model.chapter_view.clone();
        motion.connect_leave(move |_| {
            view.set_cursor(None);
        });
        model.chapter_view.add_controller(motion);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::SelectBook(id) => {
                self.go(
                    Ref {
                        book: id,
                        chapter: 1,
                        verse: 1,
                    },
                    false,
                );
            }
            Msg::SelectChapter(chapter) => {
                self.go(
                    Ref {
                        book: self.at.book,
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
                    if let Ok(at) = nav::prev_chapter(conn, &self.books, self.at) {
                        self.go(at, false);
                    }
                }
            }
            Msg::NextChapter => {
                if self.search_open {
                    return;
                }
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::next_chapter(conn, &self.books, self.at) {
                        self.go(at, false);
                    }
                }
            }
            Msg::GoTo(text) => {
                if text.is_empty() {
                    return;
                }
                if let Some(at) = nav::parse_ref(&text, &self.books, self.at) {
                    self.search_open = false;
                    self.go(at, true);
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
                self.search_open = open;
                if open {
                    self.focus_search();
                }
            }
            Msg::Search(query) => {
                self.run_search(query);
            }
            Msg::SearchActivate => {
                let idx = self
                    .search_list
                    .selected_row()
                    .map(|r| r.index())
                    .unwrap_or(0);
                self.open_hit(idx);
            }
            Msg::OpenHit(idx) => {
                self.open_hit(idx);
            }
            Msg::ToggleMhc => {
                if let Some(widgets) = self.mhc.take() {
                    widgets.window.close();
                } else if self.error.is_none() {
                    let widgets = mhc::open(sender.input_sender().clone(), self.at, &self.books);
                    if let Some(conn) = &self.conn {
                        mhc::fill(&widgets, conn, &self.books, self.at);
                    }
                    self.mhc = Some(widgets);
                }
            }
            Msg::MhcClosed => {
                self.mhc = None;
            }
            Msg::ToggleTsk => {
                if let Some(widgets) = self.tsk.take() {
                    widgets.window.close();
                } else if self.error.is_none() {
                    let mut widgets =
                        tsk::open(sender.input_sender().clone(), self.at, &self.books);
                    if let Some(conn) = &self.conn {
                        tsk::fill(&mut widgets, conn, &self.books, self.at);
                    }
                    self.tsk = Some(widgets);
                }
            }
            Msg::TskClosed => {
                self.tsk = None;
            }
            Msg::OpenTskXref(idx) => {
                let Some(widgets) = &self.tsk else { return };
                let Some(at) = tsk::xref_at(widgets, idx) else {
                    return;
                };
                self.search_open = false;
                self.go(at, true);
            }
            Msg::OpenTskDest(at) => {
                self.tsk_popover.popdown();
                self.search_open = false;
                self.go(at, true);
            }
            Msg::ClickWord(offset) => {
                self.tsk_popover.popdown();
                let tsk_hit = self
                    .tsk_at(offset)
                    .map(|m| (m.span.start, m.heading.clone(), m.dests.clone()));
                if let Some((start, heading, dests)) = tsk_hit {
                    self.strongs_popover.popdown();
                    tsk::present_phrase(
                        &self.tsk_popover,
                        &self.chapter_view,
                        start,
                        &heading,
                        &dests,
                        &self.books,
                        sender.input_sender().clone(),
                    );
                } else if let Some(at) = self.xref_at(offset) {
                    self.go(at, true);
                } else if let Some(verse) = self.mhc_at(offset) {
                    self.go(Ref { verse, ..self.at }, false);
                    self.ensure_mhc(&sender);
                } else if let Some((start, text)) =
                    self.note_at(offset).map(|n| (n.span.start, n.text.clone()))
                {
                    strongs::present_text(&self.strongs_popover, &self.chapter_view, start, &text);
                } else if layout::word_at_offset(&self.layout.words, offset).is_some() {
                    if let Some(verse) = layout::verse_at_offset(&self.layout.verse_start, offset) {
                        self.select_verse(verse);
                    }
                    self.open_strongs(offset);
                } else if let Some(verse) =
                    layout::verse_at_offset(&self.layout.verse_start, offset)
                {
                    self.strongs_popover.popdown();
                    self.select_verse(verse);
                }
            }
            Msg::ToggleDict => {
                if let Some(widgets) = self.dict.take() {
                    widgets.window.close();
                } else if self.error.is_none() {
                    let mut widgets = dict::open(sender.input_sender().clone());
                    if let Some(conn) = &self.conn {
                        dict::load_modules(&mut widgets, conn);
                        dict::search(&mut widgets, conn);
                    }
                    self.dict = Some(widgets);
                }
            }
            Msg::DictClosed => {
                self.dict = None;
            }
            Msg::DictSearch(query) => {
                let Some(widgets) = &mut self.dict else {
                    return;
                };
                widgets.query = query;
                if let Some(conn) = &self.conn {
                    dict::search(widgets, conn);
                }
            }
            Msg::DictModule => {
                let Some(widgets) = &mut self.dict else {
                    return;
                };
                if let Some(conn) = &self.conn {
                    dict::search(widgets, conn);
                }
            }
            Msg::DictOpen(idx) => {
                let Some(widgets) = &self.dict else { return };
                if let Some(conn) = &self.conn {
                    dict::open_hit(widgets, conn, idx);
                }
            }
            Msg::Back => {
                if self.search_open {
                    return;
                }
                if let Some(at) = self.history.back() {
                    self.at = at;
                    self.refresh_chapter(true);
                    self.sync_book_row();
                }
            }
            Msg::Forward => {
                if self.search_open {
                    return;
                }
                if let Some(at) = self.history.forward() {
                    self.at = at;
                    self.refresh_chapter(true);
                    self.sync_book_row();
                }
            }
            Msg::CopyVerses => {
                self.copy_current_verse();
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
            Msg::SetInterlinear(_on) => {
                // Header toggle stays; it must not paint lemmas into the chapter.
            }
            Msg::SetParagraphs(on) => {
                if self.paragraphs == on {
                    return;
                }
                self.paragraphs = on;
                self.save_state();
                if !self.search_open {
                    self.refresh_chapter(false);
                }
            }
        }
    }
}

impl App {
    fn run_search(&mut self, query: String) {
        self.search_query = query;
        self.search_hits = match &self.conn {
            Some(conn) if !self.search_query.trim().is_empty() => {
                bible_app_db::search_verses(conn, &self.search_query, bible_app_db::DEFAULT_LIMIT)
                    .unwrap_or_default()
            }
            _ => Vec::new(),
        };
        self.search_status = search::status(
            &self.search_query,
            self.search_hits.len(),
            bible_app_db::DEFAULT_LIMIT,
        );
        search::refill_list(&self.search_list, &self.search_hits, &self.books);
    }

    fn open_hit(&mut self, idx: i32) {
        let Ok(idx) = usize::try_from(idx) else {
            return;
        };
        let Some(hit) = self.search_hits.get(idx) else {
            return;
        };
        let at = search::hit_ref(hit);
        self.search_open = false;
        self.go(at, true);
    }

    fn focus_search(&self) {
        let entry = self.search_entry.clone();
        glib::timeout_add_local_once(Duration::from_millis(100), move || {
            entry.grab_focus();
        });
    }

    fn go(&mut self, at: Ref, highlight: bool) {
        self.history.navigate(at);
        self.at = at;
        self.refresh_chapter(highlight);
        self.sync_book_row();
    }

    fn save_state(&self) {
        config::save_state(&config::State::from_ref(
            self.at,
            self.font_size,
            false,
            self.paragraphs,
        ));
    }

    fn apply_font(&self) {
        self.font_provider.load_from_string(&format!(
            "textview.chapter-view {{ font-size: {}pt; }}",
            self.font_size
        ));
    }

    fn xref_at(&self, offset: i32) -> Option<Ref> {
        self.layout
            .xrefs
            .iter()
            .find(|l| l.span.contains(offset))
            .map(|l| l.at)
    }

    fn mhc_at(&self, offset: i32) -> Option<u8> {
        self.layout
            .mhc
            .iter()
            .find(|m| m.span.contains(offset))
            .map(|m| m.verse)
    }

    fn tsk_at(&self, offset: i32) -> Option<&layout::TskMark> {
        self.layout.tsk.iter().find(|m| m.span.contains(offset))
    }

    fn note_at(&self, offset: i32) -> Option<&layout::NoteMark> {
        self.layout.notes.iter().find(|n| n.span.contains(offset))
    }

    fn ensure_mhc(&mut self, sender: &ComponentSender<Self>) {
        if self.mhc.is_none() && self.error.is_none() {
            let widgets = mhc::open(sender.input_sender().clone(), self.at, &self.books);
            self.mhc = Some(widgets);
        }
        self.refresh_mhc();
    }

    fn copy_current_verse(&self) {
        let Some(conn) = &self.conn else { return };
        let Ok(verse) = bible_app_db::get_verse(conn, self.at.book, self.at.chapter, self.at.verse)
        else {
            return;
        };
        let text = layout::copy_verses(&self.books, &[verse]);
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_text(&text);
        }
    }

    fn refresh_chapter(&mut self, highlight: bool) {
        let Some(conn) = &self.conn else {
            self.title = "bible-app".into();
            return;
        };
        self.title = nav::format_chapter(&self.books, self.at.book, self.at.chapter);
        let verses = match bible_app_db::chapter(conn, self.at.book, self.at.chapter) {
            Ok(verses) if !verses.is_empty() => verses,
            Ok(_) => {
                self.layout = ChapterLayout {
                    text: "No verses in this chapter.".into(),
                    ..ChapterLayout::default()
                };
                self.buffer.set_text(&self.layout.text);
                self.xref_tips.borrow_mut().clear();
                self.save_state();
                return;
            }
            Err(e) => {
                self.layout = ChapterLayout {
                    text: e.to_string(),
                    ..ChapterLayout::default()
                };
                self.buffer.set_text(&self.layout.text);
                self.xref_tips.borrow_mut().clear();
                self.save_state();
                return;
            }
        };
        let tsk_rows = bible_app_db::chapter_resources(conn, "TSK", self.at.book, self.at.chapter)
            .unwrap_or_default();
        let tsk_notes: Vec<(u8, &str)> = tsk_rows
            .iter()
            .map(|r| (r.verse, r.text.as_str()))
            .collect();
        let mhc_starts =
            bible_app_db::chapter_resource_verses(conn, "MHC", self.at.book, self.at.chapter)
                .unwrap_or_default();
        let words =
            bible_app_db::chapter_words(conn, self.at.book, self.at.chapter).unwrap_or_default();
        self.layout = layout::layout_chapter(
            &verses,
            &self.books,
            &tsk_notes,
            &mhc_starts,
            &words,
            &[],
            layout::LayoutOpts {
                interlinear: false,
                paragraphs: self.paragraphs,
            },
        );
        self.strongs_popover.popdown();
        self.tsk_popover.popdown();
        self.apply_layout_tags();
        self.fill_xref_tips(conn);
        self.save_state();
        if highlight {
            self.highlight_verse(self.at.verse);
        } else {
            self.apply_current_verse_tag(self.at.verse);
            self.scroll_to_top();
        }
        self.refresh_mhc();
        self.refresh_tsk();
    }

    fn apply_layout_tags(&self) {
        self.buffer.set_text(&self.layout.text);
        for span in &self.layout.verse_nums {
            self.apply_tag("verse-num", *span);
        }
        for mark in &self.layout.notes {
            self.apply_tag("note-mark", mark.span);
        }
        for span in &self.layout.apparatus {
            self.apply_tag("apparatus", *span);
        }
        for span in &self.layout.italics {
            self.apply_tag("italic", *span);
        }
        for link in &self.layout.xrefs {
            self.apply_tag("xref", link.span);
        }
        for mark in &self.layout.mhc {
            self.apply_tag("mhc-sup", mark.span);
        }
        for mark in &self.layout.tsk {
            self.apply_tag("tsk-sup", mark.span);
        }
        for word in &self.layout.words {
            self.apply_tag("strongs", word.span);
            if let Some(span) = word.lemma_span {
                self.apply_tag("lemma", span);
            }
        }
    }

    fn apply_tag(&self, name: &str, span: layout::Span) {
        let Some(tag) = self.buffer.tag_table().lookup(name) else {
            return;
        };
        if span.end <= span.start {
            return;
        }
        let s = self.buffer.iter_at_offset(span.start);
        let e = self.buffer.iter_at_offset(span.end);
        self.buffer.apply_tag(&tag, &s, &e);
    }

    fn fill_xref_tips(&self, conn: &Connection) {
        let mut tips = Vec::new();
        for mark in &self.layout.tsk {
            let dests: Vec<bible_app_db::Xref> = mark
                .dests
                .iter()
                .map(|d| bible_app_db::Xref {
                    book: d.book,
                    chapter: d.chapter,
                    verse: d.verse,
                })
                .collect();
            let (line, _, hidden) = layout::format_xref_line(&dests, &self.books);
            let extra = if hidden > 0 {
                format!(" · {hidden} more")
            } else {
                String::new()
            };
            let text = if line.is_empty() {
                mark.heading.clone()
            } else {
                format!("{}\n{line}{extra}", mark.heading)
            };
            tips.push((mark.span.start, mark.span.end, text));
        }
        for link in &self.layout.xrefs {
            let preview =
                match bible_app_db::get_verse(conn, link.at.book, link.at.chapter, link.at.verse) {
                    Ok(v) => {
                        let (stored, _) = layout::split_notes(&v.text);
                        let (text, _) = layout::strip_supplied(&stored);
                        format!("{}\n{}", nav::format_ref(&self.books, link.at), text)
                    }
                    Err(_) => nav::format_ref(&self.books, link.at),
                };
            tips.push((link.span.start, link.span.end, preview));
        }
        for mark in &self.layout.mhc {
            tips.push((mark.span.start, mark.span.end, "Open Matthew Henry".into()));
        }
        for mark in &self.layout.notes {
            tips.push((mark.span.start, mark.span.end, mark.text.clone()));
        }
        for word in &self.layout.words {
            tips.push((word.span.start, word.span.end, "Click for Strong's".into()));
        }
        *self.xref_tips.borrow_mut() = tips;
    }

    fn refresh_mhc(&self) {
        let Some(widgets) = &self.mhc else { return };
        let Some(conn) = &self.conn else { return };
        mhc::fill(widgets, conn, &self.books, self.at);
    }

    fn refresh_tsk(&mut self) {
        let Some(conn) = &self.conn else { return };
        let books = self.books.clone();
        let at = self.at;
        if let Some(widgets) = &mut self.tsk {
            tsk::fill(widgets, conn, &books, at);
        }
    }

    fn select_verse(&mut self, verse: u8) {
        if self.at.verse != verse {
            self.at.verse = verse;
            self.save_state();
            self.refresh_mhc();
            self.refresh_tsk();
        }
        self.apply_current_verse_tag(verse);
    }

    fn open_strongs(&self, offset: i32) {
        let Some(word) = layout::word_at_offset(&self.layout.words, offset) else {
            return;
        };
        let Some(conn) = &self.conn else { return };
        let defs: Vec<_> = word
            .codes
            .iter()
            .filter_map(|c| bible_app_db::lookup_strongs(conn, c).ok().flatten())
            .collect();
        if defs.is_empty() {
            return;
        }
        self.tsk_popover.popdown();
        strongs::present(
            &self.strongs_popover,
            &self.chapter_view,
            word.span.start,
            &defs,
        );
    }

    fn highlight_verse(&self, verse: u8) {
        self.apply_current_verse_tag(verse);
        let Some((_, offset)) = self.layout.verse_start.iter().find(|(v, _)| *v == verse) else {
            return;
        };
        let offset = *offset;
        let view = self.chapter_view.clone();
        let buffer = self.buffer.clone();
        glib::idle_add_local_once(move || {
            let mut iter = buffer.iter_at_offset(offset);
            view.scroll_to_iter(&mut iter, 0.15, true, 0.0, 0.2);
        });
    }

    fn apply_current_verse_tag(&self, verse: u8) {
        let Some(tag) = self.buffer.tag_table().lookup("current-verse") else {
            return;
        };
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.remove_tag(&tag, &start, &end);
        let text_end = end.offset();
        let Some(span) = layout::verse_range(&self.layout.verse_start, verse, text_end) else {
            return;
        };
        self.apply_tag("current-verse", span);
    }

    fn scroll_to_top(&self) {
        let start = self.buffer.start_iter();
        self.buffer.place_cursor(&start);
        let view = self.chapter_view.clone();
        glib::idle_add_local_once(move || {
            let buffer = view.buffer();
            let mut iter = buffer.start_iter();
            view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.0);
        });
    }

    fn sync_book_row(&self) {
        let list = self.book_list.clone();
        let grid = self.chapter_grid.clone();
        let book = self.at.book;
        let chapter = self.at.chapter;
        let n = self
            .conn
            .as_ref()
            .and_then(|c| bible_app_db::max_chapter(c, book).ok())
            .unwrap_or(1);
        glib::idle_add_local_once(move || {
            sidebar::select_book(&list, book);
            sidebar::sync_chapters(&grid, book, n, chapter);
        });
    }
}

type LoadedLibrary = (Connection, Vec<Book>, Ref, i32, bool, bool);

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
    let interlinear = false;
    let paragraphs = state.paragraphs;
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
    Ok((conn, books, at, font_size, interlinear, paragraphs))
}
