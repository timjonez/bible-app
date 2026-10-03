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
/// `chapter:verse` or `chapter:verse–verse`, a chapter alone (`Ge 4`),
/// and Thayer-style roman chapters (`Mt. ii. 4`), with later chapters and
/// verses of the same book after a comma or semicolon (`xvi. 16`; `5, 23`).
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
            if let Some((found, consumed)) = parse_cluster_at(&text[i..], books, dummy) {
                let blocked = found
                    .first()
                    .is_some_and(|f| numbered_book_tail(text, i + f.start, &f.dests, books));
                if !blocked {
                    for f in found {
                        out.push(FoundRef {
                            start: i + f.start,
                            consumed: f.consumed,
                            dests: f.dests,
                        });
                    }
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
    let dests = dests_for(book, chapter, verse, end_verse, books, dummy)?;
    Some((dests, name_len + ws + nums_len))
}

fn parse_cluster_at(s: &str, books: &[Book], dummy: Ref) -> Option<(Vec<FoundRef>, usize)> {
    let (dests, consumed) = parse_dest_at(s, books, dummy)?;
    let first = *dests.first()?;
    let Some(book) = books.iter().find(|b| b.id == first.book) else {
        return Some((
            vec![FoundRef {
                start: 0,
                consumed,
                dests,
            }],
            consumed,
        ));
    };
    let mut refs = vec![FoundRef {
        start: 0,
        consumed,
        dests,
    }];
    let mut pos = consumed;
    let mut last_chapter = first.chapter;
    while let Some(gap) = skip_cluster_gap(&s[pos..]) {
        let at = pos + gap;
        if at >= s.len() {
            break;
        }
        if parse_dest_at(&s[at..], books, dummy).is_some() {
            break;
        }
        let Some((more, nlen, chapter)) =
            parse_same_book_ref(&s[at..], book, last_chapter, books, dummy)
        else {
            break;
        };
        refs.push(FoundRef {
            start: at,
            consumed: nlen,
            dests: more,
        });
        pos = at + nlen;
        last_chapter = chapter;
    }
    Some((refs, pos))
}

fn parse_same_book_ref(
    s: &str,
    book: &Book,
    last_chapter: u8,
    books: &[Book],
    dummy: Ref,
) -> Option<(Vec<Ref>, usize, u8)> {
    if looks_like_new_chapter(s) {
        let (chapter, verse, end_verse, nums_len) = parse_chap_verse(s)?;
        let dests = dests_for(book, chapter, verse, end_verse, books, dummy)?;
        return Some((dests, nums_len, chapter));
    }
    let (verse, end_verse, nums_len) = parse_verse_and_range(s)?;
    let dests = dests_for(book, last_chapter, verse, end_verse, books, dummy)?;
    Some((dests, nums_len, last_chapter))
}

fn dests_for(
    book: &Book,
    chapter: u8,
    verse: u8,
    end_verse: Option<u8>,
    books: &[Book],
    dummy: Ref,
) -> Option<Vec<Ref>> {
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
    Some(dests)
}

fn looks_like_new_chapter(s: &str) -> bool {
    if let Some((_, len)) = parse_roman(s) {
        let rest = &s[len..];
        return rest.starts_with('.') || rest.starts_with(':');
    }
    if let Some((_, len)) = parse_ascii_u8(s) {
        return s[len..].starts_with(':');
    }
    false
}

fn skip_cluster_gap(s: &str) -> Option<usize> {
    let mut i = 0;
    let mut had_sep = false;
    loop {
        let start = i;
        i = skip_ws(s, i);
        i = skip_one_short_paren(s, i);
        if let Some(&b) = s.as_bytes().get(i) {
            if b == b'[' {
                had_sep = true;
                i += 1;
                continue;
            }
            if b == b']' {
                i += 1;
                continue;
            }
            if b == b',' || b == b';' {
                had_sep = true;
                i += 1;
                continue;
            }
        }
        if let Some(n) = apparatus_len(&s[i..]) {
            i += n;
            continue;
        }
        if i == start {
            break;
        }
    }
    had_sep.then_some(i)
}

fn skip_ws(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return i;
    }
    i + s[i..]
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(|c| c.len_utf8())
        .sum::<usize>()
}

fn skip_ws_and_short_parens(s: &str, mut i: usize) -> usize {
    loop {
        let start = i;
        i = skip_ws(s, i);
        i = skip_one_short_paren(s, i);
        if i == start {
            return i;
        }
    }
}

fn skip_one_short_paren(s: &str, i: usize) -> usize {
    if i >= s.len() || !s[i..].starts_with('(') {
        return i;
    }
    let Some(rel) = s[i + 1..].find(')') else {
        return i;
    };
    let inner = &s[i + 1..i + 1 + rel];
    if inner.chars().count() <= 16
        && inner.chars().all(|c| {
            c.is_ascii_digit()
                || roman_value(c).is_some()
                || c.is_whitespace()
                || matches!(c, '.' | ',' | ';' | '-' | '–' | '—')
        })
    {
        i + 1 + rel + 1
    } else {
        i
    }
}

const APPARATUS: &[&str] = &[
    "G L T Tr WH",
    "L T Tr WH",
    "L T Tr",
    "T Tr WH",
    "R G L",
    "WH txt.",
    "WH mrg.",
    "WH txt",
    "WH mrg",
    "R. V.",
    "R.V.",
    "Rec.",
    "Tdf.",
    "Treg.",
    "sqq.",
    "Rec",
    "sq.",
    "al.",
    "etc.",
    "WH",
];

fn apparatus_len(s: &str) -> Option<usize> {
    for token in APPARATUS {
        let n = token.len();
        if s.len() >= n && s.is_char_boundary(n) && s[..n].eq_ignore_ascii_case(token) {
            let bound_ok = s[n..]
                .chars()
                .next()
                .map(|c| !c.is_alphanumeric())
                .unwrap_or(true);
            if bound_ok {
                return Some(n);
            }
        }
    }
    None
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
    ("1 s", "1 Samuel"),
    ("2 sam", "2 Samuel"),
    ("2sam", "2 Samuel"),
    ("2 s", "2 Samuel"),
    ("1 kgs", "1 Kings"),
    ("1kgs", "1 Kings"),
    ("1 k", "1 Kings"),
    ("2 kgs", "2 Kings"),
    ("2kgs", "2 Kings"),
    ("2 k", "2 Kings"),
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
    ("is.", "Isaiah"),
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
    ("jo", "John"),
    ("act", "Acts"),
    ("rom", "Romans"),
    ("1 cor", "1 Corinthians"),
    ("1cor", "1 Corinthians"),
    ("1 co", "1 Corinthians"),
    ("2 cor", "2 Corinthians"),
    ("2cor", "2 Corinthians"),
    ("2 co", "2 Corinthians"),
    ("gal", "Galatians"),
    ("phil", "Philippians"),
    ("1 thess", "1 Thessalonians"),
    ("1thess", "1 Thessalonians"),
    ("1 th", "1 Thessalonians"),
    ("2 thess", "2 Thessalonians"),
    ("2thess", "2 Thessalonians"),
    ("2 th", "2 Thessalonians"),
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
    let (chapter, mut i, roman) = parse_chapter_number(s)?;
    let rest = &s[i..];
    let colon = rest.starts_with(':');
    let dotted = verse_dot(rest);
    if colon || dotted {
        i += 1;
        i = skip_ws_and_short_parens(s, i);
        let (verse, end_verse, vlen) = parse_verse_and_range(&s[i..])?;
        i += vlen;
        Some((chapter, verse, end_verse, i))
    } else if roman {
        None
    } else {
        Some((chapter, 1, None, i))
    }
}

fn verse_dot(rest: &str) -> bool {
    if !rest.starts_with('.') {
        return false;
    }
    let after = skip_ws_and_short_parens(rest, 1);
    rest.as_bytes()
        .get(after)
        .is_some_and(|b| b.is_ascii_digit())
}

fn parse_chapter_number(s: &str) -> Option<(u8, usize, bool)> {
    if let Some((n, len)) = parse_ascii_u8(s) {
        return Some((n, len, false));
    }
    if let Some((n, len)) = parse_roman(s) {
        return Some((n, len, true));
    }
    None
}

fn parse_ascii_u8(s: &str) -> Option<(u8, usize)> {
    let mut i = 0;
    while i < s.len() && s.as_bytes()[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let n: u8 = s[..i].parse().ok()?;
    Some((n, i))
}

fn parse_verse_and_range(s: &str) -> Option<(u8, Option<u8>, usize)> {
    let (verse, mut i) = parse_ascii_u8(s)?;
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
    Some((verse, end_verse, i))
}

fn roman_value(c: char) -> Option<u16> {
    match c.to_ascii_lowercase() {
        'i' => Some(1),
        'v' => Some(5),
        'x' => Some(10),
        'l' => Some(50),
        'c' => Some(100),
        'd' => Some(500),
        'm' => Some(1000),
        _ => None,
    }
}

fn parse_roman(s: &str) -> Option<(u8, usize)> {
    let mut len = 0;
    for c in s.chars() {
        if roman_value(c).is_none() {
            break;
        }
        len += c.len_utf8();
    }
    if len == 0 {
        return None;
    }
    let mut total: i32 = 0;
    let mut prev: i32 = 0;
    for c in s[..len].chars().rev() {
        let v = i32::from(roman_value(c)?);
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    if !(1..=151).contains(&total) {
        return None;
    }
    Some((total as u8, len))
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

    fn thayer_books() -> Vec<Book> {
        let mut v = books();
        v.extend(
            [
                (3, "Le", "Leviticus"),
                (10, "2Sa", "2 Samuel"),
                (14, "2Ch", "2 Chronicles"),
                (23, "Isa", "Isaiah"),
                (35, "Hab", "Habakkuk"),
                (42, "Lu", "Luke"),
                (44, "Ac", "Acts"),
                (45, "Ro", "Romans"),
                (46, "1Co", "1 Corinthians"),
                (50, "Php", "Philippians"),
                (52, "1Th", "1 Thessalonians"),
                (63, "2Jo", "2 John"),
            ]
            .into_iter()
            .map(|(id, abbrev, name)| Book {
                id,
                abbrev: abbrev.into(),
                name: name.into(),
            }),
        );
        v
    }

    fn thayer_cited(text: &str) -> Vec<(String, Ref)> {
        citations(text, &thayer_books())
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

    fn r(book: u8, chapter: u8, verse: u8) -> Ref {
        Ref {
            book,
            chapter,
            verse,
        }
    }

    #[test]
    fn thayer_roman_chapter_and_arabic_verse() {
        assert_eq!(
            thayer_cited("Lev. iv. 5"),
            vec![("Lev. iv. 5".into(), r(3, 4, 5))]
        );
        assert_eq!(
            thayer_cited("Mt. ii. 4"),
            vec![("Mt. ii. 4".into(), r(40, 2, 4))]
        );
        assert_eq!(
            thayer_cited("Hab. iii. 13"),
            vec![("Hab. iii. 13".into(), r(35, 3, 13))]
        );
        assert_eq!(
            thayer_cited("1 Jn. v. 1"),
            vec![("1 Jn. v. 1".into(), r(62, 5, 1))]
        );
        assert_eq!(
            thayer_cited("Acts ii. 36"),
            vec![("Acts ii. 36".into(), r(44, 2, 36))]
        );
    }

    #[test]
    fn thayer_same_book_continues_after_semicolon_or_comma() {
        assert_eq!(
            thayer_cited("Mt. ii. 4; xvi. 16; xxiii. 10; xxiv. 5, 23; xxvi. 63"),
            vec![
                ("Mt. ii. 4".into(), r(40, 2, 4)),
                ("xvi. 16".into(), r(40, 16, 16)),
                ("xxiii. 10".into(), r(40, 23, 10)),
                ("xxiv. 5".into(), r(40, 24, 5)),
                ("23".into(), r(40, 24, 23)),
                ("xxvi. 63".into(), r(40, 26, 63)),
            ]
        );
        assert_eq!(
            thayer_cited("Lev. iv. 5; vi. 22"),
            vec![
                ("Lev. iv. 5".into(), r(3, 4, 5)),
                ("vi. 22".into(), r(3, 6, 22)),
            ]
        );
        assert_eq!(
            thayer_cited("1 S. ii. 10, 35"),
            vec![
                ("1 S. ii. 10".into(), r(9, 2, 10)),
                ("35".into(), r(9, 2, 35)),
            ]
        );
    }

    #[test]
    fn thayer_skips_lxx_chapter_in_parentheses() {
        assert_eq!(
            thayer_cited("Ps. civ. (cv.) 15"),
            vec![("Ps. civ. (cv.) 15".into(), r(19, 104, 15))]
        );
        assert_eq!(
            thayer_cited("Ps. xvii. (xviii.) 51"),
            vec![("Ps. xvii. (xviii.) 51".into(), r(19, 17, 51))]
        );
    }

    #[test]
    fn thayer_book_aliases_and_a_new_book_breaks_the_cluster() {
        assert_eq!(
            thayer_cited("Is. xlv. 1"),
            vec![("Is. xlv. 1".into(), r(23, 45, 1))]
        );
        assert_eq!(
            thayer_cited("1 Co. iii. 11"),
            vec![("1 Co. iii. 11".into(), r(46, 3, 11))]
        );
        assert_eq!(
            thayer_cited("1 Th. iv. 3"),
            vec![("1 Th. iv. 3".into(), r(52, 4, 3))]
        );
        assert_eq!(
            thayer_cited("1 K. iii. 24"),
            vec![("1 K. iii. 24".into(), r(11, 3, 24))]
        );
        assert_eq!(
            thayer_cited("Jo. viii. 59"),
            vec![("Jo. viii. 59".into(), r(43, 8, 59))]
        );
        assert_eq!(
            thayer_cited("Jn. xv. 13; Ro. xiii. 10"),
            vec![
                ("Jn. xv. 13".into(), r(43, 15, 13)),
                ("Ro. xiii. 10".into(), r(45, 13, 10)),
            ]
        );
        assert_eq!(
            thayer_cited("Hab. iii. 13; [2 Chr. xxii. 7]"),
            vec![
                ("Hab. iii. 13".into(), r(35, 3, 13)),
                ("2 Chr. xxii. 7".into(), r(14, 22, 7)),
            ]
        );
        assert_eq!(
            thayer_cited("1 S. ii. 10, 35; [xxiv. 11; xxvi. 9]; 2 S. i. 14"),
            vec![
                ("1 S. ii. 10".into(), r(9, 2, 10)),
                ("35".into(), r(9, 2, 35)),
                ("xxiv. 11".into(), r(9, 24, 11)),
                ("xxvi. 9".into(), r(9, 26, 9)),
                ("2 S. i. 14".into(), r(10, 1, 14)),
            ]
        );
    }

    #[test]
    fn thayer_range_and_apparatus_then_another_verse() {
        assert_eq!(
            thayer_cited("Phil. i. 19–21"),
            vec![("Phil. i. 19–21".into(), r(50, 1, 19))]
        );
        assert_eq!(
            thayer_cited("Acts ii. 30 Rec., 31"),
            vec![
                ("Acts ii. 30".into(), r(44, 2, 30)),
                ("31".into(), r(44, 2, 31)),
            ]
        );
        assert_eq!(
            thayer_cited("Mt. vi. 1 Rec., 2, 3"),
            vec![
                ("Mt. vi. 1".into(), r(40, 6, 1)),
                ("2".into(), r(40, 6, 2)),
                ("3".into(), r(40, 6, 3)),
            ]
        );
    }

    #[test]
    fn roman_without_a_book_or_english_is_are_not_citations() {
        assert!(thayer_cited("Keim ii. 549").is_empty());
        assert!(thayer_cited("is 4 people here").is_empty());
        assert!(thayer_cited("I. The birth of Cain.").is_empty());
    }
}
