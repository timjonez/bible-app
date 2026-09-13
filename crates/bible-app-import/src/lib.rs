pub mod allowlist;
pub mod pbcd;
pub mod scan;
pub mod write;

use crate::pbcd::load_kjv;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Write(#[from] write::WriteError),
    #[error(transparent)]
    Scan(#[from] scan::ScanError),
    #[error(transparent)]
    Pbcd(#[from] pbcd::Error),
}

#[derive(Debug)]
pub struct ImportStats {
    pub verses: usize,
    pub resources: usize,
    pub xrefs: usize,
    pub verse_words: usize,
    pub strongs: usize,
    pub report: scan::ScanReport,
}

pub fn import_from(from: &Path, out: &Path) -> Result<ImportStats, ImportError> {
    if !from.is_dir() {
        return Err(ImportError::Message(format!(
            "--from is not a directory: {}",
            from.display()
        )));
    }
    let mut conn = write::create_db(out)?;
    let report = scan::scan_dump(from, &conn)?;
    let kjv_ok = report
        .imported
        .iter()
        .any(|(stem, _)| stem.eq_ignore_ascii_case("KJV"));
    if !kjv_ok {
        return Err(ImportError::Message(
            "KJV.bt0 not found in --from; nothing imported".into(),
        ));
    }
    let module = load_kjv(from)?;
    let verses = write::import_kjv(&mut conn, &module)?;
    let mhc = write::import_mhc(&mut conn, from, &module)?;
    let (tsk, xrefs) = write::import_tsk(&mut conn, from, &module)?;
    let (verse_words, strongs) = write::import_strongs(&mut conn, from, &module)?;
    Ok(ImportStats {
        verses,
        resources: mhc + tsk,
        xrefs,
        verse_words,
        strongs,
        report,
    })
}
