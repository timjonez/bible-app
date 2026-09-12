use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use thiserror::Error;

pub const SCHEMA_VERSION: i32 = 1;

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

        CREATE TABLE IF NOT EXISTS import_log (
            stem   TEXT PRIMARY KEY,
            action TEXT NOT NULL,
            reason TEXT NOT NULL
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
