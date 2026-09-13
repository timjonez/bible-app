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

    let mut tsk = vec![0u8; 80];
    let t = b"The Treasury of Scripture Knowledge";
    tsk[0] = t.len() as u8;
    tsk[1..1 + t.len()].copy_from_slice(t);
    tsk[0x33] = 3;
    tsk[0x34..0x37].copy_from_slice(b"TSK");
    fs::write(dir.join("TSK.ct0"), tsk).unwrap();
    // Hex 3 in this 3-verse index is Revelation 22:21.
    let mut tsk4 = Vec::new();
    tsk4.extend_from_slice(b"1\x00beginning. \x033\x03");
    fs::write(dir.join("TSK.ct4"), &tsk4).unwrap();
    let mut tsk7 = Vec::new();
    tsk7.extend_from_slice(&0u32.to_le_bytes());
    tsk7.extend_from_slice(&2u32.to_le_bytes());
    tsk7.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    tsk7.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    tsk7.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    tsk7.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
    fs::write(dir.join("TSK.ct7"), tsk7).unwrap();

    let gen_nums: [i16; 7] = [-7225, -430, -1254, -853, -8064, -853, -776];
    let john_nums: [i16; 22] = [
        1063, 2316, 3779, 25, 2889, 5620, 1325, 846, 3439, 5207, 2443, 3956, 4100, 1519, 846, 622,
        3361, 622, 235, 2192, 166, 2222,
    ];
    let rev_nums: [i16; 9] = [5485, 2257, 2962, 2424, 5547, 3326, 5213, 3956, 281];
    let mut bt8 = Vec::new();
    let mut payload = Vec::new();
    for nums in [&gen_nums[..], &john_nums[..], &rev_nums[..]] {
        bt8.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        for n in nums {
            payload.extend_from_slice(&n.to_le_bytes());
        }
    }
    bt8.extend_from_slice(&payload);
    fs::write(dir.join("KJV.bt8"), bt8).unwrap();

    write_lexicon(
        dir,
        "StrGrk",
        "gx",
        "StrGrk",
        "Strong's Greek Dictionary",
        &[(2316, "a deity, especially the supreme Divinity.")],
    );
    write_lexicon(
        dir,
        "StrHeb",
        "hx",
        "StrHeb",
        "Strong's Hebrew Dictionary",
        &[(430, "gods in the ordinary sense; the supreme God.")],
    );
    write_lemmas(
        dir,
        2317,
        431,
        &[
            (430, "H", "'elohiym", "el-o-heem'"),
            (2316, "G", "theos", "theh'-os"),
        ],
    );
}

fn write_lexicon(
    dir: &Path,
    stem: &str,
    ext: &str,
    id: &str,
    title: &str,
    entries: &[(u16, &str)],
) {
    let mut header = vec![0u8; 80];
    header[0] = title.len() as u8;
    header[1..1 + title.len()].copy_from_slice(title.as_bytes());
    header[0x33] = id.len() as u8;
    header[0x34..0x34 + id.len()].copy_from_slice(id.as_bytes());
    fs::write(dir.join(format!("{stem}.{ext}0")), header).unwrap();

    let mut body = Vec::new();
    let mut idx = Vec::new();
    for (num, def) in entries {
        let start = body.len() as u32;
        body.extend_from_slice(num.to_string().as_bytes());
        body.push(0);
        let end = body.len() as u32;
        body.extend_from_slice(def.as_bytes());
        idx.extend_from_slice(&start.to_le_bytes());
        idx.extend_from_slice(&end.to_le_bytes());
    }
    fs::write(dir.join(format!("{stem}.{ext}4")), body).unwrap();
    fs::write(dir.join(format!("{stem}.{ext}7")), idx).unwrap();
}

fn write_lemmas(dir: &Path, gcount: u32, hcount: u32, entries: &[(u16, &str, &str, &str)]) {
    let mut sd0 = Vec::new();
    sd0.extend_from_slice(&gcount.to_le_bytes());
    sd0.extend_from_slice(&hcount.to_le_bytes());
    fs::write(dir.join("Strongs.sd0"), sd0).unwrap();

    let mut pool = vec![0u8];
    pool.extend_from_slice(b"No Value\0");
    let mut lemma_at = std::collections::HashMap::new();
    for (num, lang, lemma, pron) in entries {
        let lo = pool.len() as u32;
        pool.extend_from_slice(lemma.as_bytes());
        pool.push(0);
        let po = pool.len() as u32;
        pool.extend_from_slice(pron.as_bytes());
        pool.push(0);
        lemma_at.insert((*num, *lang), (lo, po));
    }

    let n = (gcount + hcount) as usize;
    let mut sd1 = vec![0u8; n * 12];
    for i in 0..n {
        let off = i * 12;
        sd1[off..off + 4].copy_from_slice(&1u32.to_le_bytes());
        sd1[off + 4..off + 8].copy_from_slice(&0u32.to_le_bytes());
        sd1[off + 8..off + 12].copy_from_slice(&1u32.to_le_bytes());
    }
    for ((num, lang), (lo, po)) in lemma_at {
        let idx = if lang == "H" {
            usize::from(num)
        } else {
            hcount as usize + usize::from(num)
        };
        let off = idx * 12;
        sd1[off + 4..off + 8].copy_from_slice(&lo.to_le_bytes());
        sd1[off + 8..off + 12].copy_from_slice(&po.to_le_bytes());
    }
    fs::write(dir.join("Strongs.sd1"), sd1).unwrap();
    fs::write(dir.join("Strongs.sd2"), pool).unwrap();

    write_headwords(
        dir,
        "Easton",
        "dt",
        "Easton's Bible Dictionary",
        "Easton",
        &[
            ("Aaron", "the eldest son of Amram.\x033\x03"),
            ("Abaddon", "destruction"),
        ],
    );
    write_headwords(
        dir,
        "Nave",
        "tt",
        "Nave's Topical Bible",
        "Nave",
        &[("AARON", "-Lineage of \x033\x03")],
    );
}

fn write_headwords(
    dir: &Path,
    stem: &str,
    ext: &str,
    title: &str,
    id: &str,
    entries: &[(&str, &str)],
) {
    let mut header = vec![0u8; 80];
    header[0] = title.len() as u8;
    header[1..1 + title.len()].copy_from_slice(title.as_bytes());
    header[0x33] = id.len() as u8;
    header[0x34..0x34 + id.len()].copy_from_slice(id.as_bytes());
    fs::write(dir.join(format!("{stem}.{ext}0")), header).unwrap();

    let mut body = Vec::new();
    let mut idx = Vec::new();
    for (head, text) in entries {
        let start = body.len() as u32;
        body.extend_from_slice(head.as_bytes());
        body.push(0);
        let end = body.len() as u32;
        body.extend_from_slice(text.as_bytes());
        idx.extend_from_slice(&start.to_le_bytes());
        idx.extend_from_slice(&end.to_le_bytes());
    }
    fs::write(dir.join(format!("{stem}.{ext}4")), body).unwrap();
    fs::write(dir.join(format!("{stem}.{ext}7")), idx).unwrap();
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
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("TSK")));
    assert_eq!(stats.resources, 3);
    assert_eq!(stats.xrefs, 1);
    assert!(stats.verse_words > 0);
    assert!(stats.strongs > 0);
    assert!(stats.entries >= 3);
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("StrGrk")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("Easton")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("Nave")));

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
    let xrefs = bible_app_db::xrefs_from(&conn, 1, 1, 1).unwrap();
    assert_eq!(xrefs.len(), 1);
    assert_eq!(
        (xrefs[0].book, xrefs[0].chapter, xrefs[0].verse),
        (66, 22, 21)
    );

    let words = bible_app_db::chapter_words(&conn, 43, 3).unwrap();
    let god = words
        .iter()
        .find(|w| w.verse == 16 && w.strongs.split_whitespace().any(|s| s == "G2316"))
        .expect("John 3:16 God should be G2316");
    let verse = get_verse(&conn, 43, 3, 16).unwrap();
    assert_eq!(&verse.text[god.start as usize..god.end as usize], "God");
    let def = bible_app_db::lookup_strongs(&conn, "G2316")
        .unwrap()
        .unwrap();
    assert_eq!(def.lemma, "theos");
    assert!(def.definition.contains("deity"));
    let h430 = bible_app_db::lookup_strongs(&conn, "H430")
        .unwrap()
        .unwrap();
    assert_eq!(h430.lemma, "'elohiym");

    let aaron = bible_app_db::search_entries(&conn, "Easton", "Aaron", 10).unwrap();
    assert_eq!(aaron.len(), 1);
    let (head, text) = bible_app_db::get_entry(&conn, "Easton", aaron[0].i)
        .unwrap()
        .unwrap();
    assert_eq!(head, "Aaron");
    assert!(text.contains("Amram"));
    assert!(text.contains("Revelation"));

    let nave = bible_app_db::search_entries(&conn, "Nave", "AARON", 10).unwrap();
    assert_eq!(nave[0].headword, "AARON");
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
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("TSK")));
    assert!(stats.resources > 0);
    assert!(stats.xrefs > 0);
    assert!(stats.verse_words > 0);
    assert!(stats.strongs > 0);
    assert!(stats.entries > 1000);
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("StrGrk")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("StrHeb")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("Easton")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("Smith")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("Nave")));
    assert!(stats
        .report
        .imported
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("Torrey")));
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
        .skipped
        .iter()
        .any(|(s, _)| s.eq_ignore_ascii_case("bcdxrefs")));

    let conn = bible_app_db::open(&out).unwrap();
    let gen = get_verse(&conn, 1, 1, 1).unwrap();
    assert_eq!(gen.text, GEN_1_1);
    assert!(gen.para_break);
    assert_eq!(get_verse(&conn, 43, 3, 16).unwrap().text, JOHN_3_16);
    assert_eq!(get_verse(&conn, 66, 22, 21).unwrap().text, REV_22_21);
    assert_eq!(bible_app_db::books(&conn).unwrap().len(), 66);

    let tsk = bible_app_db::resource_covering(&conn, "TSK", 43, 3, 16)
        .unwrap()
        .expect("TSK should cover John 3:16");
    assert!(!tsk.text.is_empty());
    let xrefs = bible_app_db::xrefs_from(&conn, 43, 3, 16).unwrap();
    assert!(
        !xrefs.is_empty(),
        "TSK should yield cross-references for John 3:16"
    );

    let words = bible_app_db::chapter_words(&conn, 43, 3).unwrap();
    let god = words
        .iter()
        .find(|w| w.verse == 16 && w.strongs.split_whitespace().any(|s| s == "G2316"))
        .expect("John 3:16 God should be G2316");
    let john = get_verse(&conn, 43, 3, 16).unwrap();
    assert_eq!(&john.text[god.start as usize..god.end as usize], "God");
    let theos = bible_app_db::lookup_strongs(&conn, "G2316")
        .unwrap()
        .expect("G2316 in lexicon");
    assert!(
        theos.lemma.to_lowercase().contains("theos")
            || theos.definition.to_lowercase().contains("deity")
            || theos.definition.to_lowercase().contains("god"),
        "unexpected G2316: {theos:?}"
    );
    let elohim = bible_app_db::lookup_strongs(&conn, "H430")
        .unwrap()
        .expect("H430 in lexicon");
    assert!(
        elohim.lemma.to_lowercase().contains("elohiym")
            || elohim.definition.to_lowercase().contains("god"),
        "unexpected H430: {elohim:?}"
    );

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

    let aaron = bible_app_db::search_entries(&conn, "Easton", "Aaron", 10).unwrap();
    assert!(
        aaron.iter().any(|h| h.headword == "Aaron"),
        "Easton missing Aaron: {aaron:?}"
    );
    let (_, text) = bible_app_db::get_entry(&conn, "Easton", aaron[0].i)
        .unwrap()
        .expect("Easton Aaron body");
    assert!(
        text.to_lowercase().contains("amram") || text.to_lowercase().contains("moses"),
        "unexpected Easton Aaron: {text}"
    );
    let nave = bible_app_db::search_entries(&conn, "Nave", "AARON", 10).unwrap();
    assert!(
        nave.iter()
            .any(|h| h.headword.eq_ignore_ascii_case("Aaron")),
        "Nave missing AARON: {nave:?}"
    );
}
