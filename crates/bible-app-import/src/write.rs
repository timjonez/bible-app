use crate::dumpfmt::{self, KjvModule, ResourceModule};
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

pub fn import_mhc(
    conn: &mut Connection,
    from: &Path,
    kjv: &KjvModule,
) -> Result<usize, WriteError> {
    import_commentary(conn, from, kjv, "MHC")
}

pub fn import_tsk(
    conn: &mut Connection,
    from: &Path,
    kjv: &KjvModule,
) -> Result<(usize, usize), WriteError> {
    if !from.join("TSK.ct4").is_file() || !from.join("TSK.ct7").is_file() {
        return Ok((0, 0));
    }
    let module = dumpfmt::load_commentary(from, "TSK", &kjv.index, &kjv.books)?;
    let n = import_resource(conn, &module)?;
    let x = import_xrefs(conn, &module)?;
    Ok((n, x))
}

pub fn import_strongs(
    conn: &mut Connection,
    from: &Path,
    kjv: &KjvModule,
) -> Result<(usize, usize), WriteError> {
    let mapped = dumpfmt::load_word_map(from, kjv)?;
    let lexicon = dumpfmt::load_lexicon(from)?;
    let words = import_verse_words(conn, &mapped)?;
    let defs = import_lexicon(conn, &lexicon)?;
    Ok((words, defs))
}

fn import_verse_words(
    conn: &mut Connection,
    mapped: &[dumpfmt::MappedVerse],
) -> Result<usize, WriteError> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM verse_words", [])?;
    let mut n = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO verse_words (book, chapter, verse, i, start, end_pos, strongs)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )?;
        for v in mapped {
            for w in &v.words {
                if w.refs.is_empty() {
                    continue;
                }
                let codes: Vec<String> = w.refs.iter().map(|r| r.code()).collect();
                n += stmt.execute(rusqlite::params![
                    v.book,
                    v.chapter,
                    v.verse,
                    w.i,
                    w.start,
                    w.end,
                    codes.join(" "),
                ])?;
            }
        }
    }
    tx.commit()?;
    Ok(n)
}

fn import_lexicon(conn: &mut Connection, entries: &[dumpfmt::LexEntry]) -> Result<usize, WriteError> {
    if entries.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO modules (id, kind, title, license) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![
            "STRONGS",
            "strongs",
            "Strong's Exhaustive Concordance (1890)",
            "public-domain"
        ],
    )?;
    tx.execute("DELETE FROM strongs", [])?;
    let mut n = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT OR REPLACE INTO strongs (num, lang, lemma, pronunciation, definition)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for e in entries {
            n += stmt.execute(rusqlite::params![
                e.num,
                e.lang,
                e.lemma,
                e.pronunciation,
                e.definition,
            ])?;
        }
    }
    tx.commit()?;
    Ok(n)
}

fn import_commentary(
    conn: &mut Connection,
    from: &Path,
    kjv: &KjvModule,
    stem: &str,
) -> Result<usize, WriteError> {
    if !from.join(format!("{stem}.ct4")).is_file() || !from.join(format!("{stem}.ct7")).is_file() {
        return Ok(0);
    }
    let module = dumpfmt::load_commentary(from, stem, &kjv.index, &kjv.books)?;
    import_resource(conn, &module)
}

fn import_resource(conn: &mut Connection, module: &ResourceModule) -> Result<usize, WriteError> {
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT OR REPLACE INTO modules (id, kind, title, license) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![module.id, "commentary", module.title, "public-domain"],
    )?;
    tx.execute(
        "DELETE FROM resources WHERE module = ?1",
        rusqlite::params![module.id],
    )?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO resources (module, book, chapter, verse, text)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for e in &module.entries {
            stmt.execute(rusqlite::params![
                module.id, e.book, e.chapter, e.verse, e.text
            ])?;
        }
    }
    tx.commit()?;
    Ok(module.entries.len())
}

fn import_xrefs(conn: &mut Connection, module: &ResourceModule) -> Result<usize, WriteError> {
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM xrefs", [])?;
    let mut n = 0usize;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO xrefs (
                from_book, from_chapter, from_verse,
                to_book, to_chapter, to_verse
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for e in &module.entries {
            for d in &e.xrefs {
                n += stmt.execute(rusqlite::params![
                    e.book, e.chapter, e.verse, d.book, d.chapter, d.verse
                ])?;
            }
        }
    }
    tx.commit()?;
    Ok(n)
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
