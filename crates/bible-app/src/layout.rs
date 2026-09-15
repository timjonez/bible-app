use crate::nav::Ref;
use bible_app_db::{Book, Verse, VerseWord, Xref};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: i32,
    pub end: i32,
}

impl Span {
    pub fn contains(self, offset: i32) -> bool {
        offset >= self.start && offset < self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrefLink {
    pub span: Span,
    pub at: Ref,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MhcMark {
    pub span: Span,
    pub verse: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSpan {
    pub span: Span,
    pub lemma_span: Option<Span>,
    pub codes: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChapterLayout {
    pub text: String,
    pub italics: Vec<Span>,
    pub xrefs: Vec<XrefLink>,
    pub mhc: Vec<MhcMark>,
    pub words: Vec<WordSpan>,
    pub verse_nums: Vec<Span>,
    pub notes: Vec<Span>,
    pub apparatus: Vec<Span>,
    pub verse_body: Vec<(u8, i32)>,
    pub verse_start: Vec<(u8, i32)>,
}

/// Pull trailing `{...}` translator notes off a stored KJV verse.
pub fn split_notes(text: &str) -> (String, Vec<String>) {
    let mut notes = Vec::new();
    let mut rest = text.trim_end().to_string();
    while let Some(open) = rest.rfind('{') {
        if !rest[open..].ends_with('}') {
            break;
        }
        let note = rest[open + 1..rest.len() - 1].trim();
        if note.is_empty() {
            break;
        }
        notes.push(note.to_string());
        rest.truncate(open);
        let keep = rest.trim_end().len();
        rest.truncate(keep);
    }
    notes.reverse();
    (rest, notes)
}

/// Drop `[` `]` around KJV supplied words; return display text and italic spans.
pub fn strip_supplied(text: &str) -> (String, Vec<Span>) {
    let mut out = String::new();
    let mut italics = Vec::new();
    let mut italic_start: Option<i32> = None;
    for c in text.chars() {
        if c == '[' {
            italic_start = Some(char_len(&out));
            continue;
        }
        if c == ']' {
            if let Some(s) = italic_start.take() {
                let e = char_len(&out);
                if e > s {
                    italics.push(Span { start: s, end: e });
                }
            }
            continue;
        }
        out.push(c);
    }
    if let Some(s) = italic_start {
        let e = char_len(&out);
        if e > s {
            italics.push(Span { start: s, end: e });
        }
    }
    (out, italics)
}

/// Map a stored (bracket-inclusive) half-open span onto display text after `strip_supplied`.
pub fn map_stored_span(start: i32, end: i32, source: &str) -> Option<Span> {
    let s = stored_to_display(source, start);
    let e = stored_to_display(source, end);
    if e > s {
        Some(Span { start: s, end: e })
    } else {
        None
    }
}

fn stored_to_display(source: &str, stored: i32) -> i32 {
    let mut d = 0i32;
    for (i, c) in source.chars().enumerate() {
        if i as i32 >= stored {
            return d;
        }
        if c != '[' && c != ']' {
            d += 1;
        }
    }
    d
}

pub fn format_xref_line(xrefs: &[Xref], books: &[Book]) -> (String, Vec<XrefLink>) {
    let mut text = String::new();
    let mut links = Vec::new();
    let mut last_book: Option<u8> = None;
    for (book, chapter, verses) in xref_clusters(xrefs) {
        if !text.is_empty() {
            text.push_str("; ");
        }
        if last_book != Some(book) {
            text.push_str(book_abbrev(books, book));
            text.push(' ');
            last_book = Some(book);
        }
        text.push_str(&format!("{chapter}:"));
        for (i, run) in verse_runs(&verses).into_iter().enumerate() {
            if i > 0 {
                text.push(',');
            }
            let label = if run.0 == run.1 {
                format!("{}", run.0)
            } else {
                format!("{}-{}", run.0, run.1)
            };
            let start = char_len(&text);
            text.push_str(&label);
            let end = char_len(&text);
            links.push(XrefLink {
                span: Span { start, end },
                at: Ref {
                    book,
                    chapter,
                    verse: run.0,
                },
                label,
            });
        }
    }
    (text, links)
}

fn xref_clusters(xrefs: &[Xref]) -> Vec<(u8, u8, Vec<u8>)> {
    let mut clusters: Vec<(u8, u8, Vec<u8>)> = Vec::new();
    for x in xrefs {
        if let Some((book, chapter, verses)) = clusters.last_mut() {
            if *book == x.book && *chapter == x.chapter {
                verses.push(x.verse);
                continue;
            }
        }
        clusters.push((x.book, x.chapter, vec![x.verse]));
    }
    clusters
}

fn verse_runs(verses: &[u8]) -> Vec<(u8, u8)> {
    let mut vs = verses.to_vec();
    vs.sort_unstable();
    vs.dedup();
    let mut out = Vec::new();
    let mut i = 0;
    while i < vs.len() {
        let start = vs[i];
        let mut end = start;
        i += 1;
        while i < vs.len() && vs[i] == end + 1 {
            end = vs[i];
            i += 1;
        }
        out.push((start, end));
    }
    out
}

fn book_abbrev(books: &[Book], id: u8) -> &str {
    books
        .iter()
        .find(|b| b.id == id)
        .map(|b| b.abbrev.as_str())
        .unwrap_or("Bk")
}

pub fn layout_chapter(
    verses: &[Verse],
    books: &[Book],
    xrefs: &[(u8, Xref)],
    mhc_starts: &[u8],
    words: &[VerseWord],
    lemmas: &[(String, String)],
    interlinear: bool,
) -> ChapterLayout {
    let mut layout = ChapterLayout::default();
    for v in verses {
        if !layout.text.is_empty() {
            layout.text.push('\n');
            layout.text.push('\n');
        }

        let verse_start = char_len(&layout.text);
        let num = format!("{}", v.verse);
        layout.text.push_str(&num);
        layout.verse_nums.push(Span {
            start: verse_start,
            end: char_len(&layout.text),
        });
        layout.text.push(' ');
        if v.para_break {
            layout.text.push('¶');
            layout.text.push(' ');
        }
        let body_start = char_len(&layout.text);

        let (stored_body, note_texts) = split_notes(&v.text);
        let verse_words: Vec<VerseWord> = words
            .iter()
            .filter(|w| w.verse == v.verse)
            .cloned()
            .collect();
        let (body, italics, word_spans) =
            build_body(&stored_body, &verse_words, lemmas, interlinear);
        layout.text.push_str(&body);

        for ital in italics {
            layout.italics.push(shift_span(ital, body_start));
        }
        for mut w in word_spans {
            w.span = shift_span(w.span, body_start);
            if let Some(ls) = w.lemma_span.as_mut() {
                *ls = shift_span(*ls, body_start);
            }
            layout.words.push(w);
        }
        layout.verse_start.push((v.verse, verse_start));
        layout.verse_body.push((v.verse, body_start));

        if !note_texts.is_empty() {
            layout.text.push('\n');
            let s = char_len(&layout.text);
            layout.text.push_str(&note_texts.join("  "));
            layout.notes.push(Span {
                start: s,
                end: char_len(&layout.text),
            });
        }

        let dests: Vec<Xref> = xrefs
            .iter()
            .filter(|(from, _)| *from == v.verse)
            .map(|(_, x)| *x)
            .collect();
        let (xref_text, xref_links) = format_xref_line(&dests, books);
        let has_mhc = mhc_starts.contains(&v.verse);
        if xref_text.is_empty() && !has_mhc {
            continue;
        }
        layout.text.push('\n');
        let line_start = char_len(&layout.text);
        if !xref_text.is_empty() {
            layout.text.push_str(&xref_text);
            for mut link in xref_links {
                link.span = shift_span(link.span, line_start);
                layout.xrefs.push(link);
            }
        }
        if has_mhc {
            if !xref_text.is_empty() {
                layout.text.push(' ');
            }
            let s = char_len(&layout.text);
            layout.text.push_str("MHC");
            layout.mhc.push(MhcMark {
                span: Span {
                    start: s,
                    end: char_len(&layout.text),
                },
                verse: v.verse,
            });
        }
        layout.apparatus.push(Span {
            start: line_start,
            end: char_len(&layout.text),
        });
    }
    layout
}

fn build_body(
    stored: &str,
    words: &[VerseWord],
    lemmas: &[(String, String)],
    interlinear: bool,
) -> (String, Vec<Span>, Vec<WordSpan>) {
    let (stripped, italics) = strip_supplied(stored);
    let mut word_spans: Vec<WordSpan> = Vec::new();
    for w in words {
        let Some(span) = map_stored_span(w.start, w.end, stored) else {
            continue;
        };
        let codes: Vec<String> = w
            .strongs
            .split_whitespace()
            .map(ToString::to_string)
            .collect();
        word_spans.push(WordSpan {
            span,
            lemma_span: None,
            codes,
        });
    }
    word_spans.sort_by_key(|w| w.span.start);
    if !interlinear {
        return (stripped, italics, word_spans);
    }

    let mut out = String::new();
    let mut new_words = Vec::new();
    let mut idx = 0i32;
    for w in &word_spans {
        out.push_str(&slice_chars(&stripped, idx, w.span.start));
        let eng_start = char_len(&out);
        out.push_str(&slice_chars(&stripped, w.span.start, w.span.end));
        let eng_end = char_len(&out);
        let lemma_span = lemma_of(&w.codes, lemmas).map(|lem| {
            let ins = format!(" ({lem})");
            let ls = char_len(&out);
            out.push_str(&ins);
            Span {
                start: ls,
                end: char_len(&out),
            }
        });
        new_words.push(WordSpan {
            span: Span {
                start: eng_start,
                end: eng_end,
            },
            lemma_span,
            codes: w.codes.clone(),
        });
        idx = w.span.end;
    }
    out.push_str(&slice_chars(&stripped, idx, char_len(&stripped)));

    let inserts: Vec<(i32, i32)> = word_spans
        .iter()
        .zip(&new_words)
        .filter_map(|(ow, nw)| nw.lemma_span.map(|ls| (ow.span.end, ls.end - ls.start)))
        .collect();
    let new_italics = italics
        .into_iter()
        .filter_map(|ital| {
            let s = shift_by_inserts(ital.start, &inserts);
            let e = shift_by_inserts(ital.end, &inserts);
            (e > s).then_some(Span { start: s, end: e })
        })
        .collect();
    (out, new_italics, new_words)
}

fn lemma_of(codes: &[String], lemmas: &[(String, String)]) -> Option<String> {
    for code in codes {
        if let Some((_, lem)) = lemmas.iter().find(|(c, _)| c == code) {
            if !lem.is_empty() {
                return Some(lem.clone());
            }
        }
    }
    None
}

fn shift_by_inserts(off: i32, inserts: &[(i32, i32)]) -> i32 {
    off + inserts
        .iter()
        .filter(|(at, _)| *at < off)
        .map(|(_, n)| *n)
        .sum::<i32>()
}

fn shift_span(span: Span, by: i32) -> Span {
    Span {
        start: span.start + by,
        end: span.end + by,
    }
}

fn slice_chars(s: &str, start: i32, end: i32) -> String {
    let n = (end - start).max(0) as usize;
    s.chars().skip(start.max(0) as usize).take(n).collect()
}

fn char_len(s: &str) -> i32 {
    s.chars().count() as i32
}

/// Current-verse (or range) copy with a `Book chapter:verse` citation and `KJV`.
pub fn copy_verses(books: &[Book], verses: &[Verse]) -> String {
    if verses.is_empty() {
        return String::new();
    }
    let body: Vec<String> = verses
        .iter()
        .map(|v| {
            let (stored, _) = split_notes(&v.text);
            let (text, _) = strip_supplied(&stored);
            if verses.len() == 1 {
                text
            } else {
                format!("{} {text}", v.verse)
            }
        })
        .collect();
    let first = &verses[0];
    let last = verses.last().unwrap();
    let name = books
        .iter()
        .find(|b| b.id == first.book)
        .map(|b| b.name.as_str())
        .unwrap_or("Book");
    let cite =
        if first.book == last.book && first.chapter == last.chapter && first.verse == last.verse {
            format!("{name} {}:{}", first.chapter, first.verse)
        } else if first.book == last.book && first.chapter == last.chapter {
            format!("{name} {}:{}–{}", first.chapter, first.verse, last.verse)
        } else {
            format!(
                "{name} {}:{}–{}:{}",
                first.chapter, first.verse, last.chapter, last.verse
            )
        };
    format!("{}\n\n{cite} (KJV)", body.join("\n\n"))
}

pub fn smaller_font(pt: i32) -> i32 {
    (pt - 1).clamp(MIN_FONT, MAX_FONT)
}

pub fn larger_font(pt: i32) -> i32 {
    (pt + 1).clamp(MIN_FONT, MAX_FONT)
}

pub const MIN_FONT: i32 = 10;
pub const MAX_FONT: i32 = 28;
pub const DEFAULT_FONT: i32 = 14;

#[cfg(test)]
mod tests {
    use super::*;

    fn books() -> Vec<Book> {
        vec![
            Book {
                id: 1,
                abbrev: "Ge".into(),
                name: "Genesis".into(),
            },
            Book {
                id: 2,
                abbrev: "Ex".into(),
                name: "Exodus".into(),
            },
            Book {
                id: 4,
                abbrev: "Nu".into(),
                name: "Numbers".into(),
            },
            Book {
                id: 6,
                abbrev: "Jos".into(),
                name: "Joshua".into(),
            },
        ]
    }

    fn verse(n: u8, text: &str, para: bool) -> Verse {
        Verse {
            book: 6,
            chapter: 20,
            verse: n,
            text: text.into(),
            para_break: para,
        }
    }

    fn word(verse: u8, start: i32, end: i32, strongs: &str) -> VerseWord {
        VerseWord {
            book: 6,
            chapter: 20,
            verse,
            i: 0,
            start,
            end,
            strongs: strongs.into(),
        }
    }

    #[test]
    fn pilcrow_on_para_break_verses() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
            verse(7, "And they appointed Kedesh", true),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], false);
        assert!(
            layout.text.contains("1 ¶ The LORD also spake"),
            "{}",
            layout.text
        );
        assert!(
            layout.text.contains("2 Speak to the children"),
            "{}",
            layout.text
        );
        assert!(
            layout.text.contains("7 ¶ And they appointed Kedesh"),
            "{}",
            layout.text
        );
        assert!(
            !layout.text.contains("2 ¶"),
            "non-para verse must not show a pilcrow: {}",
            layout.text
        );
    }

    #[test]
    fn supplied_brackets_become_italic_and_disappear() {
        let stored = "killeth [any] person unawares [and] unwittingly";
        let (display, italics) = strip_supplied(stored);
        assert_eq!(display, "killeth any person unawares and unwittingly");
        assert!(!display.contains('['));
        assert!(!display.contains(']'));
        assert_eq!(italics.len(), 2);
        let any: String = display
            .chars()
            .skip(italics[0].start as usize)
            .take((italics[0].end - italics[0].start) as usize)
            .collect();
        let and: String = display
            .chars()
            .skip(italics[1].start as usize)
            .take((italics[1].end - italics[1].start) as usize)
            .collect();
        assert_eq!(any, "any");
        assert_eq!(and, "and");
    }

    #[test]
    fn strongs_offsets_track_words_after_bracket_strip() {
        let stored = "killeth [any] person";
        // "[any]" occupies stored 8..13 ("killeth " is 8 chars).
        assert_eq!(&stored[8..13], "[any]");
        let span = map_stored_span(8, 13, stored).unwrap();
        let (display, _) = strip_supplied(stored);
        let hit: String = display
            .chars()
            .skip(span.start as usize)
            .take((span.end - span.start) as usize)
            .collect();
        assert_eq!(hit, "any");
        assert!(!hit.contains('['));
    }

    #[test]
    fn tsk_destinations_become_under_verse_links() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
        ];
        let xrefs = vec![
            (
                2,
                Xref {
                    book: 2,
                    chapter: 21,
                    verse: 13,
                },
            ),
            (
                2,
                Xref {
                    book: 2,
                    chapter: 21,
                    verse: 14,
                },
            ),
            (
                2,
                Xref {
                    book: 4,
                    chapter: 35,
                    verse: 6,
                },
            ),
            (
                2,
                Xref {
                    book: 4,
                    chapter: 35,
                    verse: 7,
                },
            ),
        ];
        let layout = layout_chapter(&verses, &books(), &xrefs, &[], &[], &[], false);
        let dests: Vec<Ref> = layout.xrefs.iter().map(|l| l.at).collect();
        assert!(
            dests.contains(&Ref {
                book: 2,
                chapter: 21,
                verse: 13
            }),
            "{dests:?}"
        );
        assert!(
            dests.contains(&Ref {
                book: 4,
                chapter: 35,
                verse: 6
            }),
            "{dests:?}"
        );
        let v2_body = layout.verse_body.iter().find(|(v, _)| *v == 2).unwrap().1;
        for link in &layout.xrefs {
            assert!(
                link.span.start > v2_body,
                "xref {} should sit under verse 2",
                link.label
            );
        }
        assert!(layout.text.contains("Ex"));
        assert!(layout.text.contains("Nu"));
        assert!(layout.text.contains("21:13"));
    }

    #[test]
    fn mhc_mark_only_on_comment_start() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
            verse(3, "Appoint cities", false),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[2], &[], &[], false);
        assert!(
            !layout.mhc.iter().any(|m| m.verse == 1),
            "verse 1 is before the MHC start"
        );
        assert!(layout.mhc.iter().any(|m| m.verse == 2));
        assert!(
            !layout.mhc.iter().any(|m| m.verse == 3),
            "inline MHC is only on the comment start: {}",
            layout.text
        );
        assert!(layout.text.contains("MHC"));
        let none = layout_chapter(&verses, &books(), &[], &[], &[], &[], false);
        assert!(none.mhc.is_empty());
        assert!(!none.text.contains("MHC"));
    }

    #[test]
    fn translator_notes_leave_the_verse_body() {
        let stored =
            "And God divided the light from the darkness. {the light from...: Heb. between the light}";
        let (body, notes) = split_notes(stored);
        assert_eq!(body, "And God divided the light from the darkness.");
        assert_eq!(notes, vec!["the light from...: Heb. between the light"]);

        let multi = "Let fowl fly. {moving: or, creeping} {creature: Heb. soul}";
        let (body, notes) = split_notes(multi);
        assert_eq!(body, "Let fowl fly.");
        assert_eq!(notes, vec!["moving: or, creeping", "creature: Heb. soul"]);

        let verses = vec![verse(6, stored, true)];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], false);
        assert!(
            !layout.text.contains('{'),
            "braces must not appear in the reader: {}",
            layout.text
        );
        assert!(layout
            .text
            .contains("And God divided the light from the darkness."));
        assert!(layout
            .text
            .contains("the light from...: Heb. between the light"));
        assert_eq!(layout.notes.len(), 1);
        let shown: String = layout
            .text
            .chars()
            .skip(layout.notes[0].start as usize)
            .take((layout.notes[0].end - layout.notes[0].start) as usize)
            .collect();
        assert_eq!(shown, "the light from...: Heb. between the light");
        assert!(
            layout.notes[0].start > layout.verse_body[0].1,
            "note sits under the verse"
        );
    }

    #[test]
    fn interlinear_pairs_tagged_english_with_lemma() {
        let stored = "In the [beginning]";
        let w = word(1, 7, 18, "H7225");
        assert_eq!(&stored[7..18], "[beginning]");
        let verses = vec![verse(1, stored, true)];
        let layout = layout_chapter(
            &verses,
            &books(),
            &[],
            &[],
            &[w],
            &[("H7225".into(), "re'shiyth".into())],
            true,
        );
        assert!(
            layout.text.contains("beginning (re'shiyth)"),
            "{}",
            layout.text
        );
        assert!(!layout.text.contains('['));
        assert_eq!(layout.words.len(), 1);
        let hit: String = layout
            .text
            .chars()
            .skip(layout.words[0].span.start as usize)
            .take((layout.words[0].span.end - layout.words[0].span.start) as usize)
            .collect();
        assert_eq!(hit, "beginning");
        let lemma = layout.words[0].lemma_span.unwrap();
        let shown: String = layout
            .text
            .chars()
            .skip(lemma.start as usize)
            .take((lemma.end - lemma.start) as usize)
            .collect();
        assert!(shown.contains("re'shiyth"), "{shown}");
        let ital: String = layout
            .text
            .chars()
            .skip(layout.italics[0].start as usize)
            .take((layout.italics[0].end - layout.italics[0].start) as usize)
            .collect();
        assert_eq!(ital, "beginning");
    }

    #[test]
    fn copy_verses_includes_text_citation_and_kjv() {
        let verses = vec![Verse {
            book: 1,
            chapter: 1,
            verse: 1,
            text: "In the [beginning] God created. {beginning: Heb. head}".into(),
            para_break: true,
        }];
        let out = copy_verses(&books(), &verses);
        assert!(out.contains("In the beginning God created"), "{out}");
        assert!(!out.contains('['), "{out}");
        assert!(!out.contains('{'), "{out}");
        assert!(!out.contains("Heb. head"), "{out}");
        assert!(out.contains("Genesis 1:1"), "{out}");
        assert!(out.contains("KJV"), "{out}");
    }

    #[test]
    fn font_size_steps_and_clamps() {
        assert_eq!(smaller_font(14), 13);
        assert_eq!(larger_font(14), 15);
        assert_eq!(smaller_font(MIN_FONT), MIN_FONT);
        assert_eq!(larger_font(MAX_FONT), MAX_FONT);
    }
}
