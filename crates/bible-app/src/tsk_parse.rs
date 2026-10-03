use crate::nav::{self, Ref};
use bible_app_db::Book;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TskPhrase {
    pub heading: String,
    pub dests: Vec<Ref>,
}

/// One scripture citation in Treasury text. Offsets are characters, for a text buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Citation {
    pub start: i32,
    pub end: i32,
    pub at: Ref,
}

/// Every scripture citation in `text`, including a range as one link to its first verse.
///
/// Accepts a full book name or abbreviation, an optional period (`Ps. 34:1`),
/// `chapter:verse` or `chapter:verse–verse`, and a chapter alone (`Ge 4`).
pub fn citations(text: &str, books: &[Book]) -> Vec<Citation> {
    extract_refs(text, books)
        .into_iter()
        .filter_map(|found| {
            let end_byte = found.start + found.consumed;
            if !text.is_char_boundary(found.start) || !text.is_char_boundary(end_byte) {
                return None;
            }
            let start = text[..found.start].chars().count() as i32;
            let end = start + text[found.start..end_byte].chars().count() as i32;
            let at = *found.dests.first()?;
            Some(Citation { start, end, at })
        })
        .collect()
}

/// Pango markup with each citation wrapped in `bible:book/chapter/verse` links.
pub fn markup_with_cites(text: &str, books: &[Book]) -> String {
    let links = citations(text, books);
    if links.is_empty() {
        return escape_markup(text);
    }
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    for link in &links {
        let start = link.start as usize;
        let end = link.end as usize;
        if start > chars.len() || end > chars.len() || start >= end {
            continue;
        }
        if start > i {
            out.push_str(&escape_markup(&chars[i..start].iter().collect::<String>()));
        }
        let label: String = chars[start..end].iter().collect();
        out.push_str(&format!(
            "<a href=\"{}\">{}</a>",
            cite_href(link.at),
            escape_markup(&label)
        ));
        i = end;
    }
    if i < chars.len() {
        out.push_str(&escape_markup(&chars[i..].iter().collect::<String>()));
    }
    out
}

pub fn cite_href(at: Ref) -> String {
    format!("bible:{}/{}/{}", at.book, at.chapter, at.verse)
}

pub fn parse_cite_href(uri: &str) -> Option<Ref> {
    let rest = uri.strip_prefix("bible:")?;
    let mut parts = rest.split('/');
    Some(Ref {
        book: parts.next()?.parse().ok()?,
        chapter: parts.next()?.parse().ok()?,
        verse: parts.next()?.parse().ok()?,
    })
}

fn escape_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Parse TSK `* heading. refs` lines into phrase keys with destinations.
pub fn parse_tsk_phrases(text: &str, books: &[Book]) -> Vec<TskPhrase> {
    let mut out = Vec::new();
    for block in star_blocks(text) {
        out.extend(parse_star_block(&block, books));
    }
    out
}

fn star_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix('*') {
            let rest = rest.trim_start();
            if rest.to_ascii_lowercase().starts_with("gr:") {
                if let Some(cur) = current.as_mut() {
                    cur.push(' ');
                    cur.push_str(trimmed);
                }
                continue;
            }
            if let Some(cur) = current.take() {
                blocks.push(cur);
            }
            current = Some(rest.to_string());
        } else if let Some(cur) = current.as_mut() {
            if !trimmed.is_empty() {
                cur.push(' ');
                cur.push_str(trimmed);
            }
        }
    }
    if let Some(cur) = current {
        blocks.push(cur);
    }
    blocks
}

struct FoundRef {
    start: usize,
    consumed: usize,
    dests: Vec<Ref>,
}

fn parse_star_block(block: &str, books: &[Book]) -> Vec<TskPhrase> {
    let refs = extract_refs(block, books);
    let mut headings: Vec<(String, usize)> = Vec::new();
    let mut in_note = false;
    let mut pos = 0usize;
    for part in block.split_inclusive('.') {
        let start = pos;
        let end = pos + part.len();
        pos = end;
        let core = part.trim_matches(|c: char| c == '.' || c.is_whitespace());
        if core.is_empty() {
            continue;
        }
        if is_note_intro(core) {
            in_note = true;
            continue;
        }
        if segment_starts_with_ref(block, start, &refs) {
            in_note = false;
            continue;
        }
        if in_note {
            in_note = false;
            continue;
        }
        headings.push((core.to_string(), end));
    }

    let mut phrases = Vec::new();
    for (i, (heading, hend)) in headings.iter().enumerate() {
        if !usable_heading(heading) {
            continue;
        }
        let limit = headings
            .get(i + 1)
            .and_then(|(next, _)| block[*hend..].find(next.as_str()).map(|rel| *hend + rel))
            .unwrap_or(block.len());
        let dests: Vec<Ref> = refs
            .iter()
            .filter(|r| r.start >= *hend && r.start < limit)
            .flat_map(|r| r.dests.iter().copied())
            .collect();
        if dests.is_empty() {
            continue;
        }
        phrases.push(TskPhrase {
            heading: heading.clone(),
            dests,
        });
    }
    phrases
}

fn usable_heading(s: &str) -> bool {
    let t = s.trim();
    if t.chars().count() < 3 {
        return false;
    }
    if t.eq_ignore_ascii_case("the") {
        return false;
    }
    t.chars().any(|c| c.is_alphabetic())
}

fn is_note_intro(s: &str) -> bool {
    let lower: String = s.trim().chars().flat_map(char::to_lowercase).collect();
    lower == "heb"
        || lower.starts_with("heb ")
        || lower.starts_with("heb.")
        || lower == "or"
        || lower.starts_with("or,")
        || lower.starts_with("or ")
        || lower.starts_with("gr:")
        || lower.starts_with("*gr")
}

fn segment_starts_with_ref(block: &str, start: usize, refs: &[FoundRef]) -> bool {
    let skip = block[start..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum::<usize>();
    let at = start + skip;
    refs.iter().any(|r| r.start == at)
}

fn extract_refs(text: &str, books: &[Book]) -> Vec<FoundRef> {
    let dummy = Ref {
        book: 1,
        chapter: 1,
        verse: 1,
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with("*Gr:") || text[i..].starts_with("Gr:") {
            if let Some(rel) = text[i..].find('|') {
                i += rel + 1;
                continue;
            }
        }
        if text[i..].starts_with('<') {
            if let Some(rel) = text[i..].find('>') {
                i += rel + 1;
                continue;
            }
        }
        if at_token_start(text, i) {
            if let Some((dests, consumed)) = parse_dest_at(&text[i..], books, dummy) {
                if !numbered_book_tail(text, i, &dests, books) {
                    out.push(FoundRef {
                        start: i,
                        consumed,
                        dests,
                    });
                    i += consumed;
                    continue;
                }
            }
        }
        i += text[i..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
    }
    out
}

fn at_token_start(text: &str, i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let Some(prev) = text[..i].chars().last() else {
        return true;
    };
    !prev.is_alphanumeric()
}

fn numbered_book_tail(text: &str, i: usize, dests: &[Ref], books: &[Book]) -> bool {
    let Some(first) = dests.first() else {
        return false;
    };
    let Some(book) = books.iter().find(|b| b.id == first.book) else {
        return false;
    };
    if book
        .name
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        return false;
    }
    let prefix = &text[..i];
    if !prefix
        .chars()
        .last()
        .map(|c| c.is_whitespace())
        .unwrap_or(false)
    {
        return false;
    }
    let trimmed = prefix.trim_end();
    let Some(digit) = trimmed.chars().last() else {
        return false;
    };
    if !matches!(digit, '1' | '2' | '3') {
        return false;
    }
    if trimmed.chars().nth_back(1) == Some(':') {
        return false;
    }
    let candidate = format!("{digit} {}", book.name);
    books
        .iter()
        .any(|b| b.name.eq_ignore_ascii_case(&candidate))
}

fn parse_dest_at(s: &str, books: &[Book], dummy: Ref) -> Option<(Vec<Ref>, usize)> {
    let (book, name_len) = match_book_prefix(s, books)?;
    let after_name = &s[name_len..];
    let ws: usize = after_name
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum();
    if ws == 0 {
        return None;
    }
    let nums = &after_name[ws..];
    let (chapter, verse, end_verse, nums_len) = parse_chap_verse(nums)?;
    let token = format!("{} {chapter}:{verse}", book.name);
    let at = nav::parse_ref(&token, books, dummy)?;
    if at.book != book.id {
        return None;
    }
    let mut dests = vec![at];
    if let Some(ev) = end_verse {
        if ev > at.verse {
            let last = ev.min(at.verse.saturating_add(40));
            for v in at.verse + 1..=last {
                dests.push(Ref { verse: v, ..at });
            }
        }
    }
    Some((dests, name_len + ws + nums_len))
}

/// Extra spellings used in commentaries and dictionaries, mapped to `Book.name`.
const BOOK_ALIASES: &[(&str, &str)] = &[
    ("gen", "Genesis"),
    ("gn", "Genesis"),
    ("exo", "Exodus"),
    ("exod", "Exodus"),
    ("lev", "Leviticus"),
    ("num", "Numbers"),
    ("deut", "Deuteronomy"),
    ("dt", "Deuteronomy"),
    ("josh", "Joshua"),
    ("judg", "Judges"),
    ("jdg", "Judges"),
    ("1 sam", "1 Samuel"),
    ("1sam", "1 Samuel"),
    ("2 sam", "2 Samuel"),
    ("2sam", "2 Samuel"),
    ("1 kgs", "1 Kings"),
    ("1kgs", "1 Kings"),
    ("2 kgs", "2 Kings"),
    ("2kgs", "2 Kings"),
    ("1 chr", "1 Chronicles"),
    ("1chr", "1 Chronicles"),
    ("2 chr", "2 Chronicles"),
    ("2chr", "2 Chronicles"),
    ("neh", "Nehemiah"),
    ("esth", "Esther"),
    ("est", "Esther"),
    ("psa", "Psalms"),
    ("psalm", "Psalms"),
    ("prov", "Proverbs"),
    ("ecc", "Ecclesiastes"),
    ("eccl", "Ecclesiastes"),
    ("cant", "Song of Solomon"),
    ("canticles", "Song of Solomon"),
    ("sos", "Song of Solomon"),
    ("isa", "Isaiah"),
    ("jer", "Jeremiah"),
    ("lam", "Lamentations"),
    ("eze", "Ezekiel"),
    ("ezek", "Ezekiel"),
    ("dan", "Daniel"),
    ("hos", "Hosea"),
    ("obad", "Obadiah"),
    ("jonah", "Jonah"),
    ("nah", "Nahum"),
    ("zeph", "Zephaniah"),
    ("zech", "Zechariah"),
    ("matt", "Matthew"),
    ("mat", "Matthew"),
    ("mk", "Mark"),
    ("luk", "Luke"),
    ("lk", "Luke"),
    ("jn", "John"),
    ("act", "Acts"),
    ("rom", "Romans"),
    ("1 cor", "1 Corinthians"),
    ("1cor", "1 Corinthians"),
    ("2 cor", "2 Corinthians"),
    ("2cor", "2 Corinthians"),
    ("gal", "Galatians"),
    ("phil", "Philippians"),
    ("1 thess", "1 Thessalonians"),
    ("1thess", "1 Thessalonians"),
    ("2 thess", "2 Thessalonians"),
    ("2thess", "2 Thessalonians"),
    ("1 tim", "1 Timothy"),
    ("1tim", "1 Timothy"),
    ("2 tim", "2 Timothy"),
    ("2tim", "2 Timothy"),
    ("phlm", "Philemon"),
    ("1 pet", "1 Peter"),
    ("1pet", "1 Peter"),
    ("2 pet", "2 Peter"),
    ("2pet", "2 Peter"),
    ("1 jn", "1 John"),
    ("1jn", "1 John"),
    ("2 jn", "2 John"),
    ("2jn", "2 John"),
    ("3 jn", "3 John"),
    ("3jn", "3 John"),
    ("rev", "Revelation"),
];

fn match_book_prefix<'a>(s: &'a str, books: &'a [Book]) -> Option<(&'a Book, usize)> {
    let mut best: Option<(&Book, usize)> = None;
    for b in books {
        consider_token(s, b.name.as_str(), b, &mut best);
        consider_token(s, b.abbrev.as_str(), b, &mut best);
    }
    for (alias, name) in BOOK_ALIASES {
        let Some(book) = books.iter().find(|b| b.name.eq_ignore_ascii_case(name)) else {
            continue;
        };
        consider_token(s, alias, book, &mut best);
    }
    best
}

fn consider_token<'a>(
    s: &'a str,
    token: &str,
    book: &'a Book,
    best: &mut Option<(&'a Book, usize)>,
) {
    let n = token.len();
    if n == 0 || s.len() < n || !s.is_char_boundary(n) {
        return;
    }
    if !s[..n].eq_ignore_ascii_case(token) {
        return;
    }
    let mut consumed = n;
    if s[consumed..].starts_with('.') {
        consumed += 1;
    }
    let bound_ok = s[consumed..]
        .chars()
        .next()
        .map(|c| !c.is_alphanumeric())
        .unwrap_or(true);
    if bound_ok && best.is_none_or(|(_, k)| consumed > k) {
        *best = Some((book, consumed));
    }
}

fn parse_chap_verse(s: &str) -> Option<(u8, u8, Option<u8>, usize)> {
    let mut i = 0;
    while i < s.len() && s.as_bytes()[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let chapter: u8 = s[..i].parse().ok()?;
    if i >= s.len() || s.as_bytes()[i] != b':' {
        return Some((chapter, 1, None, i));
    }
    i += 1;
    let vstart = i;
    while i < s.len() && s.as_bytes()[i].is_ascii_digit() {
        i += 1;
    }
    if i == vstart {
        return None;
    }
    let verse: u8 = s[vstart..i].parse().ok()?;
    let mut end_verse = None;
    if i < s.len() {
        let rest = &s[i..];
        let dash = if rest.starts_with('–') || rest.starts_with('—') || rest.starts_with('-') {
            rest.chars().next().map(|c| c.len_utf8())
        } else {
            None
        };
        if let Some(dash_len) = dash {
            let after = &rest[dash_len..];
            let mut j = 0;
            while j < after.len() && after.as_bytes()[j].is_ascii_digit() {
                j += 1;
            }
            if j > 0 {
                if after.as_bytes().get(j) == Some(&b':') {
                    let mut k = j + 1;
                    while k < after.len() && after.as_bytes()[k].is_ascii_digit() {
                        k += 1;
                    }
                    i += dash_len + k;
                } else {
                    end_verse = after[..j].parse().ok();
                    i += dash_len + j;
                }
            }
        }
    }
    Some((chapter, verse, end_verse, i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn books() -> Vec<Book> {
        [
            (1, "Ge", "Genesis"),
            (2, "Ex", "Exodus"),
            (5, "De", "Deuteronomy"),
            (9, "1Sa", "1 Samuel"),
            (11, "1Ki", "1 Kings"),
            (13, "1Ch", "1 Chronicles"),
            (18, "Job", "Job"),
            (19, "Ps", "Psalms"),
            (20, "Pr", "Proverbs"),
            (40, "Mt", "Matthew"),
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

    fn headings(text: &str) -> Vec<String> {
        parse_tsk_phrases(text, &books())
            .into_iter()
            .map(|p| p.heading)
            .collect()
    }

    #[test]
    fn genesis_1_1_headings() {
        let text = "\
God creates heaven and earth.

* beginning. Proverbs 8:22–24 Proverbs 16:4 Mark 13:19 John 1:1–3 Hebrews 1:10 1 John 1:1
* God. Exodus 20:11 Exodus 31:18 1 Chronicles 16:26";
        let phrases = parse_tsk_phrases(text, &books());
        let names: Vec<_> = phrases.iter().map(|p| p.heading.as_str()).collect();
        assert_eq!(names, ["beginning", "God"]);
        assert!(phrases[0].dests.contains(&Ref {
            book: 20,
            chapter: 8,
            verse: 22
        }));
        assert!(phrases[0].dests.contains(&Ref {
            book: 20,
            chapter: 8,
            verse: 24
        }));
        assert!(phrases[0].dests.contains(&Ref {
            book: 62,
            chapter: 1,
            verse: 1
        }));
        assert!(phrases[1].dests.contains(&Ref {
            book: 2,
            chapter: 20,
            verse: 11
        }));
        assert!(phrases[1].dests.contains(&Ref {
            book: 13,
            chapter: 16,
            verse: 26
        }));
    }

    #[test]
    fn one_john_not_john() {
        let text = "* the beginning. John 1:2 1 John 1:1";
        let phrases = parse_tsk_phrases(text, &books());
        assert_eq!(phrases[0].heading, "the beginning");
        assert!(phrases[0].dests.contains(&Ref {
            book: 43,
            chapter: 1,
            verse: 2
        }));
        assert!(phrases[0].dests.contains(&Ref {
            book: 62,
            chapter: 1,
            verse: 1
        }));
        assert!(!phrases[0]
            .dests
            .iter()
            .any(|d| d.book == 43 && d.verse == 1));
    }

    #[test]
    fn heb_only_heading_is_skipped() {
        let text = "* Let there. Genesis 1:14\n* firmament. Heb. expansion.";
        assert_eq!(headings(text), ["Let there"]);
    }

    #[test]
    fn grass_heb_then_fruit_with_refs() {
        let text = "* grass. Heb. tender grass. fruit. Genesis 1:29 Genesis 2:9";
        let phrases = parse_tsk_phrases(text, &books());
        assert_eq!(phrases.len(), 1);
        assert_eq!(phrases[0].heading, "fruit");
        assert!(phrases[0].dests.contains(&Ref {
            book: 1,
            chapter: 1,
            verse: 29
        }));
    }

    #[test]
    fn heb_note_then_refs_keep_heading() {
        let text = "* to rule. Heb. for the rule, etc. Deuteronomy 4:19 Job 31:26";
        let phrases = parse_tsk_phrases(text, &books());
        assert_eq!(phrases.len(), 1);
        assert_eq!(phrases[0].heading, "to rule");
        assert!(phrases[0].dests.contains(&Ref {
            book: 5,
            chapter: 4,
            verse: 19
        }));
    }

    #[test]
    fn john_1_1_phrases() {
        let text = "\
1 The divinity of Jesus Christ.

* the beginning. John 1:2 Genesis 1:1 Proverbs 8:22–31
* the Word. John 1:14 1 John 1:1
* with. John 1:18
* the Word was. John 10:30–33 2 Peter 1:1*Gr:| 1 John 5:7";
        let names = headings(text);
        assert_eq!(names, ["the beginning", "the Word", "with", "the Word was"]);
        let phrases = parse_tsk_phrases(text, &books());
        let last = phrases.last().unwrap();
        assert!(last.dests.contains(&Ref {
            book: 62,
            chapter: 5,
            verse: 7
        }));
    }

    #[test]
    fn ignores_prose_before_star() {
        let text = "God creates heaven and earth;\n3 the light;\n* beginning. Proverbs 8:22";
        assert_eq!(headings(text), ["beginning"]);
    }

    #[test]
    fn or_note_without_refs_skipped() {
        let text = "* he made the stars also. Or, with the stars also.";
        assert!(parse_tsk_phrases(text, &books()).is_empty());
    }

    fn cited(text: &str) -> Vec<(String, Ref)> {
        citations(text, &books())
            .into_iter()
            .map(|link| {
                let label: String = text
                    .chars()
                    .skip(link.start as usize)
                    .take((link.end - link.start) as usize)
                    .collect();
                (label, link.at)
            })
            .collect()
    }

    #[test]
    fn citations_cover_the_whole_reference_including_a_range() {
        let text = "* beginning. Proverbs 8:22–24 Mark 13:19 1 John 1:1";
        assert_eq!(
            cited(text),
            vec![
                (
                    "Proverbs 8:22–24".into(),
                    Ref {
                        book: 20,
                        chapter: 8,
                        verse: 22
                    }
                ),
                (
                    "Mark 13:19".into(),
                    Ref {
                        book: 41,
                        chapter: 13,
                        verse: 19
                    }
                ),
                (
                    "1 John 1:1".into(),
                    Ref {
                        book: 62,
                        chapter: 1,
                        verse: 1
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_verse_range_is_one_link_to_its_first_verse() {
        let text = "John 1:1–3";
        let links = citations(text, &books());
        assert_eq!(links.len(), 1);
        assert_eq!(
            links[0].at,
            Ref {
                book: 43,
                chapter: 1,
                verse: 1
            }
        );
        assert_eq!(cited(text)[0].0, "John 1:1–3");
    }

    #[test]
    fn prose_without_a_citation_has_no_link() {
        assert!(citations("God creates heaven and earth.", &books()).is_empty());
        assert!(citations("* firmament. Heb. expansion.", &books()).is_empty());
    }

    #[test]
    fn stacked_notes_keep_citation_offsets() {
        let first = "* beginning. Proverbs 8:22–24 Mark 13:19";
        let second = "* God. Exodus 20:11";
        let sections = [("Genesis 1:1", first), ("Genesis 1:2", second)];
        let stacked = nav::stack_sections(&sections);
        let chars: Vec<char> = stacked.text.chars().collect();
        let expect = ["Proverbs 8:22–24", "Mark 13:19", "Exodus 20:11"];
        let mut found = Vec::new();
        for (i, (_, body)) in sections.iter().enumerate() {
            let body_at = stacked.heading_at[i] + stacked.heading_len[i] + 2;
            for link in citations(body.trim(), &books()) {
                let start = (body_at + link.start) as usize;
                let end = (body_at + link.end) as usize;
                found.push(chars[start..end].iter().collect::<String>());
            }
        }
        assert_eq!(found, expect);
    }

    #[test]
    fn mhc_prose_citations_use_full_names_and_ranges() {
        let text = "\
I. The birth, names, and callings, of Cain and Abel, Genesis 4:1–2.
II. Their religion (Genesis 4:3–4) and part of, Genesis 4:5.
as Samuel, when he said, 1 Samuel 16:6. The name, Psalms 39:5.";
        assert_eq!(
            cited(text),
            vec![
                (
                    "Genesis 4:1–2".into(),
                    Ref {
                        book: 1,
                        chapter: 4,
                        verse: 1
                    }
                ),
                (
                    "Genesis 4:3–4".into(),
                    Ref {
                        book: 1,
                        chapter: 4,
                        verse: 3
                    }
                ),
                (
                    "Genesis 4:5".into(),
                    Ref {
                        book: 1,
                        chapter: 4,
                        verse: 5
                    }
                ),
                (
                    "1 Samuel 16:6".into(),
                    Ref {
                        book: 9,
                        chapter: 16,
                        verse: 6
                    }
                ),
                (
                    "Psalms 39:5".into(),
                    Ref {
                        book: 19,
                        chapter: 39,
                        verse: 5
                    }
                ),
            ]
        );
    }

    #[test]
    fn abbreviation_and_chapter_only_citations() {
        assert_eq!(
            cited("Cain (Ge 4) settled in Nod."),
            vec![(
                "Ge 4".into(),
                Ref {
                    book: 1,
                    chapter: 4,
                    verse: 1
                }
            )]
        );
        assert_eq!(
            cited("See Ps. 34."),
            vec![(
                "Ps. 34".into(),
                Ref {
                    book: 19,
                    chapter: 34,
                    verse: 1
                }
            )]
        );
        assert_eq!(
            cited("Matt. 5:21–22"),
            vec![(
                "Matt. 5:21–22".into(),
                Ref {
                    book: 40,
                    chapter: 5,
                    verse: 21
                }
            )]
        );
    }

    #[test]
    fn stacked_document_links_headings_and_bodies() {
        let first = "See Proverbs 8:22.";
        let second = "See Exodus 20:11.";
        let stacked = nav::stack_sections(&[("Genesis 1:1", first), ("Genesis 1:2", second)]);
        let found: Vec<String> = citations(&stacked.text, &books())
            .into_iter()
            .map(|link| {
                stacked
                    .text
                    .chars()
                    .skip(link.start as usize)
                    .take((link.end - link.start) as usize)
                    .collect()
            })
            .collect();
        assert_eq!(
            found,
            [
                "Genesis 1:1",
                "Proverbs 8:22",
                "Genesis 1:2",
                "Exodus 20:11"
            ]
        );
    }

    #[test]
    fn markup_wraps_citations_and_escapes_the_rest() {
        let markup = markup_with_cites("Cain <and> Ge 4 & Abel.", &books());
        assert_eq!(
            markup,
            "Cain &lt;and&gt; <a href=\"bible:1/4/1\">Ge 4</a> &amp; Abel."
        );
        assert_eq!(
            parse_cite_href("bible:1/4/1"),
            Some(Ref {
                book: 1,
                chapter: 4,
                verse: 1
            })
        );
    }
}
