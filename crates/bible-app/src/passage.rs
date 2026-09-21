use crate::history::History;
use crate::layout::{self, ChapterLayout};
use crate::marks;
use crate::nav::Ref;
use crate::strongs;
use crate::tsk;
use crate::user_db;
use crate::workspace::TabId;
use adw::prelude::*;
use bible_app_db::Book;
use gtk::gio;
use gtk::glib;
use relm4::{adw, gtk};
use rusqlite::Connection;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct ChapterCtx<'a> {
    pub conn: &'a Connection,
    pub books: &'a [Book],
    pub user: Option<&'a Connection>,
    pub paragraphs: bool,
}

pub struct PassageView {
    pub at: Ref,
    pub history: History,
    pub buffer: gtk::TextBuffer,
    pub view: gtk::TextView,
    pub root: gtk::ScrolledWindow,
    pub layout: ChapterLayout,
    pub xref_tips: Rc<RefCell<Vec<(i32, i32, String)>>>,
    pub strongs_popover: gtk::Popover,
    pub tsk_popover: gtk::Popover,
    pub verse_menu: gtk::PopoverMenu,
    pub chapter_marks: HashMap<u8, user_db::VerseMarks>,
    pub strongs_at: i32,
}

impl PassageView {
    pub fn new(at: Ref, actions: &gio::SimpleActionGroup) -> Self {
        let buffer = gtk::TextBuffer::new(None::<&gtk::TextTagTable>);
        install_buffer_tags(&buffer);
        let view = gtk::TextView::new();
        view.set_buffer(Some(&buffer));
        view.set_editable(false);
        view.set_cursor_visible(false);
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_left_margin(28);
        view.set_right_margin(28);
        view.set_top_margin(20);
        view.set_bottom_margin(24);
        view.set_pixels_above_lines(1);
        view.set_pixels_below_lines(1);
        view.set_has_tooltip(true);
        view.add_css_class("chapter-view");
        view.set_accessible_role(gtk::AccessibleRole::Document);
        view.set_hexpand(true);
        view.set_vexpand(true);

        let root = gtk::ScrolledWindow::new();
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        root.set_child(Some(&view));

        let strongs_popover = strongs::create(&view);
        let tsk_popover = tsk::create_popover(&view);
        let verse_menu = gtk::PopoverMenu::from_model(None::<&gio::MenuModel>);
        verse_menu.set_parent(&view);
        verse_menu.set_has_arrow(false);
        verse_menu.set_halign(gtk::Align::Start);
        verse_menu.insert_action_group("win", Some(actions));
        let strongs_on_destroy = strongs_popover.clone();
        let tsk_on_destroy = tsk_popover.clone();
        let verse_on_destroy = verse_menu.clone();
        view.connect_destroy(move |_| {
            strongs_on_destroy.unparent();
            tsk_on_destroy.unparent();
            verse_on_destroy.unparent();
        });

        Self {
            history: History::new(at),
            at,
            buffer,
            view,
            root,
            layout: ChapterLayout::default(),
            xref_tips: Rc::new(RefCell::new(Vec::new())),
            strongs_popover,
            tsk_popover,
            verse_menu,
            chapter_marks: HashMap::new(),
            strongs_at: 0,
        }
    }

    pub fn wire(&self, id: TabId, sender: relm4::Sender<crate::app::Msg>) {
        let tips = self.xref_tips.clone();
        self.view
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

        let click = gtk::GestureClick::new();
        click.set_button(1);
        let view = self.view.clone();
        let click_sender = sender.clone();
        click.connect_released(move |_, _, x, y| {
            let (bx, by) =
                view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            if let Some(iter) = view.iter_at_location(bx, by) {
                click_sender.emit(crate::app::Msg::ClickWord {
                    id,
                    offset: iter.offset(),
                });
            }
        });
        self.view.add_controller(click);

        let right = gtk::GestureClick::new();
        right.set_button(gtk::gdk::BUTTON_SECONDARY);
        right.set_propagation_phase(gtk::PropagationPhase::Capture);
        let view = self.view.clone();
        let right_sender = sender.clone();
        right.connect_pressed(move |g, _, x, y| {
            g.set_state(gtk::EventSequenceState::Claimed);
            let (bx, by) =
                view.window_to_buffer_coords(gtk::TextWindowType::Widget, x as i32, y as i32);
            let offset = view
                .iter_at_location(bx, by)
                .map(|iter| iter.offset())
                .unwrap_or(-1);
            right_sender.emit(crate::app::Msg::VerseContext {
                id,
                offset,
                x: x as i32,
                y: y as i32,
            });
        });
        self.view.add_controller(right);

        let copy_keys = gtk::EventControllerKey::new();
        copy_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let copy_key_sender = sender.clone();
        copy_keys.connect_key_pressed(move |_, keyval, _, mods| {
            let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            if ctrl && (keyval == gtk::gdk::Key::c || keyval == gtk::gdk::Key::C) {
                copy_key_sender.emit(crate::app::Msg::CopyVerses);
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        self.view.add_controller(copy_keys);

        let motion = gtk::EventControllerMotion::new();
        let view = self.view.clone();
        let tips = self.xref_tips.clone();
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
        let view = self.view.clone();
        motion.connect_leave(move |_| {
            view.set_cursor(None);
        });
        self.view.add_controller(motion);
    }

    pub fn load(&mut self, ctx: &ChapterCtx<'_>, highlight: bool) {
        let verses = match bible_app_db::chapter(ctx.conn, self.at.book, self.at.chapter) {
            Ok(verses) if !verses.is_empty() => verses,
            Ok(_) => {
                self.layout = ChapterLayout {
                    text: "No verses in this chapter.".into(),
                    ..ChapterLayout::default()
                };
                self.buffer.set_text(&self.layout.text);
                self.xref_tips.borrow_mut().clear();
                return;
            }
            Err(e) => {
                self.layout = ChapterLayout {
                    text: e.to_string(),
                    ..ChapterLayout::default()
                };
                self.buffer.set_text(&self.layout.text);
                self.xref_tips.borrow_mut().clear();
                return;
            }
        };
        let tsk_rows =
            bible_app_db::chapter_resources(ctx.conn, "TSK", self.at.book, self.at.chapter)
                .unwrap_or_default();
        let tsk_notes: Vec<(u8, &str)> = tsk_rows
            .iter()
            .map(|r| (r.verse, r.text.as_str()))
            .collect();
        let mhc_starts =
            bible_app_db::chapter_resource_verses(ctx.conn, "MHC", self.at.book, self.at.chapter)
                .unwrap_or_default();
        let words = bible_app_db::chapter_words(ctx.conn, self.at.book, self.at.chapter)
            .unwrap_or_default();
        self.layout = layout::layout_chapter(
            &verses,
            ctx.books,
            &tsk_notes,
            &mhc_starts,
            &words,
            &[],
            layout::LayoutOpts {
                interlinear: false,
                paragraphs: ctx.paragraphs,
            },
        );
        self.strongs_popover.popdown();
        self.tsk_popover.popdown();
        self.apply_layout_tags();
        self.chapter_marks = ctx
            .user
            .and_then(|u| user_db::chapter_marks(u, self.at.book, self.at.chapter).ok())
            .unwrap_or_default();
        marks::apply_tags(&self.buffer, &self.layout, &self.chapter_marks);
        self.fill_xref_tips(ctx.conn, ctx.books);
        if highlight {
            self.highlight_verse(self.at.verse);
        } else {
            self.apply_current_verse_tag(self.at.verse);
            self.scroll_to_top();
        }
    }

    pub fn apply_layout_tags(&self) {
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
            self.apply_tag("mhc-num", mark.span);
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

    pub fn apply_tag(&self, name: &str, span: layout::Span) {
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

    pub fn fill_xref_tips(&self, conn: &Connection, books: &[Book]) {
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
            let (line, _, hidden) = layout::format_xref_line(&dests, books);
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
                        format!("{}\n{}", crate::nav::format_ref(books, link.at), text)
                    }
                    Err(_) => crate::nav::format_ref(books, link.at),
                };
            tips.push((link.span.start, link.span.end, preview));
        }
        for (span, (verse, _)) in self
            .layout
            .verse_nums
            .iter()
            .zip(self.layout.verse_start.iter())
        {
            let mut parts = Vec::new();
            if self.layout.mhc.iter().any(|m| m.verse == *verse) {
                parts.push("Open Matthew Henry".to_string());
            }
            if let Some(m) = self.chapter_marks.get(verse) {
                if m.bookmark {
                    parts.push("Bookmarked".into());
                }
                if m.highlight.is_some() {
                    parts.push("Highlighted".into());
                }
                if m.note {
                    parts.push("Has a note".into());
                }
            }
            if !parts.is_empty() {
                tips.push((span.start, span.end, parts.join(" · ")));
            }
        }
        for mark in &self.layout.notes {
            tips.push((mark.span.start, mark.span.end, mark.text.clone()));
        }
        for word in &self.layout.words {
            tips.push((
                word.span.start,
                word.span.end,
                "Click for Strong's and dictionaries".into(),
            ));
        }
        *self.xref_tips.borrow_mut() = tips;
    }

    pub fn xref_at(&self, offset: i32) -> Option<Ref> {
        self.layout
            .xrefs
            .iter()
            .find(|l| l.span.contains(offset))
            .map(|l| l.at)
    }

    pub fn mhc_at(&self, offset: i32) -> Option<u8> {
        self.layout
            .mhc
            .iter()
            .find(|m| m.span.contains(offset))
            .map(|m| m.verse)
    }

    pub fn tsk_at(&self, offset: i32) -> Option<&layout::TskMark> {
        self.layout.tsk.iter().find(|m| m.span.contains(offset))
    }

    pub fn note_at(&self, offset: i32) -> Option<&layout::NoteMark> {
        self.layout.notes.iter().find(|n| n.span.contains(offset))
    }

    pub fn select_verse(&mut self, verse: u8) -> bool {
        let changed = self.at.verse != verse;
        if changed {
            self.at.verse = verse;
        }
        self.apply_current_verse_tag(verse);
        changed
    }

    pub fn highlight_verse(&self, verse: u8) {
        self.apply_current_verse_tag(verse);
        let Some((_, offset)) = self.layout.verse_start.iter().find(|(v, _)| *v == verse) else {
            return;
        };
        let offset = *offset;
        let view = self.view.clone();
        let buffer = self.buffer.clone();
        gtk::glib::idle_add_local_once(move || {
            let mut iter = buffer.iter_at_offset(offset);
            view.scroll_to_iter(&mut iter, 0.15, true, 0.0, 0.2);
        });
    }

    pub fn apply_current_verse_tag(&self, verse: u8) {
        let Some(tag) = self.buffer.tag_table().lookup("current-verse") else {
            return;
        };
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.remove_tag(&tag, &start, &end);
        let Some(span) = marks::verse_num_span(&self.layout, verse) else {
            return;
        };
        self.apply_tag("current-verse", span);
    }

    pub fn scroll_to_top(&self) {
        let start = self.buffer.start_iter();
        self.buffer.place_cursor(&start);
        let view = self.view.clone();
        gtk::glib::idle_add_local_once(move || {
            let buffer = view.buffer();
            let mut iter = buffer.start_iter();
            view.scroll_to_iter(&mut iter, 0.0, true, 0.0, 0.0);
        });
    }

    pub fn selected_verse_range(&self) -> Option<(u8, u8)> {
        let (start, end) = self.buffer.selection_bounds()?;
        layout::verses_in_selection(&self.layout.verse_start, start.offset(), end.offset())
    }

    pub fn reload_marks(
        &mut self,
        user: Option<&Connection>,
        books: &[Book],
        conn: Option<&Connection>,
    ) {
        self.chapter_marks = user
            .and_then(|u| user_db::chapter_marks(u, self.at.book, self.at.chapter).ok())
            .unwrap_or_default();
        marks::apply_tags(&self.buffer, &self.layout, &self.chapter_marks);
        if let Some(conn) = conn {
            self.fill_xref_tips(conn, books);
        }
    }

    pub fn popdown(&self) {
        self.strongs_popover.popdown();
        self.tsk_popover.popdown();
        self.verse_menu.popdown();
    }
}

pub fn install_buffer_tags(buffer: &gtk::TextBuffer) {
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
    let mhc_tag = gtk::TextTag::new(Some("mhc-num"));
    mhc_tag.set_weight(700);
    mhc_tag.set_foreground(Some("#1c71d8"));
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
    current_verse.set_foreground(Some("#99c1f1"));
    current_verse.set_weight(700);
    buffer.tag_table().add(&current_verse);
    marks::install_tags(buffer);
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
}
