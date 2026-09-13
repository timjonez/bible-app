use super::header::ModuleHeader;
use super::Error;
use memmap2::Mmap;
use std::fs::File;
use std::path::Path;

const PARA_MARK: u8 = 0xB6;
const WORD_BREAK: u8 = 0x01;
const VERSE_END: u8 = 0x00;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BookName {
    pub id: u8,
    pub abbrev: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerseRec {
    pub offset: u32,
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
}

pub struct KjvModule {
    pub header: ModuleHeader,
    pub books: Vec<BookName>,
    pub index: Vec<VerseRec>,
    text: Mmap,
}

impl KjvModule {
    pub fn verse_raw(&self, rec: VerseRec, next_offset: Option<u32>) -> Result<&[u8], Error> {
        let start = rec.offset as usize;
        let end = match next_offset {
            Some(n) => n as usize,
            None => self.text.len(),
        };
        if start > end || end > self.text.len() {
            return Err(Error::Format(format!(
                "verse {}:{}:{} slice {start}..{end} out of range {}",
                rec.book,
                rec.chapter,
                rec.verse,
                self.text.len()
            )));
        }
        Ok(&self.text[start..end])
    }
}

pub fn load_kjv(dir: &Path) -> Result<KjvModule, Error> {
    let header = ModuleHeader::read(&dir.join("KJV.bt0"))?;
    if header.id != "KJV" {
        return Err(Error::Format(format!(
            "expected KJV module, found id {:?}",
            header.id
        )));
    }
    let books = parse_books(&std::fs::read(dir.join("KJV.bt3"))?)?;
    let index = parse_index(&std::fs::read(dir.join("KJV.bt7"))?)?;
    let file = File::open(dir.join("KJV.bt4"))?;
    let text = unsafe { Mmap::map(&file)? };
    Ok(KjvModule {
        header,
        books,
        index,
        text,
    })
}

pub fn parse_books(bytes: &[u8]) -> Result<Vec<BookName>, Error> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::Format("KJV.bt3 is not UTF-8/ASCII".into()))?
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    let (pairs, rest) = lines.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(Error::Format(format!(
            "KJV.bt3 expected abbrev/name pairs, got {} lines",
            lines.len()
        )));
    }
    let mut books = Vec::with_capacity(pairs.len());
    for (i, pair) in pairs.iter().enumerate() {
        books.push(BookName {
            id: u8::try_from(i + 1).map_err(|_| Error::Format("too many books".into()))?,
            abbrev: pair[0].to_string(),
            name: pair[1].to_string(),
        });
    }
    Ok(books)
}

pub fn parse_index(bytes: &[u8]) -> Result<Vec<VerseRec>, Error> {
    let (chunks, rest) = bytes.as_chunks::<7>();
    if !rest.is_empty() {
        return Err(Error::Format(format!(
            "KJV.bt7 length {} is not a multiple of 7",
            bytes.len()
        )));
    }
    let mut recs = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let offset = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
        recs.push(VerseRec {
            offset,
            book: chunk[4],
            chapter: chunk[5],
            verse: chunk[6],
        });
    }
    Ok(recs)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrongRef {
    Greek(u16),
    Hebrew(u16),
}

impl StrongRef {
    pub fn from_i16(n: i16) -> Option<Self> {
        match n {
            0 | -1 => None,
            n if n > 0 => Some(Self::Greek(n as u16)),
            n => Some(Self::Hebrew(n.unsigned_abs())),
        }
    }

    pub fn code(self) -> String {
        match self {
            Self::Greek(n) => format!("G{n}"),
            Self::Hebrew(n) => format!("H{n}"),
        }
    }

    pub fn lang(self) -> &'static str {
        match self {
            Self::Greek(_) => "G",
            Self::Hebrew(_) => "H",
        }
    }

    pub fn num(self) -> u16 {
        match self {
            Self::Greek(n) | Self::Hebrew(n) => n,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerseWord {
    pub i: u16,
    pub start: u32,
    pub end: u32,
    pub refs: Vec<StrongRef>,
}

/// Display text: drop `0x00` terminator, leading `0xB6` paragraph mark, and `0x01` word breaks.
/// Word-break bytes sit *between* tokens that already include their own spaces, so they are
/// deleted rather than turned into extra spaces.
pub fn decode_verse_text(raw: &[u8]) -> (bool, String) {
    let (para, text, _) = decode_verse_words(raw, &[]);
    (para, text)
}

/// Align `KJV.bt8` signed Strong's numbers (one per `0x01`) with display-text spans.
/// Empty tokens (the Hebrew object marker) append their numbers to the previous word.
pub fn decode_verse_words(raw: &[u8], nums: &[i16]) -> (bool, String, Vec<VerseWord>) {
    let mut bytes = raw;
    if bytes.last() == Some(&VERSE_END) {
        bytes = &bytes[..bytes.len() - 1];
    }
    let para = bytes.first() == Some(&PARA_MARK);
    if para {
        bytes = &bytes[1..];
    }

    let parts: Vec<&[u8]> = bytes.split(|&b| b == WORD_BREAK).collect();
    let mut text = String::new();
    let mut words: Vec<VerseWord> = Vec::new();
    let mut pending: Vec<StrongRef> = Vec::new();
    let tagged = nums.len().min(parts.len().saturating_sub(1));

    for (idx, part) in parts.iter().enumerate() {
        let piece: String = part
            .iter()
            .copied()
            .map(|b| if b < 128 { b as char } else { '\u{FFFD}' })
            .collect();
        let start = text.chars().count();
        text.push_str(&piece);
        let end = text.chars().count();

        if idx >= tagged {
            continue;
        }
        let Some(sref) = StrongRef::from_i16(nums[idx]) else {
            continue;
        };

        let lead = piece.chars().count() - piece.trim_start().chars().count();
        let trail = piece.chars().count() - piece.trim_end().chars().count();
        let ts = start + lead;
        let te = end.saturating_sub(trail);
        if ts >= te {
            if let Some(word) = words.last_mut() {
                word.refs.push(sref);
            } else {
                pending.push(sref);
            }
            continue;
        }
        let mut refs = std::mem::take(&mut pending);
        refs.push(sref);
        let i = u16::try_from(words.len()).unwrap_or(u16::MAX);
        words.push(VerseWord {
            i,
            start: ts as u32,
            end: te as u32,
            refs,
        });
    }
    if !pending.is_empty() {
        if let Some(word) = words.last_mut() {
            word.refs.extend(pending);
        }
    }
    (para, text, words)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_gen_1_1_shape() {
        let raw = b"\xb6In the beginning\x01 God\x01 created\x01\x01 the heaven\x01 and\x01 the earth\x01.\x00";
        let (para, text) = decode_verse_text(raw);
        assert!(para);
        assert_eq!(
            text,
            "In the beginning God created the heaven and the earth."
        );
    }

    #[test]
    fn decode_john_3_16_shape() {
        let raw = b"For\x01 God\x01 so\x01 loved\x01 the world\x01, that\x01 he gave\x01 his\x01 only begotten\x01 Son\x01, that\x01 whosoever\x01 believeth\x01 in\x01 him\x01 should\x01 not\x01 perish\x01, but\x01 have\x01 everlasting\x01 life\x01.\x00";
        let (para, text) = decode_verse_text(raw);
        assert!(!para);
        assert_eq!(
            text,
            "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life."
        );
    }

    #[test]
    fn gen_1_1_words_merge_eth() {
        let raw = b"\xb6In the beginning\x01 God\x01 created\x01\x01 the heaven\x01 and\x01 the earth\x01.\x00";
        let nums = [-7225i16, -430, -1254, -853, -8064, -853, -776];
        let (para, text, words) = decode_verse_words(raw, &nums);
        assert!(para);
        assert_eq!(
            text,
            "In the beginning God created the heaven and the earth."
        );
        let codes: Vec<Vec<String>> = words
            .iter()
            .map(|w| w.refs.iter().map(|r| r.code()).collect())
            .collect();
        assert_eq!(
            codes,
            vec![
                vec!["H7225".to_string()],
                vec!["H430".to_string()],
                vec!["H1254".to_string(), "H853".to_string()],
                vec!["H8064".to_string()],
                vec!["H853".to_string()],
                vec!["H776".to_string()],
            ]
        );
        let god = &words[1];
        assert_eq!(&text[god.start as usize..god.end as usize], "God");
    }

    #[test]
    fn john_3_16_god_is_g2316() {
        let raw = b"For\x01 God\x01 so\x01 loved\x01 the world\x01, that\x01 he gave\x01 his\x01 only begotten\x01 Son\x01, that\x01 whosoever\x01 believeth\x01 in\x01 him\x01 should\x01 not\x01 perish\x01, but\x01 have\x01 everlasting\x01 life\x01.\x00";
        let nums = [
            1063i16, 2316, 3779, 25, 2889, 5620, 1325, 846, 3439, 5207, 2443, 3956, 4100, 1519,
            846, 622, 3361, 622, 235, 2192, 166, 2222,
        ];
        let (_, text, words) = decode_verse_words(raw, &nums);
        let god = words.iter().find(|w| w.refs == [StrongRef::Greek(2316)]);
        let god = god.expect("G2316");
        assert_eq!(&text[god.start as usize..god.end as usize], "God");
    }

    #[test]
    fn parse_three_book_names() {
        let bt3 = b"Ge\r\nGenesis\r\nJoh\r\nJohn\r\nRe\r\nRevelation\r\n";
        let books = parse_books(bt3).unwrap();
        assert_eq!(books.len(), 3);
        assert_eq!(books[0].abbrev, "Ge");
        assert_eq!(books[2].name, "Revelation");
        assert_eq!(books[2].id, 3);
    }

    #[test]
    fn parse_index_record() {
        let mut buf = [0u8; 7];
        buf[0..4].copy_from_slice(&4105660u32.to_le_bytes());
        buf[4] = 43;
        buf[5] = 3;
        buf[6] = 16;
        let recs = parse_index(&buf).unwrap();
        assert_eq!(
            recs[0],
            VerseRec {
                offset: 4105660,
                book: 43,
                chapter: 3,
                verse: 16
            }
        );
    }
}
