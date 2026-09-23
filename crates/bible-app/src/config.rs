use crate::layout;
use crate::nav::Ref;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "bible-app";
const DB_NAME: &str = "bible-app.sqlite";
const ARCHIVE_NAME: &str = "bible-app.sqlite.gz";
const STATE_NAME: &str = "state.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    #[serde(default = "default_font_size")]
    pub font_size: i32,
    #[serde(default)]
    pub interlinear: bool,
    #[serde(default = "default_true")]
    pub paragraphs: bool,
    #[serde(default)]
    pub search_mode: u32,
}

fn default_font_size() -> i32 {
    layout::DEFAULT_FONT
}

fn default_true() -> bool {
    true
}

impl Default for State {
    fn default() -> Self {
        Self {
            book: 1,
            chapter: 1,
            verse: 1,
            font_size: layout::DEFAULT_FONT,
            interlinear: false,
            paragraphs: true,
            search_mode: 0,
        }
    }
}

impl From<State> for Ref {
    fn from(s: State) -> Self {
        Ref {
            book: s.book,
            chapter: s.chapter,
            verse: s.verse,
        }
    }
}

impl State {
    pub fn from_ref(r: Ref, font_size: i32, paragraphs: bool) -> Self {
        Self {
            book: r.book,
            chapter: r.chapter,
            verse: r.verse,
            font_size,
            interlinear: false,
            paragraphs,
            search_mode: 0,
        }
    }
}

pub fn locate_database() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("BIBLE_APP_DB") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    let local = PathBuf::from("data").join(DB_NAME);
    if local.is_file() {
        return Ok(local);
    }
    let gz = find_archive();
    if let Some(dest) = xdg_db_path() {
        if dest.is_file() && !archive_newer(gz.as_deref(), &dest) {
            return Ok(dest);
        }
        if let Some(gz) = gz {
            extract_gzip(&gz, &dest).map_err(|e| {
                format!(
                    "Could not unpack {} to {}:\n{e}",
                    gz.display(),
                    dest.display()
                )
            })?;
            return Ok(dest);
        }
        if dest.is_file() {
            return Ok(dest);
        }
    } else if gz.is_some() {
        return Err(
            "Found a shipped database archive but could not determine a writable data directory."
                .into(),
        );
    }
    Err(database_hint())
}

fn archive_newer(gz: Option<&Path>, dest: &Path) -> bool {
    let Some(gz) = gz else {
        return false;
    };
    let Ok(gz_time) = fs::metadata(gz).and_then(|m| m.modified()) else {
        return false;
    };
    let Ok(dest_time) = fs::metadata(dest).and_then(|m| m.modified()) else {
        return true;
    };
    gz_time > dest_time
}

fn xdg_db_path() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(APP_DIR).join(DB_NAME))
}

fn find_archive() -> Option<PathBuf> {
    let local = PathBuf::from("data").join(ARCHIVE_NAME);
    if local.is_file() {
        return Some(local);
    }
    let Ok(exe) = std::env::current_exe() else {
        return None;
    };
    let mut dir = exe.parent().map(Path::to_path_buf);
    for _ in 0..6 {
        let Some(current) = dir else {
            break;
        };
        let beside = current.join(ARCHIVE_NAME);
        if beside.is_file() {
            return Some(beside);
        }
        let nested = current.join("data").join(ARCHIVE_NAME);
        if nested.is_file() {
            return Some(nested);
        }
        dir = current.parent().map(Path::to_path_buf);
    }
    None
}

fn extract_gzip(src: &Path, dest: &Path) -> io::Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = dest.with_extension("sqlite.partial");
    {
        let input = fs::File::open(src)?;
        let mut decoder = flate2::read::GzDecoder::new(input);
        let mut output = fs::File::create(&tmp)?;
        io::copy(&mut decoder, &mut output)?;
    }
    fs::rename(tmp, dest)?;
    Ok(())
}

pub fn database_hint() -> String {
    "No bible-app.sqlite found.\n\n\
     Expected data/bible-app.sqlite.gz in this checkout (unpacked on first run to \
     ~/.local/share/bible-app/), data/bible-app.sqlite, or BIBLE_APP_DB."
        .into()
}

pub fn load_state() -> State {
    let Some(path) = state_path() else {
        return State::default();
    };
    let Ok(text) = fs::read_to_string(&path) else {
        return State::default();
    };
    toml::from_str(&text).unwrap_or_default()
}

pub fn save_state(state: &State) {
    let Some(path) = state_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = toml::to_string_pretty(state).unwrap_or_default();
    let _ = fs::write(path, text);
}

fn state_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join(APP_DIR).join(STATE_NAME))
}

pub fn database_missing_title() -> &'static str {
    "No Bible database"
}

#[allow(dead_code)]
pub fn path_display(path: &Path) -> String {
    path.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn old_state_defaults_paragraphs_on() {
        let s: State = toml::from_str("book = 1\nchapter = 1\nverse = 1\n").unwrap();
        assert!(s.paragraphs);
        assert!(!s.interlinear);
        assert_eq!(s.font_size, layout::DEFAULT_FONT);
    }

    #[test]
    fn extract_gzip_roundtrip() {
        let dir = std::env::temp_dir().join(format!("bible-app-gz-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let src = dir.join("t.sqlite.gz");
        let dest = dir.join("t.sqlite");
        {
            let file = fs::File::create(&src).unwrap();
            let mut enc = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            enc.write_all(b"sqlite-payload").unwrap();
            enc.finish().unwrap();
        }
        extract_gzip(&src, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"sqlite-payload");
        let _ = fs::remove_dir_all(&dir);
    }
}
