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

fn book_name(books: &[Book], book: u8) -> &str {
    books
        .iter()
        .find(|b| b.id == book)
        .map(|b| b.name.as_str())
        .unwrap_or("Book")
}

pub fn format_chapter_text(verses: &[bible_app_db::Verse]) -> String {
    layout_chapter(verses).0
}

/// Character offset in the chapter string where each verse's body begins
/// (after the `"12  "` prefix).
pub fn verse_body_offsets(verses: &[bible_app_db::Verse]) -> Vec<(u8, i32)> {
    layout_chapter(verses).1
}

fn layout_chapter(verses: &[bible_app_db::Verse]) -> (String, Vec<(u8, i32)>) {
    let mut out = String::new();
    let mut offsets = Vec::with_capacity(verses.len());
    for v in verses {
        if v.para_break && !out.is_empty() {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("{}  ", v.verse));
        offsets.push((v.verse, out.chars().count() as i32));
        out.push_str(&v.text);
    }
    (out, offsets)
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
    fn verse_body_offsets_skip_number_prefix() {
        let verses = vec![
            bible_app_db::Verse {
                book: 1,
                chapter: 1,
                verse: 1,
                text: "In the beginning".into(),
                para_break: true,
            },
            bible_app_db::Verse {
                book: 1,
                chapter: 1,
                verse: 2,
                text: "And the earth".into(),
                para_break: false,
            },
        ];
        let (text, offs) = super::layout_chapter(&verses);
        assert_eq!(text, "1  In the beginning\n2  And the earth");
        assert_eq!(offs, vec![(1, 3), (2, 23)]);
        assert_eq!(
            &text[offs[0].1 as usize..offs[0].1 as usize + 16],
            "In the beginning"
        );
    }
}
