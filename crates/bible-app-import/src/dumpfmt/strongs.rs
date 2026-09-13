//! Strong's maps (`KJV.bt8`) and 1890 lexicon (`StrGrk`, `StrHeb`, `Strongs.sd*`).
//!
//! `KJV.bt8`: 31,102 `u32le` payload offsets, then `i16le` numbers — one per `0x01`
//! in that verse. Positive = Greek, negative = Hebrew, `0`/`-1` = untagged.
//! `KJV.bt9` is unused (not required for word hover).

use super::bible::{decode_verse_words, KjvModule, StrongRef, VerseWord};
use super::Error;
use encoding_rs::WINDOWS_1252;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const SEE_OPEN: u8 = 0xBB;
const SEE_CLOSE: u8 = 0xAB;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MappedVerse {
    pub book: u8,
    pub chapter: u8,
    pub verse: u8,
    pub words: Vec<VerseWord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexEntry {
    pub num: u16,
    pub lang: &'static str,
    pub lemma: String,
    pub pronunciation: String,
    pub definition: String,
}

pub fn load_word_map(dir: &Path, kjv: &KjvModule) -> Result<Vec<MappedVerse>, Error> {
    let path = dir.join("KJV.bt8");
    if !path.is_file() {
        return Ok(Vec::new());
    }
    parse_word_map(&fs::read(path)?, kjv)
}

pub fn parse_word_map(bt8: &[u8], kjv: &KjvModule) -> Result<Vec<MappedVerse>, Error> {
    let n = kjv.index.len();
    let table = n
        .checked_mul(4)
        .ok_or_else(|| Error::Format("bt8 overflow".into()))?;
    if bt8.len() < table {
        return Err(Error::Format(format!(
            "KJV.bt8 length {} shorter than {n} offsets",
            bt8.len()
        )));
    }
    let mut offs = Vec::with_capacity(n);
    for i in 0..n {
        let o = u32::from_le_bytes(bt8[i * 4..i * 4 + 4].try_into().unwrap()) as usize;
        offs.push(o);
    }
    let payload = &bt8[table..];
    let mut out = Vec::with_capacity(n);
    for (i, rec) in kjv.index.iter().enumerate() {
        let start = offs[i];
        let end = offs.get(i + 1).copied().unwrap_or(payload.len());
        if start > end || end > payload.len() {
            return Err(Error::Format(format!(
                "KJV.bt8 verse {}:{}:{} span {start}..{end} out of {}",
                rec.book,
                rec.chapter,
                rec.verse,
                payload.len()
            )));
        }
        let chunk = &payload[start..end];
        if !chunk.len().is_multiple_of(2) {
            return Err(Error::Format(format!(
                "KJV.bt8 verse {}:{}:{} odd payload {}",
                rec.book,
                rec.chapter,
                rec.verse,
                chunk.len()
            )));
        }
        let nums: Vec<i16> = chunk
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| i16::from_le_bytes(*c))
            .collect();
        let next = kjv.index.get(i + 1).map(|r| r.offset);
        let raw = kjv.verse_raw(*rec, next)?;
        let n01 = raw.iter().filter(|&&b| b == 0x01).count();
        if n01 != nums.len() {
            return Err(Error::Format(format!(
                "KJV.bt8 verse {}:{}:{} has {} numbers, {} word breaks",
                rec.book,
                rec.chapter,
                rec.verse,
                nums.len(),
                n01
            )));
        }
        let (_, _, words) = decode_verse_words(raw, &nums);
        out.push(MappedVerse {
            book: rec.book,
            chapter: rec.chapter,
            verse: rec.verse,
            words,
        });
    }
    Ok(out)
}

pub fn load_lexicon(dir: &Path) -> Result<Vec<LexEntry>, Error> {
    let mut by_key: HashMap<(u16, &'static str), LexEntry> = HashMap::new();

    merge_defs(&mut by_key, load_dict(dir, "StrGrk", "gx", "G")?);
    merge_defs(&mut by_key, load_dict(dir, "StrHeb", "hx", "H")?);

    if let Some(lemmas) = load_lemmas(dir)? {
        for item in lemmas {
            let key = (item.sref.num(), item.sref.lang());
            by_key
                .entry(key)
                .and_modify(|e| {
                    if e.lemma.is_empty() {
                        e.lemma = item.lemma.clone();
                    }
                    if e.pronunciation.is_empty() {
                        e.pronunciation = item.pronunciation.clone();
                    }
                })
                .or_insert(LexEntry {
                    num: item.sref.num(),
                    lang: item.sref.lang(),
                    lemma: item.lemma,
                    pronunciation: item.pronunciation,
                    definition: String::new(),
                });
        }
    }

    let mut out: Vec<_> = by_key.into_values().collect();
    out.sort_by_key(|e| (e.lang, e.num));
    Ok(out)
}

struct DictDef {
    num: u16,
    lang: &'static str,
    definition: String,
}

fn merge_defs(into: &mut HashMap<(u16, &'static str), LexEntry>, defs: Vec<DictDef>) {
    for def in defs {
        into.entry((def.num, def.lang))
            .and_modify(|e| {
                if e.definition.is_empty() {
                    e.definition = def.definition.clone();
                }
            })
            .or_insert(LexEntry {
                num: def.num,
                lang: def.lang,
                lemma: String::new(),
                pronunciation: String::new(),
                definition: def.definition,
            });
    }
}

fn load_dict(dir: &Path, stem: &str, ext: &str, lang: &'static str) -> Result<Vec<DictDef>, Error> {
    let idx_path = dir.join(format!("{stem}.{ext}7"));
    let body_path = dir.join(format!("{stem}.{ext}4"));
    if !idx_path.is_file() || !body_path.is_file() {
        return Ok(Vec::new());
    }
    parse_dict(&fs::read(idx_path)?, &fs::read(body_path)?, lang)
}

fn parse_dict(index: &[u8], body: &[u8], lang: &'static str) -> Result<Vec<DictDef>, Error> {
    let (chunks, rest) = index.as_chunks::<8>();
    if !rest.is_empty() {
        return Err(Error::Format(format!(
            "lexicon index length {} is not a multiple of 8",
            index.len()
        )));
    }
    let mut recs = Vec::new();
    for chunk in chunks {
        let start = u32::from_le_bytes(chunk[0..4].try_into().unwrap()) as usize;
        let end = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize;
        if start > end || end > body.len() {
            return Err(Error::Format(format!(
                "lexicon span {start}..{end} out of {}",
                body.len()
            )));
        }
        let label = std::str::from_utf8(&body[start..end])
            .unwrap_or("")
            .trim_end_matches('\0');
        recs.push((parse_label(label), start, end));
    }
    let mut out = Vec::new();
    for (i, (num, _, label_end)) in recs.iter().enumerate() {
        let Some(num) = *num else { continue };
        if num == 0 {
            continue;
        }
        let text_end = recs.get(i + 1).map(|r| r.1).unwrap_or(body.len());
        if *label_end > text_end {
            return Err(Error::Format("lexicon text range inverted".into()));
        }
        let definition = decode_lexicon(&body[*label_end..text_end]);
        if definition.is_empty() {
            continue;
        }
        out.push(DictDef {
            num,
            lang,
            definition,
        });
    }
    Ok(out)
}

fn parse_label(label: &str) -> Option<u16> {
    let s = label.trim();
    if s.is_empty() {
        return None;
    }
    if s.chars().all(|c| c == '0') {
        return Some(0);
    }
    s.trim_start_matches('0').parse().ok()
}

fn decode_lexicon(raw: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            0 => i += 1,
            b'\r' => i += 1,
            b'\n' => {
                out.push('\n');
                i += 1;
            }
            SEE_OPEN => {
                i += 1;
                let start = i;
                while i < raw.len() && raw[i] != SEE_CLOSE && raw[i] != 0 {
                    i += 1;
                }
                let body = std::str::from_utf8(&raw[start..i]).unwrap_or("");
                if i < raw.len() && raw[i] == SEE_CLOSE {
                    i += 1;
                }
                out.push_str(&see_label(body));
            }
            b => {
                let ch = if b < 128 {
                    b as char
                } else {
                    WINDOWS_1252
                        .decode(&[b])
                        .0
                        .chars()
                        .next()
                        .unwrap_or('\u{FFFD}')
                };
                out.push(ch);
                i += 1;
            }
        }
    }
    collapse(out)
}

fn see_label(body: &str) -> String {
    let s = body.trim();
    if let Some(rest) = s.strip_prefix('-') {
        if rest.parse::<u16>().is_ok() {
            return format!("H{rest}");
        }
    }
    if s.parse::<u16>().is_ok() {
        return format!("G{s}");
    }
    s.to_string()
}

fn collapse(s: String) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newlines = 0;
    for c in s.chars() {
        if c == '\n' {
            newlines += 1;
            if newlines <= 2 {
                out.push('\n');
            }
        } else {
            newlines = 0;
            out.push(c);
        }
    }
    out.trim().to_string()
}

struct Lemma {
    sref: StrongRef,
    lemma: String,
    pronunciation: String,
}

fn load_lemmas(dir: &Path) -> Result<Option<Vec<Lemma>>, Error> {
    let sd0 = dir.join("Strongs.sd0");
    let sd1 = dir.join("Strongs.sd1");
    let sd2 = dir.join("Strongs.sd2");
    if !sd0.is_file() || !sd1.is_file() || !sd2.is_file() {
        return Ok(None);
    }
    Ok(Some(parse_lemmas(
        &fs::read(sd0)?,
        &fs::read(sd1)?,
        &fs::read(sd2)?,
    )?))
}

fn parse_lemmas(sd0: &[u8], sd1: &[u8], sd2: &[u8]) -> Result<Vec<Lemma>, Error> {
    if sd0.len() < 8 {
        return Err(Error::Format("Strongs.sd0 too short".into()));
    }
    let gcount = u32::from_le_bytes(sd0[0..4].try_into().unwrap()) as usize;
    let hcount = u32::from_le_bytes(sd0[4..8].try_into().unwrap()) as usize;
    let n = gcount
        .checked_add(hcount)
        .ok_or_else(|| Error::Format("Strongs.sd0 overflow".into()))?;
    if sd1.len() < n.saturating_mul(12) {
        return Err(Error::Format(format!(
            "Strongs.sd1 length {} too small for {n} records",
            sd1.len()
        )));
    }
    let mut out = Vec::new();
    for i in 0..n {
        let off = i * 12;
        let lemma_off = u32::from_le_bytes(sd1[off + 4..off + 8].try_into().unwrap()) as usize;
        let pron_off = u32::from_le_bytes(sd1[off + 8..off + 12].try_into().unwrap()) as usize;
        let lemma = cstr(sd2, lemma_off);
        let pronunciation = cstr(sd2, pron_off);
        if lemma.is_empty() && pronunciation.is_empty() {
            continue;
        }
        if lemma == "No Value" {
            continue;
        }
        let sref = if i < hcount {
            let num = u16::try_from(i).unwrap_or(0);
            if num == 0 {
                continue;
            }
            StrongRef::Hebrew(num)
        } else {
            let num = u16::try_from(i - hcount).unwrap_or(0);
            if num == 0 {
                continue;
            }
            StrongRef::Greek(num)
        };
        out.push(Lemma {
            sref,
            lemma,
            pronunciation,
        });
    }
    Ok(out)
}

fn cstr(buf: &[u8], off: usize) -> String {
    if off >= buf.len() {
        return String::new();
    }
    let end = buf[off..]
        .iter()
        .position(|&b| b == 0)
        .map(|n| off + n)
        .unwrap_or(buf.len());
    WINDOWS_1252.decode(&buf[off..end]).0.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn see_refs_use_h_or_g() {
        assert_eq!(see_label("-433"), "H433");
        assert_eq!(see_label("3588"), "G3588");
    }

    #[test]
    fn decode_strips_see_markup() {
        let raw = b"to love. See \xbb5368\xab \r\nSee \xbb-1\xab \x00";
        assert_eq!(decode_lexicon(raw), "to love. See G5368 \nSee H1");
    }
}
