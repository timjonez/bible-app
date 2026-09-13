use crate::config;
use crate::dict;
use crate::mhc;
use crate::nav::{self, Ref};
use crate::search;
use crate::strongs;
use crate::tsk;
use adw::prelude::*;
use bible_app_db::{self, Book, SearchHit, Verse};
use gtk::glib;
use relm4::prelude::*;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::time::Duration;

pub struct App {
    conn: Option<Connection>,
    books: Vec<Book>,
    at: Ref,
    title: String,
    chapter_text: String,
    error: Option<String>,
    buffer: gtk::TextBuffer,
    book_list: gtk::ListBox,
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
    chapter_words: Vec<TaggedWord>,
    strongs_popover: gtk::Popover,
}

struct TaggedWord {
    start: i32,
    end: i32,
    codes: Vec<String>,
}

#[derive(Debug)]
pub enum Msg {
    SelectBook(u8),
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
    ClickWord(i32),
    ToggleDict,
    DictClosed,
    DictSearch(String),
    DictModule,
    DictOpen(i32),
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
                    pack_end = &gtk::ToggleButton {
                        set_label: "Dict",
                        set_tooltip_text: Some("Public-domain dictionaries (opens a second window)"),
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
                } else if model.search_open {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
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
                        },

                        gtk::Label {
                            #[watch]
                            set_label: &model.search_status,
                            set_xalign: 0.0,
                            add_css_class: "dim-label",
                        },

                        if model.search_hits.is_empty() {
                            adw::StatusPage {
                                set_icon_name: Some("edit-find-symbolic"),
                                #[watch]
                                set_title: &model.search_status,
                                #[watch]
                                set_description: search::empty_description(&model.search_query),
                                set_vexpand: true,
                            }
                        } else {
                            gtk::ScrolledWindow {
                                set_hexpand: true,
                                set_vexpand: true,
                                set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),

                                adw::Clamp {
                                    set_maximum_size: 720,
                                    set_tightening_threshold: 480,

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
                        }
                    }
                } else {
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,

                        gtk::ScrolledWindow {
                            set_width_request: 200,
                            set_propagate_natural_width: true,
                            add_css_class: "sidebar",

                            #[local_ref]
                            book_list -> gtk::ListBox {
                                set_selection_mode: gtk::SelectionMode::Single,
                                add_css_class: "navigation-sidebar",
                                set_accessible_role: gtk::AccessibleRole::List,
                                connect_row_activated[sender] => move |_, row| {
                                    let id = u8::try_from(row.index() + 1).unwrap_or(1);
                                    sender.input(Msg::SelectBook(id));
                                }
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
                                set_left_margin: 20,
                                set_right_margin: 20,
                                set_top_margin: 16,
                                set_bottom_margin: 16,
                                set_pixels_above_lines: 2,
                                set_pixels_below_lines: 2,
                                set_tooltip_text: Some("Click an underlined word for Strong's"),
                                set_accessible_role: gtk::AccessibleRole::Document,
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
        let search_list = gtk::ListBox::new();
        let search_entry = gtk::SearchEntry::new();
        let chapter_view = gtk::TextView::new();
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        buffer.tag_table().add(&strongs::make_tag());
        let strongs_popover = strongs::create(&chapter_view);

        let (conn, books, at, error) = match load_library() {
            Ok((conn, books, at)) => (Some(conn), books, at, None),
            Err(e) => (
                None,
                Vec::new(),
                Ref {
                    book: 1,
                    chapter: 1,
                    verse: 1,
                },
                Some(e),
            ),
        };

        let mut model = App {
            conn,
            books,
            at,
            title: "bible-app".into(),
            chapter_text: String::new(),
            error,
            buffer,
            book_list: book_list.clone(),
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
            chapter_words: Vec::new(),
            strongs_popover,
        };
        model.refresh_chapter(false);

        for book in &model.books {
            let label = gtk::Label::new(Some(&book.name));
            label.set_xalign(0.0);
            label.set_margin_start(8);
            label.set_margin_end(8);
            label.set_margin_top(4);
            label.set_margin_bottom(4);
            let row = gtk::ListBoxRow::new();
            row.set_child(Some(&label));
            row.set_tooltip_text(Some(&book.name));
            model.book_list.append(&row);
        }
        if let Some(row) = model.book_list.row_at_index(i32::from(model.at.book) - 1) {
            model.book_list.select_row(Some(&row));
        }

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
            glib::Propagation::Proceed
        });
        root.add_controller(key);

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

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            Msg::SelectBook(id) => {
                self.at = Ref {
                    book: id,
                    chapter: 1,
                    verse: 1,
                };
                self.refresh_chapter(false);
            }
            Msg::PrevChapter => {
                if self.search_open {
                    return;
                }
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::prev_chapter(conn, &self.books, self.at) {
                        self.at = at;
                        self.refresh_chapter(false);
                        self.sync_book_row();
                    }
                }
            }
            Msg::NextChapter => {
                if self.search_open {
                    return;
                }
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::next_chapter(conn, &self.books, self.at) {
                        self.at = at;
                        self.refresh_chapter(false);
                        self.sync_book_row();
                    }
                }
            }
            Msg::GoTo(text) => {
                if text.is_empty() {
                    return;
                }
                if let Some(at) = nav::parse_ref(&text, &self.books, self.at) {
                    self.at = at;
                    self.search_open = false;
                    self.refresh_chapter(true);
                    self.sync_book_row();
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
                    self.title = "Search".into();
                    self.focus_search();
                } else {
                    self.refresh_chapter(false);
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
                self.at = at;
                self.search_open = false;
                self.refresh_chapter(true);
                self.sync_book_row();
            }
            Msg::ClickWord(offset) => {
                self.open_strongs(offset);
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
        self.at = search::hit_ref(hit);
        self.search_open = false;
        self.refresh_chapter(true);
        self.sync_book_row();
    }

    fn focus_search(&self) {
        let entry = self.search_entry.clone();
        glib::timeout_add_local_once(Duration::from_millis(100), move || {
            entry.grab_focus();
        });
    }

    fn refresh_chapter(&mut self, highlight: bool) {
        let verses = {
            let Some(conn) = &self.conn else {
                self.title = "bible-app".into();
                return;
            };
            self.title = nav::format_chapter(&self.books, self.at.book, self.at.chapter);
            match bible_app_db::chapter(conn, self.at.book, self.at.chapter) {
                Ok(verses) if !verses.is_empty() => {
                    self.chapter_text = nav::format_chapter_text(&verses);
                    verses
                }
                Ok(_) => {
                    self.chapter_text = "No verses in this chapter.".into();
                    Vec::new()
                }
                Err(e) => {
                    self.chapter_text = e.to_string();
                    Vec::new()
                }
            }
        };
        self.strongs_popover.popdown();
        self.buffer.set_text(&self.chapter_text);
        self.tag_strongs(&verses);
        config::save_state(self.at);
        if highlight {
            self.highlight_verse(self.at.verse);
        } else {
            self.scroll_to_top();
        }
        self.refresh_mhc();
        self.refresh_tsk();
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

    fn tag_strongs(&mut self, verses: &[Verse]) {
        self.chapter_words.clear();
        let Some(tag) = self.buffer.tag_table().lookup("strongs") else {
            return;
        };
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.remove_tag(&tag, &start, &end);
        let Some(conn) = &self.conn else { return };
        let Ok(words) = bible_app_db::chapter_words(conn, self.at.book, self.at.chapter) else {
            return;
        };
        let starts = nav::verse_body_offsets(verses);
        for w in words {
            let Some((_, body)) = starts.iter().find(|(v, _)| *v == w.verse) else {
                continue;
            };
            let start = *body + w.start;
            let end = *body + w.end;
            if start < 0 || end <= start {
                continue;
            }
            let s = self.buffer.iter_at_offset(start);
            let e = self.buffer.iter_at_offset(end);
            self.buffer.apply_tag(&tag, &s, &e);
            self.chapter_words.push(TaggedWord {
                start,
                end,
                codes: w
                    .strongs
                    .split_whitespace()
                    .map(ToString::to_string)
                    .collect(),
            });
        }
    }

    fn open_strongs(&self, offset: i32) {
        let Some(word) = self
            .chapter_words
            .iter()
            .find(|w| offset >= w.start && offset < w.end)
        else {
            self.strongs_popover.popdown();
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
        strongs::present(&self.strongs_popover, &self.chapter_view, word.start, &defs);
    }

    fn highlight_verse(&self, verse: u8) {
        let needle = format!("{verse}  ");
        let start = self.buffer.start_iter();
        let Some((match_start, _)) =
            start.forward_search(&needle, gtk::TextSearchFlags::TEXT_ONLY, None)
        else {
            return;
        };
        let mut match_end = match_start;
        match_end.forward_to_line_end();
        self.buffer.select_range(&match_start, &match_end);
        let offset = match_start.offset();
        let view = self.chapter_view.clone();
        let buffer = self.buffer.clone();
        glib::idle_add_local_once(move || {
            let mut iter = buffer.iter_at_offset(offset);
            view.scroll_to_iter(&mut iter, 0.15, true, 0.0, 0.2);
        });
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
        let idx = i32::from(self.at.book) - 1;
        glib::idle_add_local_once(move || {
            if let Some(row) = list.row_at_index(idx) {
                list.select_row(Some(&row));
            }
        });
    }
}

fn load_library() -> Result<(Connection, Vec<Book>, Ref), String> {
    let path = config::find_database().ok_or_else(config::import_hint)?;
    let conn = bible_app_db::open(&path).map_err(|e| {
        format!(
            "Could not open {}:\n{e}\n\n{}",
            path.display(),
            config::import_hint()
        )
    })?;
    let books = bible_app_db::books(&conn).map_err(|e| e.to_string())?;
    if books.is_empty() {
        return Err("The database has no books. Re-run the importer.".into());
    }
    let mut at = Ref::from(config::load_state());
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
    Ok((conn, books, at))
}
