//! Source dump binary layout. Used only by the importer.

mod bible;
mod header;
mod resource;
mod strongs;
mod verse_id;

pub use bible::{
    decode_verse_text, decode_verse_words, load_kjv, BookName, KjvModule, StrongRef, VerseRec,
    VerseWord,
};
pub use header::ModuleHeader;
pub use resource::{
    load_commentary, load_dictionary, load_topic, HeadwordModule, ResourceModule, XrefDest,
};
pub use strongs::{load_lexicon, load_word_map, LexEntry, MappedVerse};
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
