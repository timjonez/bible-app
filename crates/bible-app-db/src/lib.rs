mod occurrences;
mod search;

use rusqlite::{Connection, OptionalExtension};
use std::path::Path;
use thiserror::Error;

pub use occurrences::{strongs_occurrence_count, strongs_occurrences, Occurrence};
pub use search::{
    ensure_entries_fts, ensure_resources_fts, ensure_verses_fts, match_query, rebuild_entries_fts,
    rebuild_resources_fts, rebuild_verses_fts, search_library, search_verses, LibraryHit,
    LibraryKind, SearchHit, SearchScope, DEFAULT_LIMIT,
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
    ensure_resources_fts(&conn)?;
    ensure_entries_fts(&conn)?;
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

        DROP TABLE IF EXISTS import_log;

        CREATE VIRTUAL TABLE IF NOT EXISTS verses_fts USING fts5(
            text,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS resources_fts USING fts5(
            text,
            module UNINDEXED,
            book UNINDEXED,
            chapter UNINDEXED,
            verse UNINDEXED,
            tokenize = 'unicode61'
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS entries_fts USING fts5(
            headword,
            text,
            module UNINDEXED,
            i UNINDEXED,
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

pub const STRONGS_MODULE: &str = "Strongs";

pub fn search_strongs(conn: &Connection, query: &str, limit: i32) -> Result<Vec<DictHit>, DbError> {
    let pattern = like_prefix(query);
    let mut stmt = conn.prepare(
        "SELECT num, lang, lemma FROM strongs
         WHERE (lang || num) LIKE ?1 ESCAPE '\\'
            OR lemma LIKE ?1 ESCAPE '\\' COLLATE NOCASE
         ORDER BY lang, num, lemma
         LIMIT ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let num: i32 = row.get(0)?;
        let lang: String = row.get(1)?;
        Ok(DictHit {
            i: strongs_row_i(&lang, num),
            headword: format!("{lang}{num}"),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_strongs_entry(conn: &Connection, i: i32) -> Result<Option<(String, String)>, DbError> {
    let Some((num, lang)) = strongs_row_parts(i) else {
        return Ok(None);
    };
    let Some(def) = lookup_strongs(conn, &format!("{lang}{num}"))? else {
        return Ok(None);
    };
    let mut head = format!("{}{}", def.lang, def.num);
    if !def.lemma.is_empty() {
        head = if def.pronunciation.is_empty() {
            format!("{head}  {}", def.lemma)
        } else {
            format!("{head}  {}  ({})", def.lemma, def.pronunciation)
        };
    }
    Ok(Some((head, def.definition)))
}

fn strongs_row_i(lang: &str, num: i32) -> i32 {
    if lang.eq_ignore_ascii_case("G") {
        num + 10_000
    } else {
        num
    }
}

fn strongs_row_parts(i: i32) -> Option<(i32, String)> {
    if i >= 10_000 {
        Some((i - 10_000, "G".into()))
    } else if i > 0 {
        Some((i, "H".into()))
    } else {
        None
    }
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
    pub kind: String,
}

pub fn dictionary_modules(conn: &Connection) -> Result<Vec<DictModule>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT id, title, kind FROM modules
         WHERE kind IN ('dictionary', 'topic')
         ORDER BY CASE kind WHEN 'dictionary' THEN 0 ELSE 1 END,
                  CASE id
                    WHEN 'Webster' THEN 0
                    WHEN 'Easton' THEN 1
                    WHEN 'Nave' THEN 0
                    ELSE 2
                  END,
                  title",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(DictModule {
            id: row.get(0)?,
            title: row.get(1)?,
            kind: row.get(2)?,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictEntry {
    pub module: String,
    pub title: String,
    pub headword: String,
    pub text: String,
}

/// Keys to try for a clicked KJV token: exact, then possessive, then a trailing apostrophe.
pub fn dict_lookup_keys(word: &str) -> Vec<String> {
    let w = word.trim();
    if w.is_empty() {
        return Vec::new();
    }
    let mut keys = vec![w.to_string()];
    let lower = w.to_ascii_lowercase();
    if let Some(base) = lower
        .strip_suffix("'s")
        .or_else(|| lower.strip_suffix("’s"))
    {
        if !base.is_empty() && !keys.iter().any(|k| k.eq_ignore_ascii_case(base)) {
            keys.push(base.to_string());
        }
    }
    keys
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClickedDict {
    pub bible: Vec<DictEntry>,
    pub english: Option<DictEntry>,
}

impl ClickedDict {
    pub fn is_empty(&self) -> bool {
        self.bible.is_empty() && self.english.is_none()
    }
}

/// Every matching Bible dictionary, plus Webster 1828.
pub fn lookup_clicked_word(conn: &Connection, word: &str) -> Result<ClickedDict, DbError> {
    let mut bible = Vec::new();
    for module in dictionary_modules(conn)? {
        if module.kind != "dictionary" || module.id == "Webster" {
            continue;
        }
        if let Some(entry) = lookup_in_module(conn, &module.id, word)? {
            bible.push(entry);
        }
    }
    Ok(ClickedDict {
        bible,
        english: lookup_in_module(conn, "Webster", word)?,
    })
}

fn lookup_in_module(
    conn: &Connection,
    module: &str,
    word: &str,
) -> Result<Option<DictEntry>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT e.module, m.title, e.headword, e.text
         FROM entries e
         JOIN modules m ON m.id = e.module
         WHERE e.module = ?1 AND e.headword = ?2 COLLATE NOCASE
         ORDER BY e.i
         LIMIT 1",
    )?;
    for key in dict_lookup_keys(word) {
        let hit = stmt
            .query_row(rusqlite::params![module, &key], row_dict_entry)
            .optional()?;
        if hit.is_some() {
            return Ok(hit);
        }
        let tick = format!("{key}'");
        let hit = stmt
            .query_row(rusqlite::params![module, tick], row_dict_entry)
            .optional()?;
        if hit.is_some() {
            return Ok(hit);
        }
    }
    Ok(None)
}

fn row_dict_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictEntry> {
    Ok(DictEntry {
        module: row.get(0)?,
        title: row.get(1)?,
        headword: row.get(2)?,
        text: row.get(3)?,
    })
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

/// `(from_verse, dest)` pairs for every TSK xref in a chapter, in display order.
pub fn chapter_xrefs(conn: &Connection, book: u8, chapter: u8) -> Result<Vec<(u8, Xref)>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT from_verse, to_book, to_chapter, to_verse
         FROM xrefs
         WHERE from_book = ?1 AND from_chapter = ?2
         ORDER BY from_verse, to_book, to_chapter, to_verse",
    )?;
    let rows = stmt.query_map(rusqlite::params![book, chapter], |row| {
        Ok((
            row.get(0)?,
            Xref {
                book: row.get(1)?,
                chapter: row.get(2)?,
                verse: row.get(3)?,
            },
        ))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Verse numbers in this chapter that have a resource row for `module`.
pub fn chapter_resource_verses(
    conn: &Connection,
    module: &str,
    book: u8,
    chapter: u8,
) -> Result<Vec<u8>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT verse FROM resources
         WHERE module = ?1 AND book = ?2 AND chapter = ?3
         ORDER BY verse",
    )?;
    let rows = stmt.query_map(rusqlite::params![module, book, chapter], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Resource rows for `module` in this chapter, keyed by the row's own verse.
pub fn chapter_resources(
    conn: &Connection,
    module: &str,
    book: u8,
    chapter: u8,
) -> Result<Vec<Resource>, DbError> {
    let mut stmt = conn.prepare(
        "SELECT module, book, chapter, verse, text
         FROM resources
         WHERE module = ?1 AND book = ?2 AND chapter = ?3
         ORDER BY verse",
    )?;
    let rows = stmt.query_map(rusqlite::params![module, book, chapter], |row| {
        Ok(Resource {
            module: row.get(0)?,
            book: row.get(1)?,
            chapter: row.get(2)?,
            verse: row.get(3)?,
            text: row.get(4)?,
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
        conn.execute_batch(
            r#"
            INSERT INTO xrefs (
                from_book, from_chapter, from_verse,
                to_book, to_chapter, to_verse
            ) VALUES (1, 1, 2, 2, 1, 1);
            "#,
        )
        .unwrap();
        let chapter = chapter_xrefs(&conn, 1, 1).unwrap();
        assert_eq!(chapter.len(), 2);
        assert_eq!(chapter[0].0, 1);
        assert_eq!(chapter[1].0, 2);
        assert!(chapter_xrefs(&conn, 1, 2).unwrap().is_empty());
    }

    #[test]
    fn chapter_resource_verses_lists_starts() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license)
            VALUES ('MHC', 'commentary', 'Matthew Henry', 'public-domain');
            INSERT INTO resources (module, book, chapter, verse, text) VALUES
                ('MHC', 1, 1, 1, 'on 1'),
                ('MHC', 1, 1, 2, 'on 2');
            "#,
        )
        .unwrap();
        assert_eq!(
            chapter_resource_verses(&conn, "MHC", 1, 1).unwrap(),
            vec![1, 2]
        );
        assert!(chapter_resource_verses(&conn, "MHC", 1, 2)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn chapter_resources_uses_each_rows_verse() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license) VALUES
                ('TSK', 'commentary', 'Treasury of Scripture Knowledge', 'public-domain'),
                ('MHC', 'commentary', 'Matthew Henry', 'public-domain');
            INSERT INTO resources (module, book, chapter, verse, text) VALUES
                ('TSK', 1, 1, 1, '* beginning. Proverbs 8:22'),
                ('TSK', 1, 1, 2, '* without. Job 26:7'),
                ('MHC', 1, 1, 1, 'comment covering later verses');
            "#,
        )
        .unwrap();
        let tsk = chapter_resources(&conn, "TSK", 1, 1).unwrap();
        assert_eq!(tsk.len(), 2);
        assert_eq!(tsk[0].verse, 1);
        assert_eq!(tsk[0].text, "* beginning. Proverbs 8:22");
        assert_eq!(tsk[1].verse, 2);
        assert_eq!(tsk[1].text, "* without. Job 26:7");
        assert!(chapter_resources(&conn, "TSK", 1, 2).unwrap().is_empty());
        let covering = resource_covering(&conn, "MHC", 1, 1, 2).unwrap().unwrap();
        assert_eq!(covering.verse, 1);
        let mhc = chapter_resources(&conn, "MHC", 1, 1).unwrap();
        assert_eq!(mhc.len(), 1);
        assert_eq!(mhc[0].verse, 1);
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
        let hits = search_strongs(&conn, "H7225", 10).unwrap();
        assert_eq!(hits[0].headword, "H7225");
        let (head, text) = get_strongs_entry(&conn, hits[0].i).unwrap().unwrap();
        assert!(head.contains("H7225"));
        assert!(head.contains("re'shiyth"));
        assert!(text.contains("the first"));
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
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license)
            VALUES ('Nave', 'topic', 'Nave''s Topical Bible', 'public-domain'),
                   ('Webster', 'dictionary', 'Webster''s 1828 Dictionary', 'MIT');
            "#,
        )
        .unwrap();
        let mods = dictionary_modules(&conn).unwrap();
        assert_eq!(mods[0].id, "Webster");
        assert_eq!(mods[1].id, "Easton");
        assert!(mods.iter().any(|m| m.id == "Nave"));
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
    fn real_sqlite_webster_1828_senses() {
        let Some(path) = shipped_sqlite() else {
            eprintln!("skipping: unpacked bible-app.sqlite not found");
            return;
        };
        let conn = open(&path).unwrap();
        let mods = dictionary_modules(&conn).unwrap();
        assert_eq!(mods[0].id, "Webster");
        let hits = search_entries(&conn, "Webster", "prevent", 20).unwrap();
        assert!(
            hits.iter()
                .any(|h| h.headword.eq_ignore_ascii_case("prevent")),
            "prevent missing from {hits:?}"
        );
        let hit = hits
            .iter()
            .find(|h| h.headword.eq_ignore_ascii_case("prevent"))
            .unwrap();
        let (_, text) = get_entry(&conn, "Webster", hit.i).unwrap().unwrap();
        let lower = text.to_ascii_lowercase();
        assert!(
            lower.contains("go before") || lower.contains("precede"),
            "1828 prevent sense missing: {text}"
        );
        assert!(!text.contains('<'), "HTML leaked into Webster text");
        let god = lookup_clicked_word(&conn, "God").unwrap();
        let bible_ids: Vec<&str> = god.bible.iter().map(|e| e.module.as_str()).collect();
        assert!(bible_ids.contains(&"Easton"), "{bible_ids:?}");
        assert!(bible_ids.contains(&"Smith"), "{bible_ids:?}");
        assert_eq!(
            god.english.as_ref().map(|e| e.module.as_str()),
            Some("Webster")
        );
        let latin = lookup_clicked_word(&conn, "A-posteriori").unwrap();
        assert!(latin.bible.is_empty());
        assert_eq!(
            latin.english.as_ref().map(|e| e.module.as_str()),
            Some("Webster")
        );
    }

    #[test]
    fn dict_keys_keep_exact_and_drop_possessive() {
        assert_eq!(dict_lookup_keys("God"), vec!["God".to_string()]);
        assert_eq!(
            dict_lookup_keys("God's"),
            vec!["God's".to_string(), "god".to_string()]
        );
        assert!(dict_lookup_keys("  ").is_empty());
    }

    #[test]
    fn lookup_returns_bible_and_english() {
        let conn = open_memory().unwrap();
        seed(&conn);
        conn.execute_batch(
            r#"
            INSERT INTO modules (id, kind, title, license) VALUES
                ('Webster', 'dictionary', 'Webster''s 1828 Dictionary', 'MIT'),
                ('Easton', 'dictionary', 'Easton''s Bible Dictionary', 'public-domain'),
                ('Smith', 'dictionary', 'Smith''s Bible Dictionary', 'public-domain');
            INSERT INTO entries (module, i, headword, text) VALUES
                ('Webster', 0, 'God', 'english sense'),
                ('Easton', 0, 'God', 'easton sense'),
                ('Smith', 0, 'God', 'smith sense'),
                ('Webster', 1, 'PREVENT''', 'go before');
            "#,
        )
        .unwrap();
        let god = lookup_clicked_word(&conn, "God").unwrap();
        assert_eq!(
            god.bible
                .iter()
                .map(|e| (e.module.as_str(), e.text.as_str()))
                .collect::<Vec<_>>(),
            vec![("Easton", "easton sense"), ("Smith", "smith sense")]
        );
        assert_eq!(
            god.english.as_ref().map(|e| e.text.as_str()),
            Some("english sense")
        );
        let possessive = lookup_clicked_word(&conn, "God's").unwrap();
        assert_eq!(
            possessive
                .bible
                .iter()
                .map(|e| e.module.as_str())
                .collect::<Vec<_>>(),
            vec!["Easton", "Smith"]
        );
        let prevent = lookup_clicked_word(&conn, "prevent").unwrap();
        assert!(prevent.bible.is_empty());
        assert_eq!(
            prevent.english.as_ref().map(|e| e.module.as_str()),
            Some("Webster")
        );
        assert!(prevent
            .english
            .as_ref()
            .is_some_and(|e| e.text.contains("go before")));
    }
}
