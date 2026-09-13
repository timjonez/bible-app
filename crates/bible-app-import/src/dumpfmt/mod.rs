//! Source dump binary layout. Used only by the importer.

mod bible;
mod header;
mod resource;
mod verse_id;

pub use bible::{decode_verse_text, load_kjv, BookName, KjvModule, VerseRec};
pub use header::ModuleHeader;
pub use resource::{load_commentary, ResourceModule, XrefDest};
pub use verse_id::hex_id_to_index;

use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("format: {0}")]
    Format(String),
}
