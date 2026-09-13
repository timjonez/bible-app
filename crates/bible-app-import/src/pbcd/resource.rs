//! Commentary / dictionary body (`*.ct4`) plus per-verse index (`*.ct7`).
//!
//! `ct7` is 8 bytes per KJV verse: `u32le` start and `u32le` end of the
//! ASCII verse label in `ct4` (`0xFFFFFFFF` = no entry). The comment text
//! runs from `end` to the next present entry's `start`.

use super::bible::{BookName, VerseRec};
use super::Error;
use encoding_rs::WINDOWS_1252;
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

const NONE: u32 = 0xFFFFFFFF;
const REF: u8 = 0x03;
const TRANSLIT: u8 = 0x04;
const ALT: u8 = 0x05;
const ITALIC: u8 = 0x06;
const HEADING: u8 = 0x07;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceEntry {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
}

pub struct ResourceModule {
    pub id: String,
    pub title: String,
    pub entries: Vec<ResourceEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    verse_i: usize,
    start: u32,
    end: u32,
}

pub fn load_commentary(
    dir: &Path,
    stem: &str,
    kjv_index: &[VerseRec],
    books: &[BookName],
) -> Result<ResourceModule, Error> {
    let header = super::header::ModuleHeader::read(&dir.join(format!("{stem}.ct0")))?;
    let index = std::fs::read(dir.join(format!("{stem}.ct7")))?;
    let file = File::open(dir.join(format!("{stem}.ct4")))?;
    let text = unsafe { Mmap::map(&file)? };
    let entries = parse_entries(kjv_index, &index, &text, |hex| {
        hex_label(hex, kjv_index, books)
    })?;
    Ok(ResourceModule {
        id: header.id,
        title: header.title,
        entries,
    })
}

pub fn parse_entries(
    kjv_index: &[VerseRec],
    ct7: &[u8],
    ct4: &[u8],
    resolve: impl Fn(u32) -> Option<String>,
) -> Result<Vec<ResourceEntry>, Error> {
    let (chunks, rest) = ct7.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(Error::Format(format!(
            "resource index length {} is not a multiple of 8",
            ct7.len()
        )));
    }
    let n = chunks.len().min(kjv_index.len());
    let mut spans = Vec::new();
    for (i, chunk) in chunks.iter().take(n).enumerate() {
        let start = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
        let end = u32::from_le_bytes(chunk[4..8].try_into().unwrap());
        if start == NONE {
            continue;
        }
        let start_u = start as usize;
        let end_u = end as usize;
        if start_u > end_u || end_u > ct4.len() {
            return Err(Error::Format(format!(
                "resource span {start}..{end} out of range {}",
                ct4.len()
            )));
        }
        spans.push(Span {
            verse_i: i,
            start,
            end,
        });
    }

    let mut entries = Vec::with_capacity(spans.len());
    for (k, span) in spans.iter().enumerate() {
        let text_start = span.end as usize;
        let text_end = spans
            .get(k + 1)
            .map(|s| s.start as usize)
            .unwrap_or(ct4.len());
        if text_start > text_end {
            return Err(Error::Format("resource text range inverted".into()));
        }
        let rec = kjv_index[span.verse_i];
        let text = decode_markup(&ct4[text_start..text_end], &resolve);
        if text.trim().is_empty() {
            continue;
        }
        entries.push(ResourceEntry {
            book: rec.book,
            chapter: rec.chapter,
            verse: rec.verse,
            text,
        });
    }
    Ok(entries)
}

pub fn decode_markup(raw: &[u8], resolve: impl Fn(u32) -> Option<String>) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            0x00 | 0x0D => i += 1,
            0x0A => {
                push_nl(&mut out);
                i += 1;
            }
            REF => {
                i += 1;
                let start = i;
                while i < raw.len() && raw[i] != REF {
                    i += 1;
                }
                let body = &raw[start..i];
                if i < raw.len() {
                    i += 1;
                }
                push_ref(&mut out, body, &resolve);
            }
            ITALIC | HEADING | TRANSLIT | ALT => i += 1,
            _ => {
                let start = i;
                while i < raw.len() && !is_markup(raw[i]) {
                    i += 1;
                }
                out.push_str(&cp1252(&raw[start..i]));
            }
        }
    }
    collapse_blank(&out)
}

fn is_markup(b: u8) -> bool {
    matches!(
        b,
        0x00 | 0x0A | 0x0D | REF | TRANSLIT | ALT | ITALIC | HEADING
    )
}

fn push_nl(out: &mut String) {
    if out.ends_with('\n') {
        if !out.ends_with("\n\n") {
            out.push('\n');
        }
    } else if !out.is_empty() {
        out.push('\n');
    }
}

fn push_ref(out: &mut String, body: &[u8], resolve: impl Fn(u32) -> Option<String>) {
    let s = std::str::from_utf8(body).unwrap_or("").trim();
    if s.is_empty() {
        return;
    }
    let label = if let Some((a, b)) = s.split_once('-') {
        match (
            u32::from_str_radix(a.trim(), 16),
            u32::from_str_radix(b.trim(), 16),
        ) {
            (Ok(x), Ok(y)) => match (resolve(x), resolve(y)) {
                (Some(l), Some(r)) if l == r => l,
                (Some(l), Some(r)) => compact_range(&l, &r),
                (Some(l), None) => l,
                _ => s.to_string(),
            },
            _ => s.to_string(),
        }
    } else if let Ok(n) = u32::from_str_radix(s, 16) {
        resolve(n).unwrap_or_else(|| s.to_string())
    } else {
        s.to_string()
    };
    if !out.is_empty() && !out.ends_with([' ', '\n', '(']) {
        out.push(' ');
    }
    out.push_str(&label);
}

fn cp1252(bytes: &[u8]) -> String {
    WINDOWS_1252.decode(bytes).0.into_owned()
}

fn collapse_blank(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out.trim().to_string()
}

fn compact_range(left: &str, right: &str) -> String {
    let Some((lbook, lcv)) = left.rsplit_once(' ') else {
        return format!("{left}–{right}");
    };
    let Some((rbook, rcv)) = right.rsplit_once(' ') else {
        return format!("{left}–{right}");
    };
    if lbook != rbook {
        return format!("{left}–{right}");
    }
    match (lcv.split_once(':'), rcv.split_once(':')) {
        (Some((lc, lv)), Some((rc, rv))) if lc == rc => {
            format!("{lbook} {lc}:{lv}–{rv}")
        }
        _ => format!("{left}–{right}"),
    }
}

fn hex_label(hex: u32, index: &[VerseRec], books: &[BookName]) -> Option<String> {
    let rec = index.get(hex.checked_sub(1)? as usize)?;
    let name = books
        .iter()
        .find(|b| b.id == rec.book)
        .map(|b| b.name.as_str())
        .unwrap_or("Book");
    Some(format!("{name} {}:{}", rec.chapter, rec.verse))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(book: u8, chapter: u8, verse: u8) -> VerseRec {
        VerseRec {
            offset: 0,
            book,
            chapter,
            verse,
        }
    }

    #[test]
    fn decodes_heading_italic_and_ref() {
        let raw = b"\x07HEADING\x07 \r\nWe call it \x06the book\x06, see \x0356BF\x03.";
        let text = decode_markup(raw, |n| {
            if n == 0x56BF {
                Some("John 5:39".into())
            } else {
                None
            }
        });
        assert!(text.starts_with("HEADING"));
        assert!(text.contains("the book"));
        assert!(text.contains("John 5:39"));
        assert!(!text.contains('\u{3}'));
    }

    #[test]
    fn parses_label_then_body() {
        let index = [rec(1, 1, 1), rec(1, 1, 3)];
        let mut ct4 = Vec::new();
        ct4.extend_from_slice(b"1\x00Intro to Genesis.");
        let second = ct4.len() as u32;
        ct4.extend_from_slice(b"3\x00Let there be light.");
        let mut ct7 = Vec::new();
        ct7.extend_from_slice(&0u32.to_le_bytes());
        ct7.extend_from_slice(&2u32.to_le_bytes());
        ct7.extend_from_slice(&second.to_le_bytes());
        ct7.extend_from_slice(&(second + 2).to_le_bytes());
        let entries = parse_entries(&index, &ct7, &ct4, |_| None).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].verse, 1);
        assert!(entries[0].text.contains("Intro to Genesis"));
        assert_eq!(entries[1].verse, 3);
        assert!(entries[1].text.contains("Let there be light"));
    }

    #[test]
    fn compact_same_chapter_range() {
        assert_eq!(compact_range("John 1:1", "John 1:3"), "John 1:1–3");
        assert_eq!(compact_range("John 1:1", "John 2:1"), "John 1:1–John 2:1");
    }
}
