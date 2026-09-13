mod search;

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use thiserror::Error;

pub use search::{
    ensure_verses_fts, match_query, rebuild_verses_fts, search_verses, SearchHit, DEFAULT_LIMIT,
};

pub const SCHEMA_VERSION: i32 = 6;

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("no such verse {book}:{chapter}:{verse}")]
    MissingVerse { book: u8, chapter: u8, verse: u8 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub id: u8,
    pub abbrev: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verse {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
    pub para_break: bool,
}

pub fn open(path: &Path) -> Result<Connection, DbError> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    init_schema(&conn)?;
    ensure_verses_fts(&conn)?;
    Ok(conn)
}

pub fn open_memory() -> Result<Connection, DbError> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> Result<(), DbError> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS modules (
            id      TEXT PRIMARY KEY,
            kind    TEXT NOT NULL,
            title   TEXT NOT NULL,
            license TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS books (
            id     INTEGER PRIMARY KEY,
            abbrev TEXT NOT NULL,
            name   TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS verses (
            book       INTEGER NOT NULL,
            chapter    INTEGER NOT NULL,
            verse      INTEGER NOT NULL,
            text       TEXT NOT NULL,
            para_break INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (book, chapter, verse),
            FOREIGN KEY (book) REFERENCES books(id)
        );

        CREATE TABLE IF NOT EXISTS resources (
            module  TEXT NOT NULL,
            book    INTEGER NOT NULL,
            chapter INTEGER NOT NULL,
            verse   INTEGER NOT NULL,
            text    TEXT NOT NULL,
            PRIMARY KEY (module, book, chapter, verse),
            FOREIGN KEY (module) REFERENCES modules(id),
            FOREIGN KEY (book) REFERENCES books(id)
        );

        CREATE TABLE IF NOT EXISTS xrefs (
            from_book    INTEGER NOT NULL,
            from_chapter INTEGER NOT NULL,
            from_verse   INTEGER NOT NULL,
            to_book      INTEGER NOT NULL,
            to_chapter   INTEGER NOT NULL,
            to_verse     INTEGER NOT NULL,
            PRIMARY KEY (
                from_book, from_chapter, from_verse,
                to_book, to_chapter, to_verse
            )
        );

        CREATE TABLE IF NOT EXISTS verse_words (
            book    INTEGER NOT NULL,
            chapter INTEGER NOT NULL,
            verse   INTEGER NOT NULL,
            i       INTEGER NOT NULL,
            start   INTEGER NOT NULL,
            end_pos INTEGER NOT NULL,
            strongs TEXT NOT NULL,
            PRIMARY KEY (book, chapter, verse, i),
            FOREIGN KEY (book) REFERENCES books(id)
        );

        CREATE TABLE IF NOT EXISTS strongs (
            num            INTEGER NOT NULL,
            lang           TEXT NOT NULL,
            lemma          TEXT NOT NULL,
            pronunciation  TEXT NOT NULL,
            definition     TEXT NOT NULL,
            PRIMARY KEY (num, lang)
        );

        CREATE TABLE IF NOT EXISTS entries (
            module   TEXT NOT NULL,
            i        INTEGER NOT NULL,
            headword TEXT NOT NULL,
            text     TEXT NOT NULL,
            PRIMARY KEY (module, i),
            FOREIGN KEY (module) REFERENCES modules(id)
        );
        CREATE INDEX IF NOT EXISTS entries_headword ON entries(module, headword COLLATE NOCASE);

        CREATE TABLE IF NOT EXISTS import_log (
            stem   TEXT PRIMARY KEY,
            action TEXT NOT NULL,
            reason TEXT NOT NULL
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
            text,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );
        "#,
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        [SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

pub fn get_verse(conn: &Connection, book: u8, chapter: u8, verse: u8) -> Result<Verse, DbError> {
    let row = conn
        .query_row(
            "SELECT book, chapter, verse, text, para_break
             FROM verses WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
            [book, chapter, verse],
            |row| {
                Ok(Verse {
                    book: row.get(0)?,
                    chapter: row.get(1)?,
                    verse: row.get(2)?,
                    text: row.get(3)?,
                    para_break: row.get::<_, i32>(4)? != 0,
                })
            },
        )
        .optional()?;
    row.ok_or(DbError::MissingVerse {
        book,
        chapter,
        verse,
    })
}

pub fn books(conn: &Connection) -> Result<Vec<Book>, DbError> {
    let mut stmt = conn.prepare("SELECT id, abbrev, name FROM books ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok(Book {
            id: row.get(0)?,
            abbrev: row.get(1)?,
            name: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn verse_count(conn: &Connection) -> Result<i64, DbError> {
    Ok(conn.query_row("SELECT COUNT(*) FROM verses", [], |row| row.get(0))?)
}

pub fn chapter(conn: &Connection, book: u8, chapter: u8) -> Result<Vec<Verse>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT book, chapter, verse, text, para_break
         FROM verses
         WHERE book = ?1 AND chapter = ?2
         ORDER BY verse",
    )?;
    let rows = stmt.query_map([book, chapter], |row| {
        Ok(Verse {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            text: row.get(3)?,
            para_break: row.get::<_, i32>(4)? != 0,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn max_chapter(conn: &Connection, book: u8) -> Result<u8, DbError> {
    let n: Option<u8> = conn.query_row(
        "SELECT MAX(chapter) FROM verses WHERE book = ?1",
        [book],
        |row| row.get(0),
    )?;
    n.ok_or(DbError::MissingVerse {
        book,
        chapter: 1,
        verse: 1,
    })
}

pub fn max_verse(conn: &Connection, book: u8, chapter: u8) -> Result<u8, DbError> {
    let n: Option<u8> = conn.query_row(
        "SELECT MAX(verse) FROM verses WHERE book = ?1 AND chapter = ?2",
        [book, chapter],
        |row| row.get(0),
    )?;
    n.ok_or(DbError::MissingVerse {
        book,
        chapter,
        verse: 1,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub module: String,
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
}

/// Latest resource on or before this verse in the same book and chapter.
pub fn resource_covering(
    conn: &Connection,
    module: &str,
    book: u8,
    chapter: u8,
    verse: u8,
) -> Result<Option<Resource>, DbError> {
    let row = conn
        .query_row(
            "SELECT module, book, chapter, verse, text
             FROM resources
             WHERE module = ?1 AND book = ?2 AND chapter = ?3 AND verse <= ?4
             ORDER BY verse DESC
             LIMIT 1",
            rusqlite::params![module, book, chapter, verse],
            |row| {
                Ok(Resource {
                    module: row.get(0)?,
                    book: row.get(1)?,
                    chapter: row.get(2)?,
                    verse: row.get(3)?,
                    text: row.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Xref {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseWord {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub i: i32,
    pub start: i32,
    pub end: i32,
    pub strongs: String,
}

pub fn chapter_words(conn: &Connection, book: u8, chapter: u8) -> Result<Vec<VerseWord>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT book, chapter, verse, i, start, end_pos, strongs
         FROM verse_words
         WHERE book = ?1 AND chapter = ?2
         ORDER BY verse, i",
    )?;
    let rows = stmt.query_map(rusqlite::params![book, chapter], |row| {
        Ok(VerseWord {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            i: row.get(3)?,
            start: row.get(4)?,
            end: row.get(5)?,
            strongs: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StrongDef {
    pub num: i32,
    pub lang: String,
    pub lemma: String,
    pub pronunciation: String,
    pub definition: String,
}

pub fn parse_strongs_code(code: &str) -> Option<(i32, String)> {
    let code = code.trim();
    let (lang, rest) = code.split_at(code.chars().next()?.len_utf8());
    let lang = lang.to_ascii_uppercase();
    if lang != "G" && lang != "H" {
        return None;
    }
    let num: i32 = rest.parse().ok()?;
    if num <= 0 {
        return None;
    }
    Some((num, lang))
}

pub fn lookup_strongs(conn: &Connection, code: &str) -> Result<Option<StrongDef>, DbError> {
    let Some((num, lang)) = parse_strongs_code(code) else {
        return Ok(None);
    };
    let row = conn
        .query_row(
            "SELECT num, lang, lemma, pronunciation, definition
             FROM strongs WHERE num = ?1 AND lang = ?2",
            rusqlite::params![num, lang],
            |row| {
                Ok(StrongDef {
                    num: row.get(0)?,
                    lang: row.get(1)?,
                    lemma: row.get(2)?,
                    pronunciation: row.get(3)?,
                    definition: row.get(4)?,
                })
            },
        )
        .optional()?;
    Ok(row.or_else(|| {
        Some(StrongDef {
            num,
            lang,
            lemma: String::new(),
            pronunciation: String::new(),
            definition: String::new(),
        })
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictModule {
    pub id: String,
    pub title: String,
}

pub fn dictionary_modules(conn: &Connection) -> Result<Vec<DictModule>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, title FROM modules
         WHERE kind = 'dictionary'
         ORDER BY CASE id WHEN 'Easton' THEN 0 ELSE 1 END, title",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DictModule {
            id: row.get(0)?,
            title: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictHit {
    pub i: i32,
    pub headword: String,
}

pub const ENTRY_LIMIT: i32 = 200;

pub fn search_entries(
    conn: &Connection,
    module: &str,
    query: &str,
    limit: i32,
) -> Result<Vec<DictHit>, DbError> {
    let pattern = like_prefix(query);
    let mut stmt = conn.prepare(
        "SELECT i, headword FROM entries
         WHERE module = ?1 AND headword LIKE ?2 ESCAPE '\\'
         ORDER BY headword COLLATE NOCASE, i
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(rusqlite::params![module, pattern, limit], |row| {
        Ok(DictHit {
            i: row.get(0)?,
            headword: row.get(1)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn like_prefix(query: &str) -> String {
    let q = query.trim();
    if q.is_empty() {
        return "%".into();
    }
    let mut out = String::new();
    for c in q.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('%');
    out
}

pub fn get_entry(
    conn: &Connection,
    module: &str,
    i: i32,
) -> Result<Option<(String, String)>, DbError> {
    let row = conn
        .query_row(
            "SELECT headword, text FROM entries WHERE module = ?1 AND i = ?2",
            rusqlite::params![module, i],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    Ok(row)
}

pub fn xrefs_from(
    conn: &Connection,
    book: u8,
    chapter: u8,
    verse: u8,
) -> Result<Vec<Xref>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT to_book, to_chapter, to_verse
         FROM xrefs
         WHERE from_book = ?1 AND from_chapter = ?2 AND from_verse = ?3
         ORDER BY to_book, to_chapter, to_verse",
    )?;
    let rows = stmt.query_map(rusqlite::params![book, chapter, verse], |row| {
        Ok(Xref {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn book_by_id(conn: &Connection, id: u8) -> Result<Book, DbError> {
    let row = conn
        .query_row(
            "SELECT id, abbrev, name FROM books WHERE id = ?1",
            [id],
            |row| {
                Ok(Book {
                    id: row.get(0)?,
                    abbrev: row.get(1)?,
                    name: row.get(2)?,
                })
            },
        )
        .optional()?;
    row.ok_or(DbError::MissingVerse {
        book: id,
        chapter: 1,
        verse: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(conn: &Connection) {
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis'), (2, 'Ex', 'Exodus');
            INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 1, 1, 'In the beginning', 1),
                (1, 1, 2, 'And the earth', 0),
                (1, 2, 1, 'Thus the heavens', 1),
                (2, 1, 1, 'Now these are the names', 1);
            "#,
        )
        .unwrap();
    }

    #[test]
    fn chapter_lists_verses_in_order() {
        let conn = open_memory().unwrap();
        seed(&conn);
        let vs = chapter(&conn, 1, 1).unwrap();
        assert_eq!(vs.len(), 2);
        assert_eq!(vs[0].text, "In the beginning");
        assert!(vs[0].para_break);
        assert_eq!(max_chapter(&conn, 1).unwrap(), 2);
        assert_eq!(max_verse(&conn, 1, 1).unwrap(), 2);
    }

    #[test]
    fn resource_covers_later_verse_in_chapter() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license)
            VALUES ('MHC', 'commentary', 'Matthew Henry', 'public-domain');
            INSERT INTO resources (module, book, chapter, verse, text)
            VALUES ('MHC', 1, 1, 1, 'comment on verse 1');
            "#,
        )
        .unwrap();
        let at = resource_covering(&conn, "MHC", 1, 1, 2).unwrap().unwrap();
        assert_eq!(at.verse, 1);
        assert_eq!(at.text, "comment on verse 1");
        assert!(resource_covering(&conn, "MHC", 1, 2, 1).unwrap().is_none());
    }

    #[test]
    fn xrefs_from_lists_destinations() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO xrefs (
                from_book, from_chapter, from_verse,
                to_book, to_chapter, to_verse
            ) VALUES (1, 1, 1, 2, 1, 1);
            "#,
        )
        .unwrap();
        let xs = xrefs_from(&conn, 1, 1, 1).unwrap();
        assert_eq!(xs.len(), 1);
        assert_eq!((xs[0].book, xs[0].chapter, xs[0].verse), (2, 1, 1));
        assert!(xrefs_from(&conn, 1, 1, 2).unwrap().is_empty());
    }

    #[test]
    fn chapter_words_and_strongs_lookup() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO verse_words (book, chapter, verse, i, start, end_pos, strongs)
            VALUES (1, 1, 1, 0, 0, 3, 'H7225');
            INSERT INTO strongs (num, lang, lemma, pronunciation, definition)
            VALUES (7225, 'H', 're''shiyth', 'ray-sheeth''', 'the first');
            "#,
        )
        .unwrap();
        let ws = chapter_words(&conn, 1, 1).unwrap();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].strongs, "H7225");
        let def = lookup_strongs(&conn, "H7225").unwrap().unwrap();
        assert_eq!(def.lemma, "re'shiyth");
        assert_eq!(parse_strongs_code("G2316"), Some((2316, "G".into())));
    }

    #[test]
    fn search_entries_prefix() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license)
            VALUES ('Easton', 'dictionary', 'Easton''s Bible Dictionary', 'public-domain');
            INSERT INTO entries (module, i, headword, text) VALUES
                ('Easton', 0, 'Aaron', 'the eldest son of Amram'),
                ('Easton', 1, 'Abaddon', 'destruction'),
                ('Easton', 2, 'Zuzims', 'restless');
            "#,
        )
        .unwrap();
        let hits = search_entries(&conn, "Easton", "Aa", 20).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].headword, "Aaron");
        let (head, text) = get_entry(&conn, "Easton", 0).unwrap().unwrap();
        assert_eq!(head, "Aaron");
        assert!(text.contains("Amram"));
        assert_eq!(dictionary_modules(&conn).unwrap()[0].id, "Easton");
    }
}
