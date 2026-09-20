use rusqlite::Connection;

use crate::{parse_strongs_code, DbError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Occurrence {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub snippet: String,
}

/// Unique KJV verses whose `verse_words` include `code` as a whole token.
pub fn strongs_occurrences(
    conn: &Connection,
    code: &str,
    limit: usize,
) -> Result<Vec<Occurrence>, DbError> {
    let Some(code) = canonical_code(code) else {
        return Ok(Vec::new());
    };
    let pattern = like_token_pattern(&code);
    let limit = limit.max(1) as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT DISTINCT v.book, v.chapter, v.verse, v.text
        FROM verse_words w
        JOIN verses v
          ON v.book = w.book AND v.chapter = w.chapter AND v.verse = w.verse
        WHERE (' ' || w.strongs || ' ') LIKE ?1 ESCAPE '\'
        ORDER BY v.book, v.chapter, v.verse
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        Ok(Occurrence {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            snippet: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Unique KJV verses tagged with `code` as a whole token.
pub fn strongs_occurrence_count(conn: &Connection, code: &str) -> Result<usize, DbError> {
    let Some(code) = canonical_code(code) else {
        return Ok(0);
    };
    let pattern = like_token_pattern(&code);
    let n: i64 = conn.query_row(
        r#"
        SELECT COUNT(*) FROM (
            SELECT DISTINCT book, chapter, verse
            FROM verse_words
            WHERE (' ' || strongs || ' ') LIKE ?1 ESCAPE '\'
        )
        "#,
        rusqlite::params![pattern],
        |row| row.get(0),
    )?;
    Ok(n as usize)
}

fn canonical_code(code: &str) -> Option<String> {
    let (num, lang) = parse_strongs_code(code)?;
    Some(format!("{lang}{num}"))
}

fn like_token_pattern(code: &str) -> String {
    let mut out = String::from("% ");
    for c in code.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push_str(" %");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open, open_memory, DEFAULT_LIMIT};
    use rusqlite::OptionalExtension;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES
                (1, 'Ge', 'Genesis'),
                (2, 'Ex', 'Exodus');
            INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 1, 1, 'In the beginning God created the heaven and the earth.', 1),
                (1, 1, 2, 'And the earth was without form.', 0),
                (1, 1, 3, 'And God said, Let there be light.', 0),
                (2, 1, 1, 'Now these are the names.', 1);
            INSERT INTO verse_words (book, chapter, verse, i, start, end_pos, strongs) VALUES
                (1, 1, 1, 0, 0, 16, 'H7225'),
                (1, 1, 1, 1, 17, 20, 'H430'),
                (1, 1, 2, 0, 0, 3, 'H43'),
                (1, 1, 3, 0, 4, 7, 'H7225 H1254'),
                (2, 1, 1, 0, 0, 3, 'H8034');
            "#,
        )
        .unwrap();
    }

    fn refs(hits: &[Occurrence]) -> Vec<(u8, u8, u8)> {
        hits.iter().map(|h| (h.book, h.chapter, h.verse)).collect()
    }

    fn shipped_sqlite() -> Option<std::path::PathBuf> {
        std::env::var_os("BIBLE_APP_DB")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| {
                let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../data/bible-app.sqlite");
                p.is_file().then_some(p)
            })
    }

    #[test]
    fn h7225_returns_genesis_1_1_only_from_single_code_row() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis');
            INSERT INTO verses (book, chapter, verse, text, para_break)
                VALUES (1, 1, 1, 'In the beginning', 1);
            INSERT INTO verse_words (book, chapter, verse, i, start, end_pos, strongs)
                VALUES (1, 1, 1, 0, 0, 16, 'H7225');
            "#,
        )
        .unwrap();
        let hits = strongs_occurrences(&conn, "H7225", 20).unwrap();
        assert_eq!(refs(&hits), vec![(1, 1, 1)]);
        assert_eq!(hits[0].snippet, "In the beginning");
        assert_eq!(strongs_occurrence_count(&conn, "H7225").unwrap(), 1);
        assert_eq!(strongs_occurrence_count(&conn, "h7225").unwrap(), 1);
        assert!(strongs_occurrences(&conn, "nope", 20).unwrap().is_empty());
        assert_eq!(strongs_occurrence_count(&conn, "").unwrap(), 0);
    }

    #[test]
    fn token_match_does_not_treat_h43_as_h430() {
        let conn = open_memory().unwrap();
        seed(&conn);
        let h43 = strongs_occurrences(&conn, "H43", 20).unwrap();
        assert_eq!(refs(&h43), vec![(1, 1, 2)]);
        let h430 = strongs_occurrences(&conn, "H430", 20).unwrap();
        assert_eq!(refs(&h430), vec![(1, 1, 1)]);
        assert!(!refs(&h43).contains(&(1, 1, 1)));
        assert_eq!(strongs_occurrence_count(&conn, "H43").unwrap(), 1);
        assert_eq!(strongs_occurrence_count(&conn, "H430").unwrap(), 1);
    }

    #[test]
    fn whitespace_separated_codes_match_each_token() {
        let conn = open_memory().unwrap();
        seed(&conn);
        let a = strongs_occurrences(&conn, "H7225", 20).unwrap();
        let b = strongs_occurrences(&conn, "H1254", 20).unwrap();
        assert!(refs(&a).contains(&(1, 1, 1)));
        assert!(refs(&a).contains(&(1, 1, 3)));
        assert_eq!(refs(&b), vec![(1, 1, 3)]);
        assert_eq!(strongs_occurrence_count(&conn, "H7225").unwrap(), 2);
        assert_eq!(strongs_occurrence_count(&conn, "H1254").unwrap(), 1);
    }

    #[test]
    fn results_are_unique_verses_in_book_chapter_verse_order() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO verse_words (book, chapter, verse, i, start, end_pos, strongs)
                VALUES (1, 1, 1, 2, 21, 28, 'H430');
            "#,
        )
        .unwrap();
        let hits = strongs_occurrences(&conn, "H430", 20).unwrap();
        assert_eq!(refs(&hits), vec![(1, 1, 1)]);
        assert_eq!(strongs_occurrence_count(&conn, "H430").unwrap(), 1);
    }

    #[test]
    fn real_sqlite_g26_or_h430_is_ordered_and_limited() {
        let Some(path) = shipped_sqlite() else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = open(&path).unwrap();
        let g26 = strongs_occurrences(&conn, "G26", DEFAULT_LIMIT).unwrap();
        let g26_n = strongs_occurrence_count(&conn, "G26").unwrap();
        assert!(
            g26_n > 50 && g26.len() == g26_n.min(DEFAULT_LIMIT),
            "G26 count={g26_n} hits={}",
            g26.len()
        );
        let keys = refs(&g26);
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(
            keys, sorted,
            "G26 hits should be in book/chapter/verse order"
        );
        assert!(g26.iter().any(|h| !h.snippet.is_empty()));

        let h430 = strongs_occurrences(&conn, "H430", DEFAULT_LIMIT).unwrap();
        let h430_n = strongs_occurrence_count(&conn, "H430").unwrap();
        assert!(
            h430_n > DEFAULT_LIMIT && h430.len() == DEFAULT_LIMIT,
            "H430 count={h430_n} hits={}",
            h430.len()
        );
        let keys = refs(&h430);
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
        assert_eq!(h430[0].book, 1);
        assert_eq!((h430[0].chapter, h430[0].verse), (1, 1));
    }

    #[test]
    fn real_sqlite_tr_comma_verse_still_has_strongs() {
        let Some(path) = shipped_sqlite() else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = open(&path).unwrap();
        let code: Option<String> = conn
            .query_row(
                "SELECT strongs FROM verse_words
                 WHERE book = 62 AND chapter = 5 AND verse = 7
                 ORDER BY i LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .unwrap();
        let Some(raw) = code.filter(|s| !s.is_empty()) else {
            eprintln!("skipping: no verse_words on 1 John 5:7");
            return;
        };
        let token = raw.split_whitespace().next().unwrap_or(&raw);
        let hits = strongs_occurrences(&conn, token, 10_000).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 62 && h.chapter == 5 && h.verse == 7),
            "{token} missing 1 John 5:7 from {} hits",
            hits.len()
        );
    }
}
