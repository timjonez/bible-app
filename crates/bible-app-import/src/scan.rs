use crate::allowlist::{self, Decision};
use rusqlite::Connection;
use std::fs;
use std::io;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Default)]
pub struct ScanReport {
    pub imported: Vec<(String, String)>,
    pub deferred: Vec<(String, String)>,
    pub skipped: Vec<(String, String)>,
}

pub fn scan_dump(dir: &Path, conn: &Connection) -> Result<ScanReport, ScanError> {
    let mut report = ScanReport::default();
    let mut entries: Vec<_> = fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(stem) = allowlist::module_stem(name) else {
            continue;
        };
        let class = allowlist::classify(stem);
        let action = match class.decision {
            Decision::Import => "import",
            Decision::Defer => "defer",
            Decision::Skip => "skip",
        };
        conn.execute(
            "INSERT OR REPLACE INTO import_log (stem, action, reason) VALUES (?1, ?2, ?3)",
            rusqlite::params![stem, action, class.reason],
        )?;
        let row = (stem.to_string(), class.reason.to_string());
        match class.decision {
            Decision::Import => report.imported.push(row),
            Decision::Defer => report.deferred.push(row),
            Decision::Skip => report.skipped.push(row),
        }
    }
    Ok(report)
}

impl ScanReport {
    pub fn print(&self) {
        eprintln!(
            "scan: {} import, {} defer, {} skip",
            self.imported.len(),
            self.deferred.len(),
            self.skipped.len()
        );
        for (stem, reason) in &self.imported {
            eprintln!("  import  {stem}: {reason}");
        }
        for (stem, reason) in &self.skipped {
            eprintln!("  skip    {stem}: {reason}");
        }
        for (stem, reason) in &self.deferred {
            eprintln!("  defer   {stem}: {reason}");
        }
    }
}
