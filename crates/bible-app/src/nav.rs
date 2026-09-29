use bible_app_db::{self, Book};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ref {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
}

/// Parse "John 3:16", "Jn 3:16", "Genesis 1", "3:16" (current book), or "1".
pub fn parse_ref(input: &str, books: &[Book], current: Ref) -> Option<Ref> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let (book_part, rest) = split_book_and_nums(input);
    let (chapter, verse) = parse_chapter_verse(rest, current)?;

    let book = if book_part.is_empty() {
        current.book
    } else {
        match_book(book_part, books)?
    };

    Some(Ref {
        book,
        chapter: chapter.max(1),
        verse: verse.max(1),
    })
}

fn split_book_and_nums(input: &str) -> (&str, &str) {
    let bytes = input.as_bytes();
    let mut i = bytes.len();
    while i > 0 {
        let c = bytes[i - 1];
        if c.is_ascii_digit() || c == b':' || c == b'.' || c.is_ascii_whitespace() {
            i -= 1;
        } else {
            break;
        }
    }
    // include leading book tokens like "1" in "1 John 3:16"
    let book = input[..i].trim();
    let nums = input[i..].trim();
    (book, nums)
}

fn parse_chapter_verse(rest: &str, current: Ref) -> Option<(u8, u8)> {
    let rest = rest.trim();
    if rest.is_empty() {
        return Some((1, 1));
    }
    let rest = rest.replace('.', ":");
    let parts: Vec<&str> = rest
        .split(':')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    match parts.as_slice() {
        [c] => Some((c.parse().ok()?, 1)),
        [c, v] => Some((c.parse().ok()?, v.parse().ok()?)),
        _ => {
            let _ = current;
            None
        }
    }
}

const ALIASES: &[(&str, &str)] = &[
    ("gn", "genesis"),
    ("jn", "john"),
    ("joh", "john"),
    ("mt", "matthew"),
    ("mk", "mark"),
    ("mr", "mark"),
    ("lk", "luke"),
    ("lu", "luke"),
    ("ac", "acts"),
    ("ro", "romans"),
    ("ps", "psalms"),
    ("rev", "revelation"),
    ("re", "revelation"),
];

fn match_book(query: &str, books: &[Book]) -> Option<u8> {
    let q = normalize(query);
    if q.is_empty() {
        return None;
    }
    let q = ALIASES
        .iter()
        .find(|(alias, _)| *alias == q)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or(q);
    if let Some(b) = books
        .iter()
        .find(|b| normalize(&b.abbrev) == q || normalize(&b.name) == q)
    {
        return Some(b.id);
    }
    let matches: Vec<_> = books
        .iter()
        .filter(|b| normalize(&b.name).starts_with(&q) || normalize(&b.abbrev).starts_with(&q))
        .collect();
    if matches.len() == 1 {
        Some(matches[0].id)
    } else {
        None
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '.')
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn format_chapter(books: &[Book], book: u8, chapter: u8) -> String {
    let name = book_name(books, book);
    format!("{name} {chapter}")
}

pub fn format_ref(books: &[Book], at: Ref) -> String {
    format!("{} {}:{}", book_name(books, at.book), at.chapter, at.verse)
}

/// One chapter of a commentary or treasury, as a single document.
pub struct StackedText {
    pub text: String,
    /// Character offset where each section heading starts, in input order.
    pub heading_at: Vec<i32>,
    pub heading_len: Vec<i32>,
}

/// Join `(heading, body)` sections with a blank line between them.
pub fn stack_sections(sections: &[(&str, &str)]) -> StackedText {
    let mut text = String::new();
    let mut heading_at = Vec::with_capacity(sections.len());
    let mut heading_len = Vec::with_capacity(sections.len());
    for (i, (head, body)) in sections.iter().enumerate() {
        if i > 0 {
            text.push_str("\n\n");
        }
        heading_at.push(text.chars().count() as i32);
        heading_len.push(head.chars().count() as i32);
        text.push_str(head);
        text.push_str("\n\n");
        text.push_str(body.trim());
    }
    StackedText {
        text,
        heading_at,
        heading_len,
    }
}

/// Offset of the section that covers `verse` (the last heading at or before it).
pub fn section_offset(verses: &[u8], offsets: &[i32], verse: u8) -> Option<i32> {
    let mut chosen = None;
    for (v, off) in verses.iter().zip(offsets) {
        if *v <= verse {
            chosen = Some(*off);
        } else {
            break;
        }
    }
    chosen.or_else(|| offsets.first().copied())
}

fn book_name(books: &[Book], book: u8) -> &str {
    books
        .iter()
        .find(|b| b.id == book)
        .map(|b| b.name.as_str())
        .unwrap_or("Book")
}

pub fn next_chapter(
    conn: &rusqlite::Connection,
    books: &[Book],
    at: Ref,
) -> Result<Ref, bible_app_db::DbError> {
    let max_ch = bible_app_db::max_chapter(conn, at.book)?;
    if at.chapter < max_ch {
        return Ok(Ref {
            book: at.book,
            chapter: at.chapter + 1,
            verse: 1,
        });
    }
    let Some(next_book) = books.iter().find(|b| b.id > at.book) else {
        return Ok(Ref {
            book: at.book,
            chapter: at.chapter,
            verse: 1,
        });
    };
    Ok(Ref {
        book: next_book.id,
        chapter: 1,
        verse: 1,
    })
}

pub fn prev_chapter(
    conn: &rusqlite::Connection,
    books: &[Book],
    at: Ref,
) -> Result<Ref, bible_app_db::DbError> {
    if at.chapter > 1 {
        return Ok(Ref {
            book: at.book,
            chapter: at.chapter - 1,
            verse: 1,
        });
    }
    let prev = books.iter().rev().find(|b| b.id < at.book);
    let Some(prev) = prev else {
        return Ok(Ref {
            book: at.book,
            chapter: 1,
            verse: 1,
        });
    };
    let ch = bible_app_db::max_chapter(conn, prev.id)?;
    Ok(Ref {
        book: prev.id,
        chapter: ch,
        verse: 1,
    })
}

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
                id: 43,
                abbrev: "Joh".into(),
                name: "John".into(),
            },
            Book {
                id: 62,
                abbrev: "1Jo".into(),
                name: "1 John".into(),
            },
        ]
    }

    fn cur() -> Ref {
        Ref {
            book: 1,
            chapter: 1,
            verse: 1,
        }
    }

    #[test]
    fn parses_full_and_abbrev() {
        let b = books();
        assert_eq!(
            parse_ref("John 3:16", &b, cur()).unwrap(),
            Ref {
                book: 43,
                chapter: 3,
                verse: 16
            }
        );
        assert_eq!(
            parse_ref("jn 3:16", &b, cur()).unwrap(),
            Ref {
                book: 43,
                chapter: 3,
                verse: 16
            }
        );
        assert_eq!(
            parse_ref("Genesis 1", &b, cur()).unwrap(),
            Ref {
                book: 1,
                chapter: 1,
                verse: 1
            }
        );
        assert_eq!(
            parse_ref("3:16", &b, cur()).unwrap(),
            Ref {
                book: 1,
                chapter: 3,
                verse: 16
            }
        );
    }

    #[test]
    fn one_john_not_john() {
        let b = books();
        assert_eq!(parse_ref("1 John 1:1", &b, cur()).unwrap().book, 62);
    }

    #[test]
    fn formats_ref() {
        let b = books();
        assert_eq!(
            format_ref(
                &b,
                Ref {
                    book: 43,
                    chapter: 3,
                    verse: 16
                }
            ),
            "John 3:16"
        );
    }

    #[test]
    fn stack_sections_records_heading_offsets() {
        let stacked = stack_sections(&[
            ("Genesis 1:1", "In the beginning"),
            ("Genesis 1:3", "And God said"),
        ]);
        assert!(stacked.text.starts_with("Genesis 1:1\n\nIn the beginning"));
        assert_eq!(stacked.heading_at.len(), 2);
        assert_eq!(stacked.heading_len[0], "Genesis 1:1".chars().count() as i32);
        let second = stacked.heading_at[1] as usize;
        let len = stacked.heading_len[1] as usize;
        let head: String = stacked.text.chars().skip(second).take(len).collect();
        assert_eq!(head, "Genesis 1:3");
    }

    #[test]
    fn section_offset_uses_the_covering_verse() {
        let verses = [1, 3, 6];
        let offsets = [0, 40, 80];
        assert_eq!(section_offset(&verses, &offsets, 1), Some(0));
        assert_eq!(section_offset(&verses, &offsets, 4), Some(40));
        assert_eq!(section_offset(&verses, &offsets, 9), Some(80));
        assert_eq!(section_offset(&[], &[], 1), None);
    }
}
