use crate::nav::Ref;
use crate::tsk_parse::{self, TskPhrase};
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
pub struct TskMark {
    pub span: Span,
    pub verse: u8,
    pub letter: String,
    pub heading: String,
    pub dests: Vec<Ref>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordSpan {
    pub span: Span,
    pub lemma_span: Option<Span>,
    pub codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteMark {
    pub span: Span,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChapterLayout {
    pub text: String,
    pub italics: Vec<Span>,
    pub xrefs: Vec<XrefLink>,
    pub mhc: Vec<MhcMark>,
    pub tsk: Vec<TskMark>,
    pub words: Vec<WordSpan>,
    pub verse_nums: Vec<Span>,
    pub notes: Vec<NoteMark>,
    pub apparatus: Vec<Span>,
    pub verse_body: Vec<(u8, i32)>,
    pub verse_start: Vec<(u8, i32)>,
    /// Exclusive end of each verse (start of the next, or end of body/apparatus).
    pub verse_end: Vec<(u8, i32)>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayoutOpts {
    pub interlinear: bool,
    pub paragraphs: bool,
}

const NOTE_DAGGER: &str = "†";

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

/// How many book+chapter groups stay visible before collapsing to "N more".
pub const XREF_VISIBLE_CLUSTERS: usize = 5;
/// Prefer a single apparatus line; stop adding clusters once the line is this long.
pub const XREF_LINE_CHARS: i32 = 56;
/// If only this many destinations would be hidden, show them instead of "N more".
pub const XREF_TAIL_KEEP: usize = 2;

pub fn format_xref_line(xrefs: &[Xref], books: &[Book]) -> (String, Vec<XrefLink>, usize) {
    let clusters = xref_clusters(xrefs);
    let total: usize = clusters.iter().map(|(_, _, vs)| unique_len(vs)).sum();
    let mut text = String::new();
    let mut links = Vec::new();
    let mut last_book: Option<u8> = None;
    let mut shown = 0usize;
    for (i, (book, chapter, verses)) in clusters.iter().enumerate() {
        let hidden_if_skip = total - shown;
        if i > 0
            && hidden_if_skip > XREF_TAIL_KEEP
            && (i >= XREF_VISIBLE_CLUSTERS || char_len(&text) >= XREF_LINE_CHARS)
        {
            break;
        }
        if !text.is_empty() {
            text.push_str("; ");
        }
        if last_book != Some(*book) {
            text.push_str(book_abbrev(books, *book));
            text.push(' ');
            last_book = Some(*book);
        }
        text.push_str(&format!("{chapter}:"));
        for (j, run) in verse_runs(verses).into_iter().enumerate() {
            if j > 0 {
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
                    book: *book,
                    chapter: *chapter,
                    verse: run.0,
                },
                label,
            });
        }
        shown += unique_len(verses);
    }
    (text, links, total.saturating_sub(shown))
}

fn unique_len(verses: &[u8]) -> usize {
    let mut vs = verses.to_vec();
    vs.sort_unstable();
    vs.dedup();
    vs.len()
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
    tsk_notes: &[(u8, &str)],
    mhc_starts: &[u8],
    words: &[VerseWord],
    lemmas: &[(String, String)],
    opts: LayoutOpts,
) -> ChapterLayout {
    let mut layout = ChapterLayout::default();
    let mut prev_had_extra = false;
    for v in verses {
        if !layout.text.is_empty() {
            if opts.paragraphs {
                if v.para_break {
                    layout.text.push('\n');
                    layout.text.push('\n');
                } else if prev_had_extra {
                    layout.text.push('\n');
                } else {
                    layout.text.push(' ');
                }
            } else {
                layout.text.push('\n');
            }
        }

        let verse_start = char_len(&layout.text);
        let num = format!("{}", v.verse);
        layout.text.push_str(&num);
        let num_span = Span {
            start: verse_start,
            end: char_len(&layout.text),
        };
        layout.verse_nums.push(num_span);
        if mhc_starts.contains(&v.verse) {
            layout.mhc.push(MhcMark {
                span: num_span,
                verse: v.verse,
            });
        }
        layout.text.push(' ');
        if !opts.paragraphs && v.para_break {
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
        let (body, italics, word_spans, lemma_inserts) =
            build_body(&stored_body, &verse_words, lemmas, opts.interlinear);
        let phrases = tsk_notes
            .iter()
            .find(|(verse, _)| *verse == v.verse)
            .map(|(_, text)| tsk_parse::parse_tsk_phrases(text, books))
            .unwrap_or_default();
        let (body, italics, word_spans, tsk_marks, tsk_inserts) =
            apply_tsk_marks(body, italics, word_spans, &phrases, v.verse);
        let (stripped, _) = strip_supplied(&stored_body);
        let note_plan: Vec<(i32, String)> = plan_note_inserts(&stripped, &note_texts)
            .into_iter()
            .map(|(at, text)| {
                let at = shift_by_inserts(at, &lemma_inserts);
                (shift_by_inserts(at, &tsk_inserts), text)
            })
            .collect();
        let mark_inserts: Vec<(i32, i32)> = note_plan
            .iter()
            .map(|(at, _)| (*at, char_len(NOTE_DAGGER)))
            .collect();
        let (body, italics, word_spans, note_marks) =
            insert_inline_marks(body, italics, word_spans, note_plan);
        let tsk_marks: Vec<TskMark> = tsk_marks
            .into_iter()
            .map(|mut mark| {
                mark.span = shift_span_inserts(mark.span, &mark_inserts);
                mark
            })
            .collect();
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
        for mut mark in tsk_marks {
            mark.span = shift_span(mark.span, body_start);
            layout.tsk.push(mark);
        }
        for mut mark in note_marks {
            mark.span = shift_span(mark.span, body_start);
            layout.notes.push(mark);
        }
        layout.verse_start.push((v.verse, verse_start));
        layout.verse_body.push((v.verse, body_start));
        layout.verse_end.push((v.verse, char_len(&layout.text)));
        prev_had_extra = false;
    }
    layout
}

type TskApplied = (
    String,
    Vec<Span>,
    Vec<WordSpan>,
    Vec<TskMark>,
    Vec<(i32, i32)>,
);

fn apply_tsk_marks(
    body: String,
    italics: Vec<Span>,
    words: Vec<WordSpan>,
    phrases: &[TskPhrase],
    verse: u8,
) -> TskApplied {
    if phrases.is_empty() {
        return (body, italics, words, Vec::new(), Vec::new());
    }
    let forbidden: Vec<Span> = words.iter().filter_map(|w| w.lemma_span).collect();
    let mut claimed = Vec::new();
    let mut hits: Vec<(Span, String, &TskPhrase)> = Vec::new();
    for phrase in phrases {
        if let Some(span) = find_heading(&body, &phrase.heading, &claimed, &forbidden) {
            claimed.push(span);
            let letter = tsk_letter(hits.len());
            hits.push((span, letter, phrase));
        }
    }
    if hits.is_empty() {
        return (body, italics, words, Vec::new(), Vec::new());
    }
    hits.sort_by_key(|(span, _, _)| span.start);

    let inserts: Vec<(i32, i32)> = hits
        .iter()
        .map(|(span, letter, _)| (span.end, char_len(letter)))
        .collect();

    let mut out = String::new();
    let mut last = 0i32;
    let mut marks = Vec::new();
    for (span, letter, phrase) in &hits {
        out.push_str(&slice_chars(&body, last, span.end));
        let start = char_len(&out);
        out.push_str(letter);
        marks.push(TskMark {
            span: Span {
                start,
                end: char_len(&out),
            },
            verse,
            letter: letter.clone(),
            heading: phrase.heading.clone(),
            dests: phrase.dests.clone(),
        });
        last = span.end;
    }
    out.push_str(&slice_chars(&body, last, char_len(&body)));

    let italics = italics
        .into_iter()
        .map(|span| shift_span_around_inserts(span, &inserts))
        .filter(|span| span.end > span.start)
        .collect();
    let words = words
        .into_iter()
        .map(|mut w| {
            w.span = shift_span_around_inserts(w.span, &inserts);
            if let Some(ls) = w.lemma_span.as_mut() {
                *ls = shift_span_around_inserts(*ls, &inserts);
            }
            w
        })
        .collect();
    (out, italics, words, marks, inserts)
}

fn find_heading(body: &str, heading: &str, claimed: &[Span], forbidden: &[Span]) -> Option<Span> {
    let needle: Vec<char> = heading.chars().collect();
    if needle.len() < 3 {
        return None;
    }
    let hay: Vec<char> = body.chars().collect();
    let mut i = 0usize;
    while i + needle.len() <= hay.len() {
        if heading_at(&hay, i, &needle) {
            let span = Span {
                start: i as i32,
                end: (i + needle.len()) as i32,
            };
            if !overlaps(span, claimed) && !overlaps(span, forbidden) {
                return Some(span);
            }
        }
        i += 1;
    }
    None
}

fn heading_at(hay: &[char], i: usize, needle: &[char]) -> bool {
    let before_ok = i == 0 || !hay[i - 1].is_alphanumeric();
    let after_ok = i + needle.len() == hay.len() || !hay[i + needle.len()].is_alphanumeric();
    if !before_ok || !after_ok {
        return false;
    }
    hay[i..i + needle.len()]
        .iter()
        .zip(needle)
        .all(|(c, n)| c.eq_ignore_ascii_case(n))
}

fn overlaps(span: Span, others: &[Span]) -> bool {
    others
        .iter()
        .any(|o| span.start < o.end && o.start < span.end)
}

fn tsk_letter(idx: usize) -> String {
    let mut n = idx;
    let mut chars = Vec::new();
    loop {
        chars.push((b'a' + (n % 26) as u8) as char);
        if n < 26 {
            break;
        }
        n = n / 26 - 1;
    }
    chars.into_iter().rev().collect()
}

fn shift_span_around_inserts(span: Span, inserts: &[(i32, i32)]) -> Span {
    let start = span.start
        + inserts
            .iter()
            .filter(|(at, _)| *at <= span.start)
            .map(|(_, n)| *n)
            .sum::<i32>();
    let end = span.end
        + inserts
            .iter()
            .filter(|(at, _)| *at < span.end)
            .map(|(_, n)| *n)
            .sum::<i32>();
    Span { start, end }
}

type BuiltBody = (String, Vec<Span>, Vec<WordSpan>, Vec<(i32, i32)>);

fn build_body(
    stored: &str,
    words: &[VerseWord],
    lemmas: &[(String, String)],
    interlinear: bool,
) -> BuiltBody {
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
        return (stripped, italics, word_spans, Vec::new());
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
    (out, new_italics, new_words, inserts)
}

fn plan_note_inserts(body: &str, notes: &[String]) -> Vec<(i32, String)> {
    let parsed: Vec<(String, bool, String)> = notes.iter().map(|n| parse_note(n)).collect();
    let mut cores = vec![None; parsed.len()];
    let mut used = Vec::new();
    for (i, (heading, _, _)) in parsed.iter().enumerate() {
        if heading.is_empty() {
            continue;
        }
        if let Some(span) = find_phrase(body, heading, &used) {
            cores[i] = Some(span);
            used.push(span);
        }
    }
    let body_end = char_len(body);
    parsed
        .iter()
        .enumerate()
        .map(|(i, (_, ellipsis, expl))| {
            let at = match cores[i] {
                Some(span) if *ellipsis => {
                    let others: Vec<Span> = cores
                        .iter()
                        .enumerate()
                        .filter(|(j, _)| *j != i)
                        .filter_map(|(_, s)| *s)
                        .collect();
                    extend_phrase(body, span.end, &others)
                }
                Some(span) => span.end,
                None => body_end,
            };
            (at, expl.clone())
        })
        .collect()
}

fn parse_note(note: &str) -> (String, bool, String) {
    let Some((raw_head, raw_expl)) = note.split_once(':') else {
        return (String::new(), false, note.trim().to_string());
    };
    let (heading, ellipsis) = strip_trailing_ellipsis(raw_head.trim());
    let expl = raw_expl.trim();
    let text = if expl.is_empty() {
        note.trim().to_string()
    } else {
        expl.to_string()
    };
    (heading, ellipsis, text)
}

fn strip_trailing_ellipsis(heading: &str) -> (String, bool) {
    for suffix in ["...", "…"] {
        if let Some(stripped) = heading.strip_suffix(suffix) {
            return (stripped.trim_end().to_string(), true);
        }
    }
    (heading.to_string(), false)
}

fn find_phrase(body: &str, phrase: &str, used: &[Span]) -> Option<Span> {
    let b: Vec<char> = body.chars().collect();
    let p: Vec<char> = phrase.chars().collect();
    if p.is_empty() || p.len() > b.len() {
        return None;
    }
    let last = b.len() - p.len();
    for start in 0..=last {
        if !chars_eq_ci(&b[start..start + p.len()], &p) {
            continue;
        }
        if !has_word_bounds(&b, start, start + p.len()) {
            continue;
        }
        let span = Span {
            start: start as i32,
            end: (start + p.len()) as i32,
        };
        if used.iter().any(|u| spans_overlap(*u, span)) {
            continue;
        }
        return Some(span);
    }
    None
}

fn chars_eq_ci(a: &[char], b: &[char]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.eq_ignore_ascii_case(y))
}

fn has_word_bounds(body: &[char], start: usize, end: usize) -> bool {
    let left_ok = start == 0 || !is_word_char(body[start - 1]) || !is_word_char(body[start]);
    let right_ok = end == body.len() || !is_word_char(body[end]) || !is_word_char(body[end - 1]);
    left_ok && right_ok
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '\''
}

fn spans_overlap(a: Span, b: Span) -> bool {
    a.start < b.end && b.start < a.end
}

fn extend_phrase(body: &str, match_end: i32, others: &[Span]) -> i32 {
    let chars: Vec<char> = body.chars().collect();
    let mut i = match_end.max(0) as usize;
    while i < chars.len() {
        if is_phrase_end(chars[i]) {
            break;
        }
        let off = i as i32;
        if others.iter().any(|s| off >= s.start && off < s.end) {
            return match_end;
        }
        i += 1;
    }
    while i > match_end as usize && chars[i - 1].is_whitespace() {
        i -= 1;
    }
    i as i32
}

fn is_phrase_end(c: char) -> bool {
    matches!(c, ',' | ';' | '.' | ':' | '?' | '!')
}

fn insert_inline_marks(
    body: String,
    italics: Vec<Span>,
    words: Vec<WordSpan>,
    notes: Vec<(i32, String)>,
) -> (String, Vec<Span>, Vec<WordSpan>, Vec<NoteMark>) {
    struct Pending {
        at: i32,
        glyph: &'static str,
        note: String,
    }
    let pending: Vec<Pending> = notes
        .into_iter()
        .map(|(at, text)| Pending {
            at,
            glyph: NOTE_DAGGER,
            note: text,
        })
        .collect();
    if pending.is_empty() {
        return (body, italics, words, Vec::new());
    }

    let insert_lens: Vec<(i32, i32)> = pending.iter().map(|p| (p.at, char_len(p.glyph))).collect();

    let mut out = body;
    let mut order: Vec<usize> = (0..pending.len()).collect();
    order.sort_by(|&a, &b| pending[b].at.cmp(&pending[a].at).then(b.cmp(&a)));
    for i in order {
        insert_at(&mut out, pending[i].at, pending[i].glyph);
    }

    let italics = shift_spans(italics, &insert_lens);
    let words = words
        .into_iter()
        .map(|mut w| {
            w.span = shift_span_inserts(w.span, &insert_lens);
            if let Some(ls) = w.lemma_span.as_mut() {
                *ls = shift_span_inserts(*ls, &insert_lens);
            }
            w
        })
        .collect();

    let mut note_marks = Vec::new();
    for (idx, p) in pending.iter().enumerate() {
        let start = p.at
            + insert_lens
                .iter()
                .enumerate()
                .filter(|(j, (at, _))| *at < p.at || (*at == p.at && *j < idx))
                .map(|(_, (_, n))| *n)
                .sum::<i32>();
        note_marks.push(NoteMark {
            span: Span {
                start,
                end: start + char_len(p.glyph),
            },
            text: p.note.clone(),
        });
    }
    (out, italics, words, note_marks)
}

fn shift_spans(spans: Vec<Span>, inserts: &[(i32, i32)]) -> Vec<Span> {
    spans
        .into_iter()
        .filter_map(|span| {
            let shifted = shift_span_inserts(span, inserts);
            (shifted.end > shifted.start).then_some(shifted)
        })
        .collect()
}

fn shift_span_inserts(span: Span, inserts: &[(i32, i32)]) -> Span {
    Span {
        start: shift_by_inserts(span.start, inserts),
        end: shift_by_inserts(span.end, inserts),
    }
}

fn insert_at(s: &mut String, at: i32, text: &str) {
    let at = at.max(0) as usize;
    let byte = s.char_indices().nth(at).map(|(i, _)| i).unwrap_or(s.len());
    s.insert_str(byte, text);
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

/// Visible English token for a mapped KJV word, punctuation stripped.
pub fn word_surface(text: &str, span: Span) -> String {
    let raw = slice_chars(text, span.start, span.end);
    normalize_token(&raw)
}

/// Word under a click. Strong's spans are often a whole phrase.
pub fn token_at(text: &str, offset: i32) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    let mut i = (offset.max(0) as usize).min(chars.len() - 1);
    if !is_token_char(chars[i]) {
        if i > 0 && is_token_char(chars[i - 1]) {
            i -= 1;
        } else {
            return String::new();
        }
    }
    let mut start = i;
    while start > 0 && is_token_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = i + 1;
    while end < chars.len() && is_token_char(chars[end]) {
        end += 1;
    }
    normalize_token(&chars[start..end].iter().collect::<String>())
}

fn is_token_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

fn normalize_token(raw: &str) -> String {
    raw.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'')
        .trim_matches('\'')
        .to_string()
}

fn char_len(s: &str) -> i32 {
    s.chars().count() as i32
}

/// Verse whose `[verse_start, next verse_start)` contains `offset`.
pub fn verse_at_offset(verse_start: &[(u8, i32)], offset: i32) -> Option<u8> {
    verse_start
        .iter()
        .rev()
        .find(|(_, start)| offset >= *start)
        .map(|(verse, _)| *verse)
}

/// Half-open range `[verse_start, next verse_start)` for `verse`.
/// `text_end` is the exclusive end of the last verse.
#[allow(dead_code)]
pub fn verse_range(verse_start: &[(u8, i32)], verse: u8, text_end: i32) -> Option<Span> {
    let i = verse_start.iter().position(|(v, _)| *v == verse)?;
    let start = verse_start[i].1;
    let end = verse_start
        .get(i + 1)
        .map(|(_, next)| *next)
        .unwrap_or(text_end);
    (end > start).then_some(Span { start, end })
}

pub fn word_at_offset(words: &[WordSpan], offset: i32) -> Option<&WordSpan> {
    words
        .iter()
        .find(|w| w.span.contains(offset) || w.lemma_span.is_some_and(|s| s.contains(offset)))
}

/// Verses covered by a half-open text selection `[start, end)`.
pub fn verses_in_selection(verse_start: &[(u8, i32)], start: i32, end: i32) -> Option<(u8, u8)> {
    if end <= start {
        return None;
    }
    let v1 = verse_at_offset(verse_start, start)?;
    let v2 = verse_at_offset(verse_start, end - 1)?;
    Some(if v1 <= v2 { (v1, v2) } else { (v2, v1) })
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
        [
            (1, "Ge", "Genesis"),
            (2, "Ex", "Exodus"),
            (4, "Nu", "Numbers"),
            (5, "De", "Deuteronomy"),
            (6, "Jos", "Joshua"),
            (13, "1Ch", "1 Chronicles"),
            (18, "Job", "Job"),
            (19, "Ps", "Psalms"),
            (20, "Pr", "Proverbs"),
            (41, "Mr", "Mark"),
            (43, "Joh", "John"),
            (58, "Heb", "Hebrews"),
            (62, "1Jo", "1 John"),
        ]
        .into_iter()
        .map(|(id, abbrev, name)| Book {
            id,
            abbrev: abbrev.into(),
            name: name.into(),
        })
        .collect()
    }

    fn span_text(text: &str, span: Span) -> String {
        text.chars()
            .skip(span.start as usize)
            .take((span.end - span.start) as usize)
            .collect()
    }

    fn before(text: &str, at: i32, n: usize) -> String {
        let chars: Vec<char> = text.chars().take(at as usize).collect();
        chars
            .iter()
            .rev()
            .take(n)
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect()
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

    fn verse_opts() -> LayoutOpts {
        LayoutOpts::default()
    }

    fn para_opts() -> LayoutOpts {
        LayoutOpts {
            paragraphs: true,
            ..LayoutOpts::default()
        }
    }

    fn inter_opts() -> LayoutOpts {
        LayoutOpts {
            interlinear: true,
            ..LayoutOpts::default()
        }
    }

    fn slice(text: &str, span: Span) -> String {
        text.chars()
            .skip(span.start as usize)
            .take((span.end - span.start).max(0) as usize)
            .collect()
    }

    fn before_mark(text: &str, span: Span) -> String {
        text.chars().take(span.start as usize).collect()
    }

    #[test]
    fn pilcrow_on_para_break_verses() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
            verse(7, "And they appointed Kedesh", true),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert!(
            layout.text.contains("1 ¶ The LORD also spake"),
            "{}",
            layout.text
        );
        assert!(
            layout.text.contains("spake\n2 Speak"),
            "verse mode uses a single line break, not a blank line: {}",
            layout.text
        );
        assert!(
            !layout.text.contains("spake\n\n2"),
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
    fn paragraphs_join_non_break_verses_with_a_space() {
        let verses = vec![
            verse(1, "The LORD also spake", false),
            verse(2, "Speak to the children", false),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], para_opts());
        assert!(
            layout
                .text
                .contains("1 The LORD also spake 2 Speak to the children"),
            "{}",
            layout.text
        );
        assert!(
            !layout.text.contains("spake\n\n2"),
            "non-break verses must not use a blank line: {}",
            layout.text
        );
        assert!(!layout.text.contains("\n\n"), "{}", layout.text);
        assert_eq!(layout.verse_nums.len(), 2);
        assert_eq!(layout.verse_start[0].1, layout.verse_nums[0].start);
        assert_eq!(span_text(&layout.text, layout.verse_nums[0]), "1");
        assert_eq!(span_text(&layout.text, layout.verse_nums[1]), "2");
        let v1_end = layout.verse_end.iter().find(|(v, _)| *v == 1).unwrap().1;
        let v2_start = layout.verse_start.iter().find(|(v, _)| *v == 2).unwrap().1;
        assert!(v1_end <= v2_start);
        let v1: String = layout
            .text
            .chars()
            .skip(layout.verse_start[0].1 as usize)
            .take((v1_end - layout.verse_start[0].1) as usize)
            .collect();
        assert!(v1.contains("The LORD also spake"), "{v1}");
        assert!(!v1.contains("Speak to the children"), "{v1}");
    }

    #[test]
    fn paragraphs_honor_para_break() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
            verse(7, "And they appointed Kedesh", true),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], para_opts());
        assert!(
            layout
                .text
                .contains("1 The LORD also spake 2 Speak to the children"),
            "{}",
            layout.text
        );
        assert!(
            layout
                .text
                .contains("children\n\n7 And they appointed Kedesh"),
            "para_break starts a new paragraph: {}",
            layout.text
        );
        assert!(!layout.text.contains('¶'), "{}", layout.text);
        assert_eq!(layout.verse_nums.len(), 3);
        for (i, v) in verses.iter().enumerate() {
            assert_eq!(layout.verse_start[i].0, v.verse);
            assert_eq!(layout.verse_nums[i].start, layout.verse_start[i].1);
            assert_eq!(
                span_text(&layout.text, layout.verse_nums[i]),
                format!("{}", v.verse)
            );
        }
    }

    #[test]
    fn paragraphs_drop_pilcrow_verse_mode_keeps_it() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
        ];
        let para = layout_chapter(&verses, &books(), &[], &[], &[], &[], para_opts());
        let verse_mode = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert!(!para.text.contains('¶'), "{}", para.text);
        assert!(
            verse_mode.text.contains("1 ¶ The LORD also spake"),
            "{}",
            verse_mode.text
        );
        assert!(!verse_mode.text.contains("2 ¶"), "{}", verse_mode.text);
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
    fn tsk_genesis_1_1_superscripts_on_phrases() {
        let verses = vec![verse(
            1,
            "In the beginning God created the heaven and the earth.",
            true,
        )];
        let tsk = [(
            1u8,
            "\
God creates heaven and earth.

* beginning. Proverbs 8:22–24 Proverbs 16:4 Mark 13:19 John 1:1–3 Hebrews 1:10 1 John 1:1
* God. Exodus 20:11 Exodus 31:18 1 Chronicles 16:26",
        )];
        let layout = layout_chapter(&verses, &books(), &tsk, &[], &[], &[], verse_opts());
        assert_eq!(layout.tsk.len(), 2, "{}", layout.text);
        assert_eq!(layout.tsk[0].heading, "beginning");
        assert_eq!(layout.tsk[1].heading, "God");
        assert_eq!(span_text(&layout.text, layout.tsk[0].span), "a");
        assert_eq!(span_text(&layout.text, layout.tsk[1].span), "b");
        assert_eq!(
            before(&layout.text, layout.tsk[0].span.start, 9),
            "beginning"
        );
        assert_eq!(before(&layout.text, layout.tsk[1].span.start, 3), "God");
        for mark in &layout.tsk {
            for num in &layout.verse_nums {
                assert!(
                    !num.contains(mark.span.start),
                    "TSK mark must not sit on the verse number: {}",
                    layout.text
                );
            }
        }
        assert!(!layout.text.contains("Ex 20:11"), "{}", layout.text);
        assert!(!layout.text.contains("Exodus 20:11"), "{}", layout.text);
        assert!(!layout.text.contains("20:11"), "{}", layout.text);
        assert!(!layout.text.contains(" more"), "{}", layout.text);
        assert!(layout.xrefs.is_empty());
        assert!(layout.tsk[1].dests.contains(&Ref {
            book: 2,
            chapter: 20,
            verse: 11
        }));
    }

    #[test]
    fn tsk_heb_only_heading_gets_no_superscript() {
        let verses = vec![verse(
            6,
            "And God said, Let there be a firmament in the midst of the waters",
            true,
        )];
        let tsk = [(
            6u8,
            "* Let there. Genesis 1:14 Job 26:7\n* firmament. Heb. expansion.",
        )];
        let layout = layout_chapter(&verses, &books(), &tsk, &[], &[], &[], verse_opts());
        assert_eq!(layout.tsk.len(), 1, "{}", layout.text);
        assert_eq!(layout.tsk[0].heading, "Let there");
        assert_eq!(
            before(&layout.text, layout.tsk[0].span.start, 9),
            "Let there"
        );
        assert!(
            !layout
                .tsk
                .iter()
                .any(|m| m.heading.eq_ignore_ascii_case("firmament")),
            "{}",
            layout.text
        );
    }

    #[test]
    fn tsk_marks_fruit_not_grass_when_grass_has_no_refs() {
        let verses = vec![verse(
            11,
            "And God said, Let the earth bring forth grass, the herb yielding seed, and the fruit tree yielding fruit",
            true,
        )];
        let tsk = [(
            11u8,
            "* grass. Heb. tender grass. fruit. Genesis 1:29 Genesis 2:9",
        )];
        let layout = layout_chapter(&verses, &books(), &tsk, &[], &[], &[], verse_opts());
        assert_eq!(layout.tsk.len(), 1, "{}", layout.text);
        assert_eq!(layout.tsk[0].heading, "fruit");
        assert_eq!(before(&layout.text, layout.tsk[0].span.start, 5), "fruit");
        assert!(
            !layout.tsk.iter().any(|m| m.heading == "grass"),
            "{}",
            layout.text
        );
    }

    #[test]
    fn tsk_unmatched_heading_does_not_mark_verse_number() {
        let verses = vec![verse(1, "In the beginning God created", true)];
        let tsk = [(1u8, "* xyzzy. Genesis 1:1 Exodus 20:11")];
        let layout = layout_chapter(&verses, &books(), &tsk, &[], &[], &[], verse_opts());
        assert!(layout.tsk.is_empty(), "{}", layout.text);
        assert_eq!(span_text(&layout.text, layout.verse_nums[0]), "1");
        assert!(!layout.text.contains("20:11"), "{}", layout.text);
        assert!(!layout.text.contains("xyzzy"), "{}", layout.text);
    }

    #[test]
    fn tsk_tooltip_line_still_collapses_long_lists() {
        let xrefs: Vec<Xref> = (21..=28)
            .map(|ch| Xref {
                book: 2,
                chapter: ch,
                verse: 1,
            })
            .collect();
        let (text, links, hidden) = format_xref_line(&xrefs, &books());
        assert!(text.contains("Ex"), "{text}");
        assert_eq!(hidden, 3);
        assert_eq!(links.len(), 5);
        assert!(
            !links.iter().any(|l| l.at.chapter == 28),
            "truncated dests must not stay clickable: {links:?}"
        );
    }

    #[test]
    fn tsk_tooltip_line_keeps_small_overflow() {
        let xrefs: Vec<Xref> = (21..=27)
            .map(|ch| Xref {
                book: 2,
                chapter: ch,
                verse: 1,
            })
            .collect();
        let (text, links, hidden) = format_xref_line(&xrefs, &books());
        assert_eq!(hidden, 0);
        assert_eq!(links.len(), 7);
        assert!(text.contains("27:1"), "{text}");
        assert!(!text.contains("more"), "{text}");
    }

    #[test]
    fn mhc_mark_only_on_comment_start() {
        let verses = vec![
            verse(1, "The LORD also spake", true),
            verse(2, "Speak to the children", false),
            verse(3, "Appoint cities", false),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[2], &[], &[], verse_opts());
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
        assert!(
            !layout.text.contains("MHC") && !layout.text.contains('M'),
            "no MHC glyph in the chapter: {}",
            layout.text
        );
        let mark = layout.mhc.iter().find(|m| m.verse == 2).unwrap();
        let num = layout
            .verse_nums
            .iter()
            .find(|n| slice(&layout.text, **n) == "2")
            .copied()
            .unwrap();
        assert_eq!(mark.span, num, "MHC click target is the verse number");
        assert!(
            layout.apparatus.is_empty(),
            "MHC-only verses must not grow an apparatus line: {}",
            layout.text
        );
        let none = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert!(none.mhc.is_empty());
        assert!(!none.text.contains("MHC"));
        assert!(!none.text.contains('\u{2020}'));

        let tsk = [(2u8, "* Speak. Exodus 21:1")];
        let with_tsk = layout_chapter(&verses, &books(), &tsk, &[2], &[], &[], verse_opts());
        let mark = with_tsk.mhc.iter().find(|m| m.verse == 2).unwrap();
        assert_eq!(slice(&with_tsk.text, mark.span), "2");
        assert!(
            !with_tsk.text.contains("MHC") && !with_tsk.text.contains('M'),
            "no MHC glyph when TSK letters are present: {}",
            with_tsk.text
        );
        assert!(with_tsk.apparatus.is_empty(), "{}", with_tsk.text);
        assert!(
            with_tsk.tsk.iter().any(|m| m.verse == 2),
            "TSK letter still lands on the reading line: {}",
            with_tsk.text
        );
    }

    #[test]
    fn ellipsis_note_gets_a_dagger_on_the_phrase() {
        let stored =
            "And God divided the light from the darkness. {the light from...: Heb. between the light}";
        let (body, notes) = split_notes(stored);
        assert_eq!(body, "And God divided the light from the darkness.");
        assert_eq!(notes, vec!["the light from...: Heb. between the light"]);

        let verses = vec![verse(4, stored, true)];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert!(
            !layout.text.contains('{'),
            "braces must not appear in the reader: {}",
            layout.text
        );
        assert!(
            !layout.text.contains("Heb. between the light"),
            "note sentence must not sit under the verse: {}",
            layout.text
        );
        assert_eq!(layout.notes.len(), 1);
        assert_eq!(slice(&layout.text, layout.notes[0].span), "†");
        assert_eq!(layout.notes[0].text, "Heb. between the light");
        let before = before_mark(&layout.text, layout.notes[0].span);
        assert!(
            before.ends_with("the light from the darkness") || before.ends_with("the light from"),
            "dagger should follow the matched prefix: {before}"
        );
        assert!(
            layout.notes[0].span.start >= layout.verse_body[0].1,
            "dagger is on the reading line"
        );
        assert!(!layout
            .verse_nums
            .iter()
            .any(|n| n.contains(layout.notes[0].span.start)));
    }

    #[test]
    fn exact_note_heading_gets_a_dagger_after_the_word() {
        let stored =
            "And God said, Let there be a firmament in the midst of the waters. {firmament: Heb. expansion}";
        let verses = vec![verse(6, stored, true)];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert_eq!(layout.notes.len(), 1);
        assert_eq!(slice(&layout.text, layout.notes[0].span), "†");
        assert_eq!(layout.notes[0].text, "Heb. expansion");
        assert!(
            before_mark(&layout.text, layout.notes[0].span).ends_with("firmament"),
            "{}",
            layout.text
        );
        assert!(
            !layout.text.contains("Heb. expansion"),
            "explanation stays off the chapter body: {}",
            layout.text
        );
        assert!(!layout.text.contains('{'));
    }

    #[test]
    fn unmatched_note_dagger_sits_at_end_of_body() {
        let stored = "In the beginning God created. {no-such-heading: Heb. foo}";
        let verses = vec![verse(1, stored, true)];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert_eq!(layout.notes.len(), 1);
        assert_eq!(slice(&layout.text, layout.notes[0].span), "†");
        assert_eq!(layout.notes[0].text, "Heb. foo");
        assert!(
            before_mark(&layout.text, layout.notes[0].span).ends_with("created."),
            "unmatched dagger belongs at the end of the body: {}",
            layout.text
        );
        assert!(
            !layout
                .verse_nums
                .iter()
                .any(|n| n.contains(layout.notes[0].span.start)),
            "unmatched dagger must not sit on the verse number: {}",
            layout.text
        );
        assert!(layout.notes[0].span.start >= layout.verse_body[0].1);
        assert!(!layout.text.contains("Heb. foo"));
    }

    #[test]
    fn two_notes_make_two_daggers() {
        let stored =
            "the moving creature that hath life. {moving: or, creeping} {creature: Heb. soul}";
        let (body, notes) = split_notes(stored);
        assert_eq!(body, "the moving creature that hath life.");
        assert_eq!(notes, vec!["moving: or, creeping", "creature: Heb. soul"]);

        let verses = vec![verse(20, stored, true)];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert_eq!(layout.notes.len(), 2, "{}", layout.text);
        assert_eq!(slice(&layout.text, layout.notes[0].span), "†");
        assert_eq!(slice(&layout.text, layout.notes[1].span), "†");
        assert_eq!(layout.notes[0].text, "or, creeping");
        assert_eq!(layout.notes[1].text, "Heb. soul");
        assert!(layout.notes[0].span.start < layout.notes[1].span.start);
        assert!(before_mark(&layout.text, layout.notes[0].span).ends_with("moving"));
        assert!(before_mark(&layout.text, layout.notes[1].span).ends_with("creature"));
        assert!(!layout.text.contains("or, creeping"));
        assert!(!layout.text.contains("Heb. soul"));
        assert!(!layout.text.contains('{'));
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
            inter_opts(),
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
    fn tsk_interlinear_keeps_strongs_spans() {
        let stored = "In the [beginning] God created";
        let w = word(1, 7, 18, "H7225");
        assert_eq!(&stored[7..18], "[beginning]");
        let verses = vec![verse(1, stored, true)];
        let tsk = [(1u8, "* beginning. Proverbs 8:22\n* God. Exodus 20:11")];
        let layout = layout_chapter(
            &verses,
            &books(),
            &tsk,
            &[],
            &[w],
            &[("H7225".into(), "re'shiyth".into())],
            inter_opts(),
        );
        assert!(
            layout.text.contains("beginninga (re'shiyth)")
                || layout.text.contains("beginninga(re'shiyth)"),
            "{}",
            layout.text
        );
        assert_eq!(layout.words.len(), 1);
        assert_eq!(span_text(&layout.text, layout.words[0].span), "beginning");
        let lemma = layout.words[0].lemma_span.unwrap();
        assert!(
            span_text(&layout.text, lemma).contains("re'shiyth"),
            "{}",
            layout.text
        );
        assert_eq!(layout.tsk[0].heading, "beginning");
        assert_eq!(
            before(&layout.text, layout.tsk[0].span.start, 9),
            "beginning"
        );
        assert!(
            layout.tsk[0].span.end <= lemma.start || layout.tsk[0].span.start >= lemma.end,
            "letter must not sit inside the lemma span: {}",
            layout.text
        );
    }

    #[test]
    fn offset_maps_to_verse_from_verse_start() {
        let verses = vec![
            verse(1, "First verse", true),
            verse(2, "Second verse", false),
            verse(3, "Third verse", false),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        assert_eq!(layout.verse_start[0], (1, 0));
        assert_eq!(layout.verse_start[1].0, 2);
        assert_eq!(layout.verse_start[2].0, 3);
        let s1 = layout.verse_start[0].1;
        let s2 = layout.verse_start[1].1;
        let s3 = layout.verse_start[2].1;
        assert!(s2 > s1);
        assert!(s3 > s2);
        assert_eq!(verse_at_offset(&layout.verse_start, s1), Some(1));
        assert_eq!(verse_at_offset(&layout.verse_start, s2 - 1), Some(1));
        assert_eq!(verse_at_offset(&layout.verse_start, s2), Some(2));
        assert_eq!(verse_at_offset(&layout.verse_start, s3 - 1), Some(2));
        assert_eq!(verse_at_offset(&layout.verse_start, s3), Some(3));
        let b2 = layout.verse_body.iter().find(|(v, _)| *v == 2).unwrap().1;
        assert_eq!(verse_at_offset(&layout.verse_start, b2), Some(2));
        assert!(b2 >= s2 && b2 < s3);
    }

    #[test]
    fn verse_highlight_range_stops_before_next_verse() {
        let verses = vec![
            verse(1, "First verse", true),
            verse(2, "Second verse", false),
            verse(3, "Third verse", false),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        let s2 = layout.verse_start[1].1;
        let s3 = layout.verse_start[2].1;
        let text_end = char_len(&layout.text);
        let range = verse_range(&layout.verse_start, 2, text_end).unwrap();
        assert_eq!(range.start, s2);
        assert_eq!(range.end, s3);
        assert!(range.contains(s2));
        assert!(
            !range.contains(s3),
            "verse 2 must not include verse 3's start"
        );
        let last = verse_range(&layout.verse_start, 3, text_end).unwrap();
        assert_eq!(last.start, s3);
        assert_eq!(last.end, text_end);
        assert!(!last.contains(text_end));
    }

    #[test]
    fn verses_in_selection_covers_the_spanned_range() {
        let verses = vec![
            verse(1, "First verse", true),
            verse(2, "Second verse", false),
            verse(3, "Third verse", false),
        ];
        let layout = layout_chapter(&verses, &books(), &[], &[], &[], &[], verse_opts());
        let s1 = layout.verse_start[0].1;
        let s2 = layout.verse_start[1].1;
        let s3 = layout.verse_start[2].1;
        assert_eq!(verses_in_selection(&layout.verse_start, s1, s1), None);
        assert_eq!(
            verses_in_selection(&layout.verse_start, s1, s2),
            Some((1, 1))
        );
        assert_eq!(
            verses_in_selection(&layout.verse_start, s1, s3),
            Some((1, 2))
        );
        assert_eq!(
            verses_in_selection(&layout.verse_start, s2, s3 + 1),
            Some((2, 3))
        );
        assert_eq!(
            verses_in_selection(&layout.verse_start, s3, s2),
            None,
            "inverted bounds are empty"
        );
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
    fn copy_verses_range_prefixes_numbers_and_cites_span() {
        let verses = vec![
            Verse {
                book: 1,
                chapter: 1,
                verse: 1,
                text: "In the [beginning] God created. {beginning: Heb. head}".into(),
                para_break: true,
            },
            Verse {
                book: 1,
                chapter: 1,
                verse: 2,
                text: "And the earth was without form.".into(),
                para_break: false,
            },
            Verse {
                book: 1,
                chapter: 1,
                verse: 3,
                text: "And God said, Let there be light.".into(),
                para_break: false,
            },
        ];
        let out = copy_verses(&books(), &verses);
        assert!(out.contains("1 In the beginning God created"), "{out}");
        assert!(out.contains("2 And the earth was without form"), "{out}");
        assert!(out.contains("3 And God said, Let there be light"), "{out}");
        assert!(!out.contains('['), "{out}");
        assert!(!out.contains('{'), "{out}");
        assert!(!out.contains("Heb. head"), "{out}");
        assert!(out.contains("Genesis 1:1–3"), "{out}");
        assert!(out.contains("KJV"), "{out}");
    }

    #[test]
    fn copy_verses_cross_chapter_citation() {
        let verses = vec![
            Verse {
                book: 1,
                chapter: 1,
                verse: 31,
                text: "And God saw every thing.".into(),
                para_break: false,
            },
            Verse {
                book: 1,
                chapter: 2,
                verse: 1,
                text: "Thus the heavens and the earth were finished.".into(),
                para_break: true,
            },
        ];
        let out = copy_verses(&books(), &verses);
        assert!(out.contains("31 And God saw every thing"), "{out}");
        assert!(
            out.contains("1 Thus the heavens and the earth were finished"),
            "{out}"
        );
        assert!(out.contains("Genesis 1:31–2:1"), "{out}");
        assert!(out.contains("KJV"), "{out}");
    }

    #[test]
    fn word_offset_finds_tagged_span() {
        let stored = "In the [beginning] God";
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
            verse_opts(),
        );
        assert!(
            !layout.text.contains("re'shiyth"),
            "app layout must not insert lemmas: {}",
            layout.text
        );
        assert_eq!(layout.words.len(), 1);
        assert!(layout.words[0].lemma_span.is_none());
        let span = layout.words[0].span;
        let mid = (span.start + span.end) / 2;
        let hit = word_at_offset(&layout.words, mid).unwrap();
        assert_eq!(hit.codes, vec!["H7225".to_string()]);
        let shown: String = layout
            .text
            .chars()
            .skip(span.start as usize)
            .take((span.end - span.start) as usize)
            .collect();
        assert_eq!(shown, "beginning");
        assert_eq!(word_surface(&layout.text, span), "beginning");
        assert!(word_at_offset(&layout.words, layout.verse_start[0].1).is_none());
        assert_eq!(
            verse_at_offset(&layout.verse_start, mid),
            Some(1),
            "word click still belongs to its verse"
        );
    }

    #[test]
    fn word_surface_strips_wrapping_punctuation() {
        let span = Span { start: 0, end: 7 };
        assert_eq!(word_surface("(God's)", span), "God's");
        assert_eq!(
            word_surface("beginning", Span { start: 0, end: 9 }),
            "beginning"
        );
    }

    #[test]
    fn token_at_picks_the_clicked_word_in_a_phrase() {
        let text = "and let it divide the waters";
        // "divide" starts at char 11
        assert_eq!(token_at(text, 11), "divide");
        assert_eq!(token_at(text, 16), "divide");
        assert_eq!(token_at(text, 0), "and");
        assert_eq!(token_at(text, 3), "and");
        assert_eq!(token_at(", divide", 0), "");
    }

    #[test]
    fn font_size_steps_and_clamps() {
        assert_eq!(smaller_font(14), 13);
        assert_eq!(larger_font(14), 15);
        assert_eq!(smaller_font(MIN_FONT), MIN_FONT);
        assert_eq!(larger_font(MAX_FONT), MAX_FONT);
    }
}
