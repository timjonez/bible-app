use crate::layout;
use crate::nav::{self, Ref};
use bible_app_db::Book;
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const APP_DIR: &str = "bible-app";
const USER_DB_NAME: &str = "user.sqlite";

pub const USER_SCHEMA_VERSION: i32 = 1;
pub const DEFAULT_HIGHLIGHT: &str = "gold";
pub const HIGHLIGHT_COLORS: &[&str] = &["gold", "green", "blue", "rose"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerseMarks {
    pub bookmark: bool,
    pub note: bool,
    pub highlight: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookmark {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub created_at: i64,
    pub label: String,
}

impl Bookmark {
    pub fn at(&self) -> Ref {
        Ref {
            book: self.book,
            chapter: self.chapter,
            verse: self.verse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub text: String,
    pub updated_at: i64,
}

impl Note {
    pub fn at(&self) -> Ref {
        Ref {
            book: self.book,
            chapter: self.chapter,
            verse: self.verse,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEntry {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub bookmark: bool,
    pub highlight: Option<String>,
    pub note: Option<String>,
}

pub fn user_db_path_in(data_dir: &Path) -> PathBuf {
    data_dir.join(APP_DIR).join(USER_DB_NAME)
}

pub fn user_db_path() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BIBLE_APP_USER_DB") {
        let p = PathBuf::from(p);
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }
    Some(user_db_path_in(&dirs::data_dir()?))
}

pub fn open_default() -> Result<Connection, String> {
    let path = user_db_path().ok_or_else(|| {
        "Could not determine a data directory for bookmarks, notes, and highlights.".to_string()
    })?;
    open(&path).map_err(|e| format!("Could not open {}:\n{e}", path.display()))
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let conn = Connection::open(path)?;
    init_schema(&conn)?;
    Ok(conn)
}

#[cfg(test)]
pub fn open_memory() -> rusqlite::Result<Connection> {
    let conn = Connection::open_in_memory()?;
    init_schema(&conn)?;
    Ok(conn)
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS bookmarks (
            book       INTEGER NOT NULL,
            chapter    INTEGER NOT NULL,
            verse      INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            label      TEXT NOT NULL DEFAULT '',
            PRIMARY KEY (book, chapter, verse)
        );

        CREATE TABLE IF NOT EXISTS notes (
            book       INTEGER NOT NULL,
            chapter    INTEGER NOT NULL,
            verse      INTEGER NOT NULL,
            text       TEXT NOT NULL,
            updated_at INTEGER NOT NULL,
            PRIMARY KEY (book, chapter, verse)
        );

        CREATE TABLE IF NOT EXISTS highlights (
            book       INTEGER NOT NULL,
            chapter    INTEGER NOT NULL,
            verse      INTEGER NOT NULL,
            color      TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            PRIMARY KEY (book, chapter, verse)
        );
        "#,
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
        [USER_SCHEMA_VERSION.to_string()],
    )?;
    Ok(())
}

pub fn parse_color(s: &str) -> Option<&'static str> {
    let s = s.trim();
    HIGHLIGHT_COLORS
        .iter()
        .copied()
        .find(|c| c.eq_ignore_ascii_case(s))
}

pub fn is_bookmarked(conn: &Connection, at: Ref) -> rusqlite::Result<bool> {
    let found: Option<i32> = conn
        .query_row(
            "SELECT 1 FROM bookmarks WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
            [at.book, at.chapter, at.verse],
            |row| row.get(0),
        )
        .optional()?;
    Ok(found.is_some())
}

pub fn toggle_bookmark(conn: &Connection, at: Ref) -> rusqlite::Result<bool> {
    if is_bookmarked(conn, at)? {
        conn.execute(
            "DELETE FROM bookmarks WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
            [at.book, at.chapter, at.verse],
        )?;
        Ok(false)
    } else {
        conn.execute(
            "INSERT INTO bookmarks (book, chapter, verse, created_at, label)
             VALUES (?1, ?2, ?3, ?4, '')",
            rusqlite::params![at.book, at.chapter, at.verse, now_unix()],
        )?;
        Ok(true)
    }
}

pub fn list_bookmarks(conn: &Connection) -> rusqlite::Result<Vec<Bookmark>> {
    let mut stmt = conn.prepare(
        "SELECT book, chapter, verse, created_at, label
         FROM bookmarks
         ORDER BY book, chapter, verse",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Bookmark {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            created_at: row.get(3)?,
            label: row.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn get_note(conn: &Connection, at: Ref) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT text FROM notes WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
        [at.book, at.chapter, at.verse],
        |row| row.get(0),
    )
    .optional()
}

pub fn upsert_note(conn: &Connection, at: Ref, text: &str) -> rusqlite::Result<()> {
    let text = text.trim();
    if text.is_empty() {
        delete_note(conn, at)?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO notes (book, chapter, verse, text, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(book, chapter, verse) DO UPDATE SET
            text = excluded.text,
            updated_at = excluded.updated_at",
        rusqlite::params![at.book, at.chapter, at.verse, text, now_unix()],
    )?;
    Ok(())
}

pub fn delete_note(conn: &Connection, at: Ref) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM notes WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
        [at.book, at.chapter, at.verse],
    )?;
    Ok(())
}

pub fn list_notes(conn: &Connection) -> rusqlite::Result<Vec<Note>> {
    let mut stmt = conn.prepare(
        "SELECT book, chapter, verse, text, updated_at
         FROM notes
         ORDER BY book, chapter, verse",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(Note {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            text: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })?;
    rows.collect()
}

pub fn get_highlight(conn: &Connection, at: Ref) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT color FROM highlights WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
        [at.book, at.chapter, at.verse],
        |row| row.get(0),
    )
    .optional()
}

pub fn set_highlight(
    conn: &Connection,
    at: Ref,
    color: Option<&str>,
) -> rusqlite::Result<Option<String>> {
    let parsed = match color {
        None => None,
        Some(s) if s.trim().is_empty() || s.eq_ignore_ascii_case("none") => None,
        Some(s) => match parse_color(s) {
            Some(c) => Some(c),
            None => return get_highlight(conn, at),
        },
    };
    match parsed {
        None => {
            conn.execute(
                "DELETE FROM highlights WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
                [at.book, at.chapter, at.verse],
            )?;
            Ok(None)
        }
        Some(color) => {
            conn.execute(
                "INSERT INTO highlights (book, chapter, verse, color, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(book, chapter, verse) DO UPDATE SET
                    color = excluded.color",
                rusqlite::params![at.book, at.chapter, at.verse, color, now_unix()],
            )?;
            Ok(Some(color.to_string()))
        }
    }
}

pub fn delete_bookmark(conn: &Connection, at: Ref) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM bookmarks WHERE book = ?1 AND chapter = ?2 AND verse = ?3",
        [at.book, at.chapter, at.verse],
    )?;
    Ok(())
}

pub fn chapter_marks(
    conn: &Connection,
    book: u8,
    chapter: u8,
) -> rusqlite::Result<HashMap<u8, VerseMarks>> {
    let mut map: HashMap<u8, VerseMarks> = HashMap::new();
    {
        let mut stmt =
            conn.prepare("SELECT verse FROM bookmarks WHERE book = ?1 AND chapter = ?2")?;
        let rows = stmt.query_map([book, chapter], |row| row.get::<_, u8>(0))?;
        for verse in rows {
            map.entry(verse?).or_default().bookmark = true;
        }
    }
    {
        let mut stmt = conn.prepare("SELECT verse FROM notes WHERE book = ?1 AND chapter = ?2")?;
        let rows = stmt.query_map([book, chapter], |row| row.get::<_, u8>(0))?;
        for verse in rows {
            map.entry(verse?).or_default().note = true;
        }
    }
    {
        let mut stmt =
            conn.prepare("SELECT verse, color FROM highlights WHERE book = ?1 AND chapter = ?2")?;
        let rows = stmt.query_map([book, chapter], |row| {
            Ok((row.get::<_, u8>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (verse, color) = row?;
            map.entry(verse).or_default().highlight = Some(color);
        }
    }
    Ok(map)
}

pub fn export_entries(conn: &Connection) -> rusqlite::Result<Vec<ExportEntry>> {
    let mut stmt = conn.prepare(
        "SELECT
            v.book, v.chapter, v.verse,
            CASE WHEN b.book IS NOT NULL THEN 1 ELSE 0 END,
            h.color,
            n.text
         FROM (
            SELECT book, chapter, verse FROM bookmarks
            UNION
            SELECT book, chapter, verse FROM notes
            UNION
            SELECT book, chapter, verse FROM highlights
         ) AS v
         LEFT JOIN bookmarks AS b
            ON b.book = v.book AND b.chapter = v.chapter AND b.verse = v.verse
         LEFT JOIN highlights AS h
            ON h.book = v.book AND h.chapter = v.chapter AND h.verse = v.verse
         LEFT JOIN notes AS n
            ON n.book = v.book AND n.chapter = v.chapter AND n.verse = v.verse
         ORDER BY v.book, v.chapter, v.verse",
    )?;
    let rows = stmt.query_map([], |row| {
        let bookmark: i32 = row.get(3)?;
        let note: Option<String> = row.get(5)?;
        Ok(ExportEntry {
            book: row.get(0)?,
            chapter: row.get(1)?,
            verse: row.get(2)?,
            bookmark: bookmark != 0,
            highlight: row.get(4)?,
            note: note.filter(|s| !s.trim().is_empty()),
        })
    })?;
    rows.collect()
}

pub fn verse_quote(library: &Connection, book: u8, chapter: u8, verse: u8) -> String {
    match bible_app_db::get_verse(library, book, chapter, verse) {
        Ok(v) => {
            let (stored, _) = layout::split_notes(&v.text);
            let (text, _) = layout::strip_supplied(&stored);
            text
        }
        Err(_) => String::new(),
    }
}

pub fn export_markdown(
    user: &Connection,
    library: &Connection,
    books: &[Book],
) -> rusqlite::Result<String> {
    let entries = export_entries(user)?;
    Ok(render_export(&entries, library, books))
}

pub fn render_export(entries: &[ExportEntry], library: &Connection, books: &[Book]) -> String {
    let mut out = String::from("# bible-app notes\n");
    if entries.is_empty() {
        out.push_str("\nNo bookmarks, notes, or highlights.\n");
        return out;
    }
    let mut last_book = 0u8;
    for e in entries {
        out.push('\n');
        if last_book != 0 && e.book != last_book {
            out.push('\n');
        }
        last_book = e.book;
        let at = Ref {
            book: e.book,
            chapter: e.chapter,
            verse: e.verse,
        };
        out.push_str("## ");
        out.push_str(&nav::format_ref(books, at));
        out.push('\n');
        if let Some(color) = &e.highlight {
            out.push_str("**Highlight:** ");
            out.push_str(color);
            out.push('\n');
        }
        if e.bookmark {
            out.push_str("**Bookmark**\n");
        }
        let quote = verse_quote(library, e.book, e.chapter, e.verse);
        if !quote.is_empty() {
            for line in quote.lines() {
                out.push_str("> ");
                out.push_str(line);
                out.push('\n');
            }
        }
        if let Some(note) = &e.note {
            let note = note.trim();
            if !note.is_empty() {
                out.push('\n');
                out.push_str(note);
                out.push('\n');
            }
        }
    }
    out
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(book: u8, chapter: u8, verse: u8) -> Ref {
        Ref {
            book,
            chapter,
            verse,
        }
    }

    fn seed_library() -> Connection {
        let conn = bible_app_db::open_memory().unwrap();
        conn.execute_batch(
            r#"
            INSERT INTO books (id, abbrev, name) VALUES (1, 'Ge', 'Genesis'), (2, 'Ex', 'Exodus');
            INSERT INTO verses (book, chapter, verse, text, para_break) VALUES
                (1, 1, 1, 'In the beginning God created the [heaven] and the earth. {Heb. created.}', 1),
                (1, 1, 2, 'And the earth was without form', 0),
                (2, 1, 1, 'Now these are the names of the children of Israel', 1);
            "#,
        )
        .unwrap();
        conn
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    #[test]
    fn user_db_path_in_joins_app_dir() {
        let dir = PathBuf::from("/tmp/xdg-data");
        assert_eq!(
            user_db_path_in(&dir),
            PathBuf::from("/tmp/xdg-data/bible-app/user.sqlite")
        );
    }

    #[test]
    fn file_roundtrip_uses_temp_dir() {
        let dir = std::env::temp_dir().join(format!(
            "bible-app-user-{}-{}",
            std::process::id(),
            now_unix()
        ));
        fs::create_dir_all(&dir).unwrap();
        let path = user_db_path_in(&dir);
        {
            let conn = open(&path).unwrap();
            assert!(toggle_bookmark(&conn, at(1, 1, 1)).unwrap());
            upsert_note(&conn, at(1, 1, 1), "a note").unwrap();
            set_highlight(&conn, at(1, 1, 1), Some("gold")).unwrap();
        }
        let conn = open(&path).unwrap();
        assert!(is_bookmarked(&conn, at(1, 1, 1)).unwrap());
        assert_eq!(
            get_note(&conn, at(1, 1, 1)).unwrap().as_deref(),
            Some("a note")
        );
        assert_eq!(
            get_highlight(&conn, at(1, 1, 1)).unwrap().as_deref(),
            Some("gold")
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bookmark_toggle_and_uniqueness() {
        let conn = open_memory().unwrap();
        assert!(toggle_bookmark(&conn, at(1, 1, 1)).unwrap());
        assert!(is_bookmarked(&conn, at(1, 1, 1)).unwrap());
        assert_eq!(count(&conn, "bookmarks"), 1);
        let err = conn.execute(
            "INSERT INTO bookmarks (book, chapter, verse, created_at, label)
             VALUES (1, 1, 1, 0, '')",
            [],
        );
        assert!(err.is_err());
        assert!(!toggle_bookmark(&conn, at(1, 1, 1)).unwrap());
        assert!(!is_bookmarked(&conn, at(1, 1, 1)).unwrap());
        assert_eq!(count(&conn, "bookmarks"), 0);
        assert!(toggle_bookmark(&conn, at(1, 1, 1)).unwrap());
        assert!(toggle_bookmark(&conn, at(1, 1, 2)).unwrap());
        assert_eq!(list_bookmarks(&conn).unwrap().len(), 2);
    }

    #[test]
    fn note_upsert_replace_and_empty_deletes() {
        let conn = open_memory().unwrap();
        upsert_note(&conn, at(43, 3, 16), "first").unwrap();
        upsert_note(&conn, at(43, 3, 16), "  second  ").unwrap();
        assert_eq!(
            get_note(&conn, at(43, 3, 16)).unwrap().as_deref(),
            Some("second")
        );
        assert_eq!(count(&conn, "notes"), 1);
        upsert_note(&conn, at(43, 3, 16), "   \n").unwrap();
        assert_eq!(get_note(&conn, at(43, 3, 16)).unwrap(), None);
        assert_eq!(count(&conn, "notes"), 0);
        upsert_note(&conn, at(1, 1, 1), "genesis").unwrap();
        delete_note(&conn, at(1, 1, 1)).unwrap();
        assert!(list_notes(&conn).unwrap().is_empty());
    }

    #[test]
    fn highlight_set_replace_and_remove() {
        let conn = open_memory().unwrap();
        assert_eq!(
            set_highlight(&conn, at(1, 1, 1), Some("gold"))
                .unwrap()
                .as_deref(),
            Some("gold")
        );
        assert_eq!(
            set_highlight(&conn, at(1, 1, 1), Some("GREEN"))
                .unwrap()
                .as_deref(),
            Some("green")
        );
        assert_eq!(count(&conn, "highlights"), 1);
        assert_eq!(
            get_highlight(&conn, at(1, 1, 1)).unwrap().as_deref(),
            Some("green")
        );
        assert_eq!(
            set_highlight(&conn, at(1, 1, 1), Some("nope"))
                .unwrap()
                .as_deref(),
            Some("green")
        );
        assert_eq!(count(&conn, "highlights"), 1);
        set_highlight(&conn, at(1, 1, 1), Some("none")).unwrap();
        assert_eq!(get_highlight(&conn, at(1, 1, 1)).unwrap(), None);
        set_highlight(&conn, at(1, 1, 1), Some("blue")).unwrap();
        set_highlight(&conn, at(1, 1, 1), None).unwrap();
        assert_eq!(count(&conn, "highlights"), 0);
    }

    #[test]
    fn chapter_marks_merge_three_tables() {
        let conn = open_memory().unwrap();
        toggle_bookmark(&conn, at(1, 1, 1)).unwrap();
        upsert_note(&conn, at(1, 1, 1), "note").unwrap();
        set_highlight(&conn, at(1, 1, 2), Some("rose")).unwrap();
        upsert_note(&conn, at(1, 2, 1), "other chapter").unwrap();
        let marks = chapter_marks(&conn, 1, 1).unwrap();
        assert_eq!(marks.len(), 2);
        assert!(marks[&1].bookmark);
        assert!(marks[&1].note);
        assert!(marks[&1].highlight.is_none());
        assert!(!marks[&2].bookmark);
        assert_eq!(marks[&2].highlight.as_deref(), Some("rose"));
        assert!(!marks.contains_key(&3));
    }

    #[test]
    fn export_markdown_citation_quote_without_notes_or_brackets() {
        let library = seed_library();
        let user = open_memory().unwrap();
        let books = bible_app_db::books(&library).unwrap();
        toggle_bookmark(&user, at(1, 1, 1)).unwrap();
        set_highlight(&user, at(1, 1, 1), Some("gold")).unwrap();
        upsert_note(&user, at(1, 1, 1), "My note text here.").unwrap();
        upsert_note(&user, at(2, 1, 1), "An Exodus note.").unwrap();
        let md = export_markdown(&user, &library, &books).unwrap();
        assert!(md.starts_with("# bible-app notes\n"));
        assert!(md.contains("## Genesis 1:1"));
        assert!(md.contains("**Highlight:** gold"));
        assert!(md.contains("**Bookmark**"));
        assert!(md.contains("> In the beginning God created the heaven and the earth."));
        assert!(!md.contains("{Heb."));
        assert!(!md.contains("[heaven]"));
        assert!(!md.contains("Heb. created"));
        assert!(md.contains("My note text here."));
        assert!(md.contains("## Exodus 1:1"));
        assert!(md.contains("An Exodus note."));
        let genesis = md.find("## Genesis 1:1").unwrap();
        let exodus = md.find("## Exodus 1:1").unwrap();
        assert!(genesis < exodus);
    }

    #[test]
    fn parse_color_accepts_known_names() {
        assert_eq!(parse_color("Gold"), Some("gold"));
        assert_eq!(parse_color(" rose "), Some("rose"));
        assert_eq!(parse_color("purple"), None);
        assert_eq!(parse_color(""), None);
        assert_eq!(parse_color(DEFAULT_HIGHLIGHT), Some("gold"));
    }
}
