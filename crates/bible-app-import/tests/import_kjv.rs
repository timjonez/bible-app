use bible_app_db::{self, get_verse};
use bible_app_import::allowlist::{classify, Decision};
use bible_app_import::import_from;
use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

const GEN_1_1: &str = "In the beginning God created the heaven and the earth.";
const JOHN_3_16: &str =
    "For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life.";
const REV_22_21: &str = "The grace of our Lord Jesus Christ [be] with you all. Amen.";

fn cd_dump() -> Option<PathBuf> {
    let p = PathBuf::from("/home/tim/BibleCD");
    if p.join("KJV.bt4").is_file() {
        Some(p)
    } else {
        None
    }
}

fn write_synthetic_dump(dir: &Path) {
    // Minimal KJV plus a copyrighted bible header and a deferred commentary header.
    let mut bt0 = vec![0u8; 80];
    let title = b"King James Version";
    bt0[0] = title.len() as u8;
    bt0[1..1 + title.len()].copy_from_slice(title);
    bt0[0x33] = 3;
    bt0[0x34..0x37].copy_from_slice(b"KJV");
    fs::write(dir.join("KJV.bt0"), bt0).unwrap();
    let mut bt3 = String::new();
    for n in 1..=66u8 {
        let (abbr, name) = match n {
            1 => ("Ge", "Genesis"),
            43 => ("Joh", "John"),
            66 => ("Re", "Revelation"),
            _ => ("X", "Placeholder"),
        };
        bt3.push_str(abbr);
        bt3.push_str("\r\n");
        bt3.push_str(name);
        bt3.push_str("\r\n");
    }
    fs::write(dir.join("KJV.bt3"), bt3).unwrap();

    let v1 = b"\xb6In the beginning\x01 God\x01 created\x01\x01 the heaven\x01 and\x01 the earth\x01.\x00";
    let v2 = b"For\x01 God\x01 so\x01 loved\x01 the world\x01, that\x01 he gave\x01 his\x01 only begotten\x01 Son\x01, that\x01 whosoever\x01 believeth\x01 in\x01 him\x01 should\x01 not\x01 perish\x01, but\x01 have\x01 everlasting\x01 life\x01.\x00";
    let v3 = b"The grace\x01 of our\x01 Lord\x01 Jesus\x01 Christ\x01 [be] with\x01 you\x01 all\x01. Amen\x01.\x00";
    let mut bt4 = Vec::new();
    let off1 = 0u32;
    bt4.extend_from_slice(v1);
    let off2 = bt4.len() as u32;
    bt4.extend_from_slice(v2);
    let off3 = bt4.len() as u32;
    bt4.extend_from_slice(v3);
    fs::write(dir.join("KJV.bt4"), &bt4).unwrap();

    let mut bt7 = Vec::new();
    for (off, b, c, v) in [(off1, 1u8, 1u8, 1u8), (off2, 43, 3, 16), (off3, 66, 22, 21)] {
        bt7.extend_from_slice(&off.to_le_bytes());
        bt7.push(b);
        bt7.push(c);
        bt7.push(v);
    }
    fs::write(dir.join("KJV.bt7"), bt7).unwrap();

    let mut niv = vec![0u8; 80];
    let t = b"New International Version";
    niv[0] = t.len() as u8;
    niv[1..1 + t.len()].copy_from_slice(t);
    niv[0x33] = 3;
    niv[0x34..0x37].copy_from_slice(b"NIV");
    fs::write(dir.join("NIV.bt0"), niv).unwrap();
    fs::write(dir.join("NIV.bt4"), b"should never be read").unwrap();

    let mut mhc = vec![0u8; 80];
    let t = b"Matthew Henry's Commentary on the Whole Bible";
    mhc[0] = t.len() as u8;
    mhc[1..1 + t.len()].copy_from_slice(t);
    mhc[0x33] = 3;
    mhc[0x34..0x37].copy_from_slice(b"MHC");
    fs::write(dir.join("MHC.ct0"), mhc).unwrap();

    let mut mhc4 = Vec::new();
    mhc4.extend_from_slice(b"1\x00Henry on Genesis 1:1.");
    let v2 = mhc4.len() as u32;
    mhc4.extend_from_slice(b"16\x00Henry on John 3:16, the only begotten Son.");
    fs::write(dir.join("MHC.ct4"), &mhc4).unwrap();
    let mut mhc7 = Vec::new();
    mhc7.extend_from_slice(&0u32.to_le_bytes());
    mhc7.extend_from_slice(&2u32.to_le_bytes());
    mhc7.extend_from_slice(&v2.to_le_bytes());
    mhc7.extend_from_slice(&(v2 + 3).to_le_bytes());
    mhc7.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    mhc7.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    fs::write(dir.join("MHC.ct7"), mhc7).unwrap();
}

#[test]
fn synthetic_dump_imports_only_kjv_goldens() {
    let tmp = tempfile::tempdir().unwrap();
    let from = tmp.path().join("cd");
    fs::create_dir(&from).unwrap();
    write_synthetic_dump(&from);
    let out = tmp.path().join("bible-app.sqlite");

    let stats = import_from(&from, &out).unwrap();
    assert_eq!(stats.verses, 3);
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("KJV")));
    assert!(stats
        .report
        .skipped
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("NIV")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("MHC")));
    assert_eq!(stats.resources, 2);

    let conn = Connection::open(&out).unwrap();
    bible_app_db::ensure_verses_fts(&conn).unwrap();
    assert_eq!(get_verse(&conn, 1, 1, 1).unwrap().text, GEN_1_1);
    assert!(get_verse(&conn, 1, 1, 1).unwrap().para_break);
    assert_eq!(get_verse(&conn, 43, 3, 16).unwrap().text, JOHN_3_16);
    assert_eq!(get_verse(&conn, 66, 22, 21).unwrap().text, REV_22_21);

    // NIV.bt4 must not have been opened as a source of verses.
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM verses", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);

    let hits = bible_app_db::search_verses(&conn, "only begotten", 20).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].book, hits[0].chapter, hits[0].verse), (43, 3, 16));

    let henry = bible_app_db::resource_covering(&conn, "MHC", 43, 3, 16)
        .unwrap()
        .unwrap();
    assert!(henry.text.contains("only begotten"));
}

#[test]
fn real_cd_kjv_goldens() {
    let Some(from) = cd_dump() else {
        eprintln!("skipping real_cd_kjv_goldens: /home/tim/BibleCD/KJV.bt4 not found");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let out = tmp.path().join("bible-app.sqlite");
    let stats = import_from(&from, &out).unwrap();
    assert_eq!(stats.verses, 31_102);
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("KJV")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("MHC")));
    assert!(stats.resources > 0);
    assert!(!stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("NIV")));
    for banned in ["NIV", "WEB", "NASB", "NKJV", "RSV"] {
        assert_eq!(classify(banned).decision, Decision::Skip, "{banned}");
        assert!(
            stats
                .report
                .skipped
                .iter()
                .any(|(s, _)| s.eq_ignore_ascii_case(banned)),
            "expected {banned} in skip log"
        );
    }
    assert!(stats
        .report
        .deferred
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("TSK")));

    let conn = bible_app_db::open(&out).unwrap();
    let gen = get_verse(&conn, 1, 1, 1).unwrap();
    assert_eq!(gen.text, GEN_1_1);
    assert!(gen.para_break);
    assert_eq!(get_verse(&conn, 43, 3, 16).unwrap().text, JOHN_3_16);
    assert_eq!(get_verse(&conn, 66, 22, 21).unwrap().text, REV_22_21);
    assert_eq!(bible_app_db::books(&conn).unwrap().len(), 66);

    let henry = bible_app_db::resource_covering(&conn, "MHC", 43, 3, 16)
        .unwrap()
        .expect("MHC should cover John 3:16");
    assert!(
        henry.text.to_lowercase().contains("eternal")
            || henry.text.to_lowercase().contains("believ"),
        "unexpected MHC covering John 3:16: {}",
        henry.text
    );

    let hits = bible_app_db::search_verses(&conn, "only begotten", 50).unwrap();
    assert!(
        hits.iter()
            .any(|h| h.book == 43 && h.chapter == 3 && h.verse == 16),
        "John 3:16 missing from {hits:?}"
    );
}
