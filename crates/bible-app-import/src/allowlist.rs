//! What we will and will not import from a Power Bible CD dump.
//!
//! Bibles: KJV only.
//! Other modules: public-domain allowlist; everything else is skipped.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bible,
    Commentary,
    Dictionary,
    Topic,
    Strongs,
    Xref,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Import in this phase (KJV text).
    Import,
    /// Public domain and wanted later; do not open the files yet.
    Defer,
    /// Do not import. `reason` is logged.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classification {
    pub decision: Decision,
    pub kind: Kind,
    pub reason: &'static str,
}

pub fn classify(stem: &str) -> Classification {
    match normalize(stem).as_str() {
        // Bible — only KJV
        "KJV" => imp(Kind::Bible, "King James Version (public domain)"),

        "ASV" | "DBY" | "YLT" | "WEB" | "WNT" | "MNT" | "TCNT" | "ORACL" | "TR" | "LGB" | "LSV"
        | "SRV" | "SRV1865" | "SRV18" | "RST" => {
            skip(Kind::Bible, "public-domain Bible, but this app is KJV-only")
        }

        "NIV" | "NASB" | "NAS95" | "NRSV" | "NKJV" | "RSV" | "MKJV" | "BBE" | "FILIPINO"
        | "FILIP" | "RVG04" | "SRVA" => skip(Kind::Bible, "not public domain"),

        // Commentaries — MHC this phase; the rest later
        "MHC" => imp(
            Kind::Commentary,
            "Matthew Henry's Commentary (public domain)",
        ),
        "TSK" => imp(
            Kind::Commentary,
            "Treasury of Scripture Knowledge (public domain)",
        ),
        "ACC" | "BARNES" | "DODDRIDGE" | "GBN" | "HALLS" | "HAWEIS" | "JFB" | "JWN" | "MHCC"
        | "MACKNIGHT" | "PLWL" | "PNTC" | "PNT" | "POOLE" | "SCM" | "SDC" | "SCOTT" | "TFG"
        | "TOD" | "WBN" => defer(Kind::Commentary, "public-domain commentary (later phase)"),

        "RWP" => skip(
            Kind::Commentary,
            "Robertson Word Pictures: later volumes not public domain",
        ),
        "ANNOTATED" | "BFB" | "BRETHREN" | "FBN" | "RIPLEY" | "TEACHER" => {
            skip(Kind::Commentary, "edition/rights unclear")
        }

        "EASTON" => imp(
            Kind::Dictionary,
            "Easton's Bible Dictionary (public domain)",
        ),
        "SMITH" => imp(Kind::Dictionary, "Smith's Bible Dictionary (public domain)"),
        "NAMES" => imp(Kind::Dictionary, "Hitchcock's Bible Names (public domain)"),
        "ATSD" => imp(
            Kind::Dictionary,
            "American Tract Society Dictionary (public domain)",
        ),
        "ISBE" | "PROTESTANT" => skip(Kind::Dictionary, "edition not confirmed public domain"),
        "PBAUTHORS" => skip(Kind::Other, "product index, not a public-domain work"),

        // Topics
        "NAVE" => imp(Kind::Topic, "Nave's Topical Bible (public domain)"),
        "TORREY" => imp(Kind::Topic, "Torrey's Topical Textbook (public domain)"),
        "BAGSTER" => imp(Kind::Topic, "Bagster's Daily Light (public domain)"),
        "CARYL" => imp(Kind::Topic, "Joseph Caryl quotations (public domain)"),
        "CONTEMP" => imp(Kind::Topic, "Bishop Hall's Contemplations (public domain)"),
        "HORNE" => imp(Kind::Topic, "Horne on the Psalms (public domain)"),
        "STACKHOUSE" => imp(Kind::Topic, "Stackhouse Body of Divinity (public domain)"),
        "TILLOTSON" => imp(Kind::Topic, "Tillotson Attributes of God (public domain)"),
        "MER" => imp(Kind::Topic, "Spurgeon Morning and Evening (public domain)"),
        "700SM" => imp(Kind::Topic, "Spurgeon sermons (public domain)"),
        "POPERY" => imp(Kind::Topic, "Nevins Thoughts on Popery (public domain)"),
        "JOSEPHUS" => imp(Kind::Topic, "Josephus (public domain)"),
        "CHAIN" => skip(Kind::Topic, "Thompson Chain: still commercial"),
        "ELR" | "SERMONS" | "SERMONII" | "QUOTES" | "HISTORYUS" | "WILSON" | "FUNERALS"
        | "GOSPEL" | "CHRIST" | "CHRISTO" | "CHURCHSTAT" | "HURST" | "MESSIAH" | "PRAYERS"
        | "TRAVELL" | "TRINITY" | "WALDENSES" => skip(Kind::Topic, "compilation/edition unclear"),

        "STRGRK" => imp(
            Kind::Strongs,
            "Strong's Greek dictionary (public domain, 1890)",
        ),
        "STRHEB" => imp(
            Kind::Strongs,
            "Strong's Hebrew dictionary (public domain, 1890)",
        ),
        "STRONGS" => imp(Kind::Strongs, "Strong's lemmas (public domain, 1890)"),
        "BCDXREFS" => skip(Kind::Xref, "Power Bible xref database; use TSK instead"),

        _ => skip(Kind::Other, "not on the allowlist"),
    }
}

fn normalize(stem: &str) -> String {
    stem.trim().to_ascii_uppercase()
}

fn imp(kind: Kind, reason: &'static str) -> Classification {
    Classification {
        decision: Decision::Import,
        kind,
        reason,
    }
}

fn defer(kind: Kind, reason: &'static str) -> Classification {
    Classification {
        decision: Decision::Defer,
        kind,
        reason,
    }
}

fn skip(kind: Kind, reason: &'static str) -> Classification {
    Classification {
        decision: Decision::Skip,
        kind,
        reason,
    }
}

/// Module stems we consider when scanning a dump (`KJV` from `KJV.bt0`).
pub fn module_stem(file_name: &str) -> Option<&str> {
    const SUFFIXES: [&str; 9] = [
        ".bt0", ".ct0", ".tt0", ".dt0", ".gx0", ".hx0", ".xr0", ".sd0", ".ucm",
    ];
    for suf in SUFFIXES {
        if let Some(stem) = file_name
            .strip_suffix(suf)
            .or_else(|| file_name.strip_suffix(&suf.to_ascii_uppercase()))
        {
            if !stem.is_empty() {
                return Some(stem);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kjv_is_imported() {
        let c = classify("KJV");
        assert_eq!(c.decision, Decision::Import);
        assert_eq!(c.kind, Kind::Bible);
    }

    #[test]
    fn other_bibles_skipped_even_if_pd() {
        for id in ["WEB", "ASV", "YLT", "DBY"] {
            let c = classify(id);
            assert_eq!(c.decision, Decision::Skip, "{id}");
            assert_eq!(c.kind, Kind::Bible, "{id}");
        }
    }

    #[test]
    fn copyrighted_bibles_skipped() {
        for id in ["NIV", "NASB", "NAS95", "NRSV", "NKJV", "RSV", "MKJV", "BBE"] {
            let c = classify(id);
            assert_eq!(c.decision, Decision::Skip, "{id}");
        }
    }

    #[test]
    fn mhc_imported() {
        let c = classify("MHC");
        assert_eq!(c.decision, Decision::Import);
        assert_eq!(c.kind, Kind::Commentary);
    }

    #[test]
    fn tsk_imported() {
        let c = classify("TSK");
        assert_eq!(c.decision, Decision::Import);
        assert_eq!(c.kind, Kind::Commentary);
    }

    #[test]
    fn chain_skipped() {
        assert_eq!(classify("Chain").decision, Decision::Skip);
    }

    #[test]
    fn bcdxrefs_skipped() {
        let c = classify("bcdxrefs");
        assert_eq!(c.decision, Decision::Skip);
        assert_eq!(c.kind, Kind::Xref);
    }

    #[test]
    fn topics_imported() {
        for id in ["Nave", "Torrey", "Bagster", "700Sm", "Josephus"] {
            let c = classify(id);
            assert_eq!(c.decision, Decision::Import, "{id}");
            assert_eq!(c.kind, Kind::Topic, "{id}");
        }
    }

    #[test]
    fn dictionaries_imported() {
        for id in ["Easton", "Smith", "Names", "ATSD"] {
            let c = classify(id);
            assert_eq!(c.decision, Decision::Import, "{id}");
            assert_eq!(c.kind, Kind::Dictionary, "{id}");
        }
    }

    #[test]
    fn strongs_imported() {
        for id in ["StrGrk", "StrHeb", "Strongs"] {
            let c = classify(id);
            assert_eq!(c.decision, Decision::Import, "{id}");
            assert_eq!(c.kind, Kind::Strongs, "{id}");
        }
    }

    #[test]
    fn stem_from_filename() {
        assert_eq!(module_stem("KJV.bt0"), Some("KJV"));
        assert_eq!(module_stem("MHC.ct0"), Some("MHC"));
        assert_eq!(module_stem("Nave.tt0"), Some("Nave"));
        assert_eq!(module_stem("KJV.bt4"), None);
        assert_eq!(module_stem("BibleCD.exe"), None);
    }
}
