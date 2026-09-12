use super::Error;
use std::fs;
use std::path::Path;

/// `*.bt0` / `*.ct0` header: Pascal title at byte 0, id at offset 0x33.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleHeader {
    pub title: String,
    pub id: String,
}

const ID_OFFSET: usize = 0x33;

impl ModuleHeader {
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = fs::read(path)?;
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() {
            return Err(Error::Format("empty module header".into()));
        }
        let title_len = bytes[0] as usize;
        if 1 + title_len > bytes.len() {
            return Err(Error::Format("title overruns header".into()));
        }
        let title = latin1(&bytes[1..1 + title_len]);
        if bytes.len() <= ID_OFFSET {
            return Err(Error::Format("header too short for id".into()));
        }
        let id_len = bytes[ID_OFFSET] as usize;
        let id_start = ID_OFFSET + 1;
        if id_start + id_len > bytes.len() {
            return Err(Error::Format("id overruns header".into()));
        }
        let id = latin1(&bytes[id_start..id_start + id_len]);
        Ok(Self { title, id })
    }
}

fn latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_kjv_shaped_header() {
        let mut buf = vec![0u8; 80];
        let title = b"King James Version";
        buf[0] = title.len() as u8;
        buf[1..1 + title.len()].copy_from_slice(title);
        buf[0x33] = 3;
        buf[0x34..0x37].copy_from_slice(b"KJV");
        let h = ModuleHeader::parse(&buf).unwrap();
        assert_eq!(h.title, "King James Version");
        assert_eq!(h.id, "KJV");
    }
}
