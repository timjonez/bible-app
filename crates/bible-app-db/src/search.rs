use rusqlite::Connection;

use crate::DbError;

pub const DEFAULT_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
    pub snippet: String,
}

/// Turn typed text into an FTS5 MATCH query.
///
/// Multi-word input becomes a phrase so "only begotten" matches that pair,
/// not any verse that happens to contain both words.
pub fn match_query(input: &str) -> Option<String> {
    let tokens: Vec<&str> = input
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '\''))
        .filter(|t| !t.is_empty())
        .filter(|t| {
            !matches!(
                t.to_ascii_uppercase().as_str(),
                "AND" | "OR" | "NOT" | "NEAR"
            )
        })
        .collect();
    if tokens.is_empty() {
        return None;
    }
    if tokens.len() == 1 {
        Some(tokens[0].to_string())
    } else {
        Some(format!("\"{}\"", tokens.join(" ")))
    }
}

pub fn rebuild_verses_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
            text,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        DELETE FROM verses_fts;
        INSERT INTO verses_fts (text, book, chapter, verse)
        SELECT text, book, chapter, verse FROM verses;
        "#,
    )?;
    Ok(())
}

pub fn ensure_verses_fts(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
            text,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        "#,
    )?;
    let verses: i64 = conn.query_row("SELECT COUNT(*) FROM verses", [], |r| r.get(0))?;
    let indexed: i64 = conn.query_row("SELECT COUNT(*) FROM verses_fts", [], |r| r.get(0))?;
    if verses != indexed {
        rebuild_verses_fts(conn)?;
    }
    Ok(())
}

pub fn search_verses(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, DbError> {
    let Some(match_q) = match_query(query) else {
        return Ok(Vec::new());
    };
    let limit = limit.max(1) as i64;
    let mut stmt = conn.prepare(
        r#"
        SELECT book, chapter, verse, text,
               snippet(verses_fts, 0, '', '', '…', 16)
        FROM verses_fts
        WHERE verses_fts MATCH ?1
        ORDER BY book, chapter, verse
        LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(rusqlite::params![match_q, limit], |row| {
        let text: String = row.get(3)?;
        let snippet: String = row.get(4)?;
        let snippet = if snippet.is_empty() {
            text.clone()
        } else {
            snippet
        };
        Ok(SearchHit {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            text,
            snippet,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::open_memory;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES
                (1, 'Ge', 'Genesis'),
                (43, 'Joh', 'John'),
                (58, 'Heb', 'Hebrews');
            INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 1, 1, 'In the beginning God created the heaven and the earth.', 1),
                (43, 3, 16, 'For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.', 0),
                (43, 1, 14, 'And the Word was made flesh, and dwelt among us, (and we beheld his glory, the glory as of the only begotten of the Father,) full of grace and truth.', 0),
                (58, 11, 17, 'By faith Abraham, when he was tried, offered up Isaac: and he that had received the promises offered up his only begotten son,', 0);
            "#,
        )
        .unwrap();
        rebuild_verses_fts(conn).unwrap();
    }

    #[test]
    fn phrase_only_begotten_includes_john_3_16() {
        let conn = open_memory().unwrap();
        seed(&conn);
        let hits = search_verses(&conn, "only begotten", 20).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 43 && h.chapter == 3 && h.verse == 16),
            "hits: {hits:?}"
        );
        assert!(hits
            .iter()
            .all(|h| h.text.to_lowercase().contains("only begotten")));
        let keys: Vec<_> = hits.iter().map(|h| (h.book, h.chapter, h.verse)).collect();
        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted, "hits should be in book/chapter/verse order");
    }

    #[test]
    fn empty_or_operator_only_query_is_empty() {
        let conn = open_memory().unwrap();
        seed(&conn);
        assert!(search_verses(&conn, "   ", 10).unwrap().is_empty());
        assert!(search_verses(&conn, "AND OR", 10).unwrap().is_empty());
        assert_eq!(
            match_query("only begotten").as_deref(),
            Some("\"only begotten\"")
        );
        assert_eq!(match_query("begotten").as_deref(), Some("begotten"));
    }

    #[test]
    fn real_sqlite_only_begotten_includes_john_3_16() {
        let path = std::env::var_os("BIBLE_APP_DB")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| {
                let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../../data/bible-app.sqlite");
                p.is_file().then_some(p)
            });
        let Some(path) = path else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = crate::open(&path).unwrap();
        let hits = search_verses(&conn, "only begotten", 50).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.book == 43 && h.chapter == 3 && h.verse == 16),
            "John 3:16 missing from {hits:?}"
        );
    }

    #[test]
    fn ensure_backfills_when_index_missing() {
        let conn = open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (43, 'Joh', 'John');
            INSERT INTO verses (book, chapter, verse, text, para_break)
            VALUES (43, 3, 16, 'his only begotten Son', 0);
            DELETE FROM verses_fts;
            "#,
        )
        .unwrap();
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM verses_fts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        ensure_verses_fts(&conn).unwrap();
        let hits = search_verses(&conn, "only begotten", 5).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!((hits[0].book, hits[0].chapter, hits[0].verse), (43, 3, 16));
    }
}
