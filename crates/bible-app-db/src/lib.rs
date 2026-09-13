mod search;

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use thiserror::Error;

pub use search::{
    ensure_verses_fts, match_query, rebuild_verses_fts, search_verses, SearchHit, DEFAULT_LIMIT,
};

pub const SCHEMA_VERSION: i32 = 3;

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
}
