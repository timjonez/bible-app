use crate::history::History;
use crate::layout::{self, ChapterLayout};
use crate::marks;
use crate::nav::Ref;
use crate::strongs;
use crate::theme;
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
    /// Buffer selection captured when the verse menu opens.
    pub menu_sel: Option<(i32, i32)>,
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
            menu_sel: None,
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
                if m.highlight().is_some() {
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
                "Click for Strong's, lexicons, dictionaries, and topics".into(),
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

    pub fn paint_search(&self, verse: u8, tokens: &[String], strongs: Option<&str>) {
        let Some(tag) = self.buffer.tag_table().lookup("search-hit") else {
            return;
        };
        let start = self.buffer.start_iter();
        let end = self.buffer.end_iter();
        self.buffer.remove_tag(&tag, &start, &end);
        if let Some(code) = strongs {
            for word in &self.layout.words {
                if word.codes.iter().any(|c| c.eq_ignore_ascii_case(code)) {
                    self.apply_tag("search-hit", word.span);
                }
            }
            return;
        }
        let Some(&(_, body)) = self.layout.verse_body.iter().find(|(v, _)| *v == verse) else {
            return;
        };
        let Some(&(_, vend)) = self.layout.verse_end.iter().find(|(v, _)| *v == verse) else {
            return;
        };
        let skip = mark_spans(&self.layout);
        for span in token_spans(&self.layout.text, body, vend, tokens, &skip) {
            self.apply_tag("search-hit", span);
        }
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

    pub fn capture_menu_sel(&mut self) {
        self.menu_sel = self
            .buffer
            .selection_bounds()
            .map(|(s, e)| (s.offset(), e.offset()))
            .filter(|(s, e)| e > s);
    }

    pub fn selected_highlight_spans(&self) -> Vec<(u8, i32, i32)> {
        let (start, end) = if let Some(sel) = self.menu_sel {
            sel
        } else {
            let Some((s, e)) = self.buffer.selection_bounds() else {
                return Vec::new();
            };
            (s.offset(), e.offset())
        };
        layout::selection_highlights(&self.layout, start, end)
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
    buffer.tag_table().add(&verse_num);
    let note = gtk::TextTag::new(Some("note"));
    note.set_style(gtk::pango::Style::Italic);
    note.set_scale(0.85);
    buffer.tag_table().add(&note);
    let note_mark = gtk::TextTag::new(Some("note-mark"));
    note_mark.set_scale(0.7);
    note_mark.set_rise(4 * gtk::pango::SCALE);
    buffer.tag_table().add(&note_mark);
    let apparatus = gtk::TextTag::new(Some("apparatus"));
    apparatus.set_scale(0.8);
    apparatus.set_left_margin(44);
    apparatus.set_pixels_above_lines(2);
    buffer.tag_table().add(&apparatus);
    let xref = gtk::TextTag::new(Some("xref"));
    xref.set_underline(gtk::pango::Underline::Single);
    xref.set_scale(0.8);
    buffer.tag_table().add(&xref);
    let mhc_tag = gtk::TextTag::new(Some("mhc-num"));
    mhc_tag.set_weight(700);
    buffer.tag_table().add(&mhc_tag);
    let tsk_sup = gtk::TextTag::new(Some("tsk-sup"));
    tsk_sup.set_scale(0.7);
    tsk_sup.set_rise(4 * 1024);
    buffer.tag_table().add(&tsk_sup);
    let lemma = gtk::TextTag::new(Some("lemma"));
    lemma.set_scale(0.85);
    buffer.tag_table().add(&lemma);
    let current_verse = gtk::TextTag::new(Some("current-verse"));
    current_verse.set_weight(700);
    buffer.tag_table().add(&current_verse);
    let search_hit = gtk::TextTag::new(Some("search-hit"));
    buffer.tag_table().add(&search_hit);
    marks::install_tags(buffer);
    theme::paint_buffer(buffer);
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
    search_hit.set_priority(3);
}

fn mark_spans(layout: &layout::ChapterLayout) -> Vec<layout::Span> {
    let mut spans = Vec::new();
    spans.extend(layout.tsk.iter().map(|m| m.span));
    spans.extend(layout.notes.iter().map(|m| m.span));
    spans.extend(layout.mhc.iter().map(|m| m.span));
    spans.extend(layout.words.iter().filter_map(|w| w.lemma_span));
    spans
}

/// Cross-reference letters and note daggers sit in the text right after a word.
/// They are not part of the word, so a search for Moses still matches Mosesᵃ.
fn token_spans(
    text: &str,
    start: i32,
    end: i32,
    tokens: &[String],
    skip: &[layout::Span],
) -> Vec<layout::Span> {
    let chars: Vec<char> = text.chars().collect();
    let start = start.max(0) as usize;
    let end = (end.max(0) as usize).min(chars.len());
    if start >= end {
        return Vec::new();
    }
    let slice: String = chars[start..end].iter().collect();
    let lower = slice.to_lowercase();
    if lower.len() != slice.len() {
        return Vec::new();
    }
    let mut spans = Vec::new();
    for token in tokens {
        let needle = token.to_lowercase();
        if needle.is_empty() {
            continue;
        }
        let mut from = 0;
        while let Some(rel) = lower[from..].find(&needle) {
            let pos = from + rel;
            let char_at = lower[..pos].chars().count();
            let char_end = char_at + needle.chars().count();
            let abs = (start + char_at) as i32;
            let abs_end = (start + char_end) as i32;
            let before = neighbor_char(&chars, abs as isize - 1, skip, false);
            let after = neighbor_char(&chars, abs_end as isize, skip, true);
            if before.is_none_or(|c| !c.is_ascii_alphanumeric())
                && after.is_none_or(|c| !c.is_ascii_alphanumeric())
            {
                spans.push(layout::Span {
                    start: abs,
                    end: abs_end,
                });
            }
            from = pos + needle.len().max(1);
        }
    }
    spans
}

fn neighbor_char(
    chars: &[char],
    mut idx: isize,
    skip: &[layout::Span],
    forward: bool,
) -> Option<char> {
    loop {
        if idx < 0 || idx as usize >= chars.len() {
            return None;
        }
        let pos = idx as i32;
        if let Some(span) = skip.iter().find(|s| s.contains(pos)) {
            idx = if forward {
                span.end as isize
            } else {
                span.start as isize - 1
            };
            continue;
        }
        return Some(chars[idx as usize]);
    }
}

#[cfg(test)]
mod token_span_tests {
    use super::*;

    #[test]
    fn search_matches_a_word_before_a_cross_reference_letter() {
        let text = "And Mosesa feared";
        let skip = [layout::Span { start: 9, end: 10 }];
        let spans = token_spans(
            text,
            0,
            text.chars().count() as i32,
            &["moses".into()],
            &skip,
        );
        assert_eq!(
            spans,
            vec![layout::Span { start: 4, end: 9 }],
            "Moses sits at 4..9, before the superscript a"
        );
    }
}
