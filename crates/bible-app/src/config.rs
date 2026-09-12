use crate::nav::Ref;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "bible-app";
const DB_NAME: &str = "bible-app.sqlite";
const STATE_NAME: &str = "state.toml";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct State {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
}

impl Default for State {
    fn default() -> Self {
        Self {
            book: 1,
            chapter: 1,
            verse: 1,
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

impl From<Ref> for State {
    fn from(r: Ref) -> Self {
        Self {
            book: r.book,
            chapter: r.chapter,
            verse: r.verse,
        }
    }
}

pub fn find_database() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("BIBLE_APP_DB") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(dir) = dirs::data_dir() {
        let p = dir.join(APP_DIR).join(DB_NAME);
        if p.is_file() {
            return Some(p);
        }
    }
    let local = PathBuf::from("data").join(DB_NAME);
    if local.is_file() {
        return Some(local);
    }
    None
}

pub fn import_hint() -> String {
    "cargo run -p bible-app-import -- --from /path/to/source-dump --out data/bible-app.sqlite\n\n\
     Then either keep the file at data/bible-app.sqlite, copy it to ~/.local/share/bible-app/, \
     or set BIBLE_APP_DB."
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

pub fn save_state(at: Ref) {
    let Some(path) = state_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let text = toml::to_string_pretty(&State::from(at)).unwrap_or_default();
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
