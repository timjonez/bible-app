use crate::nav::{self, Ref};
use bible_app_db::Book;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TskPhrase {
    pub heading: String,
    pub dests: Vec<Ref>,
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
                    out.push(FoundRef { start: i, dests });
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

fn match_book_prefix<'a>(s: &'a str, books: &'a [Book]) -> Option<(&'a Book, usize)> {
    let mut best: Option<(&Book, usize)> = None;
    for b in books {
        let name = b.name.as_str();
        if name.is_empty() || s.len() < name.len() || !s.is_char_boundary(name.len()) {
            continue;
        }
        if !s[..name.len()].eq_ignore_ascii_case(name) {
            continue;
        }
        let bound_ok = s[name.len()..]
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
        if bound_ok && best.is_none_or(|(_, k)| name.len() > k) {
            best = Some((b, name.len()));
        }
    }
    best
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
        return None;
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
            (11, "1Ki", "1 Kings"),
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
}
