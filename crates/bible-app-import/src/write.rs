use crate::dumpfmt::{self, KjvModule};
use bible_app_db;
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WriteError {
    #[error(transparent)]
    Dumpfmt(#[from] dumpfmt::Error),
    #[error(transparent)]
    Db(#[from] bible_app_db::DbError),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub fn import_kjv(conn: &mut Connection, module: &KjvModule) -> Result<usize, WriteError> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO modules (id, kind, title, license) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["KJV", "bible", module.header.title, "public-domain"],
    )?;
    tx.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('bible', 'KJV')",
        [],
    )?;

    for book in &module.books {
        tx.execute(
            "INSERT OR REPLACE INTO books (id, abbrev, name) VALUES (?1, ?2, ?3)",
            rusqlite::params![book.id, book.abbrev, book.name],
        )?;
    }

    tx.execute("DELETE FROM verses", [])?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO verses (book, chapter, verse, text, para_break)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for (i, rec) in module.index.iter().enumerate() {
            let next = module.index.get(i + 1).map(|r| r.offset);
            let raw = module.verse_raw(*rec, next)?;
            let (para, text) = dumpfmt::decode_verse_text(raw);
            stmt.execute(rusqlite::params![
                rec.book,
                rec.chapter,
                rec.verse,
                text,
                if para { 1 } else { 0 }
            ])?;
        }
    }
    bible_app_db::rebuild_verses_fts(&tx)?;
    tx.commit()?;
    Ok(module.index.len())
}

pub fn create_db(path: &Path) -> Result<Connection, WriteError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(dumpfmt::Error::from)?;
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(dumpfmt::Error::from)?;
    }
    let conn = bible_app_db::open(path)?;
    bible_app_db::init_schema(&conn)?;
    Ok(conn)
}
