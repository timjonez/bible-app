use crate::config;
use crate::nav::{self, Ref};
use adw::prelude::*;
use bible_app_db::{self, Book};
use gtk::glib;
use relm4::prelude::*;
use relm4::{adw, gtk};
use rusqlite::Connection;

pub struct App {
    conn: Option<Connection>,
    books: Vec<Book>,
    at: Ref,
    title: String,
    chapter_text: String,
    error: Option<String>,
    buffer: gtk::TextBuffer,
    book_list: gtk::ListBox,
}

#[derive(Debug)]
pub enum Msg {
    SelectBook(u8),
    PrevChapter,
    NextChapter,
    GoTo(String),
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
                        connect_clicked => Msg::PrevChapter,
                    },
                    pack_start = &gtk::Button {
                        set_icon_name: "go-next-symbolic",
                        set_tooltip_text: Some("Next chapter (Alt+Right)"),
                        set_valign: gtk::Align::Center,
                        connect_clicked => Msg::NextChapter,
                    },
                    #[name(goto_entry)]
                    pack_end = &gtk::Entry {
                        set_placeholder_text: Some("John 3:16"),
                        set_tooltip_text: Some("Go to a reference, then press Enter (Ctrl+L)"),
                        set_width_chars: 18,
                        set_valign: gtk::Align::Center,
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

                            gtk::TextView {
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
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);

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
        };
        model.refresh_chapter();

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

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            Msg::SelectBook(id) => {
                self.at = Ref {
                    book: id,
                    chapter: 1,
                    verse: 1,
                };
                self.refresh_chapter();
            }
            Msg::PrevChapter => {
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::prev_chapter(conn, &self.books, self.at) {
                        self.at = at;
                        self.refresh_chapter();
                        self.sync_book_row();
                    }
                }
            }
            Msg::NextChapter => {
                if let Some(conn) = &self.conn {
                    if let Ok(at) = nav::next_chapter(conn, &self.books, self.at) {
                        self.at = at;
                        self.refresh_chapter();
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
                    self.refresh_chapter();
                    self.sync_book_row();
                }
            }
        }
    }
}

impl App {
    fn refresh_chapter(&mut self) {
        let Some(conn) = &self.conn else {
            self.title = "bible-app".into();
            return;
        };
        self.title = nav::format_chapter(&self.books, self.at.book, self.at.chapter);
        match bible_app_db::chapter(conn, self.at.book, self.at.chapter) {
            Ok(verses) if !verses.is_empty() => {
                self.chapter_text = nav::format_chapter_text(&verses);
            }
            Ok(_) => {
                self.chapter_text = "No verses in this chapter.".into();
            }
            Err(e) => {
                self.chapter_text = e.to_string();
            }
        }
        self.buffer.set_text(&self.chapter_text);
        config::save_state(self.at);
    }

    fn sync_book_row(&self) {
        if let Some(row) = self.book_list.row_at_index(i32::from(self.at.book) - 1) {
            self.book_list.select_row(Some(&row));
        }
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
