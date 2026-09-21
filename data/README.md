# Data

`bible-app.sqlite.gz` is the shipped King James library. The reader unpacks it on
first launch to `~/.local/share/bible-app/bible-app.sqlite`. You can also gunzip
it next to this file or point `BIBLE_APP_DB` at an unpacked copy.

The database contains:

- **KJV** verse text (public domain, 1769)
- **Matthew Henry** comments keyed to KJV verses
- **Treasury of Scripture Knowledge** notes and cross-references
- **Strong’s** 1890 Hebrew/Greek definitions mapped onto KJV words
- **Brown-Driver-Briggs** 1906 Hebrew lexicon, keyed to Strong’s numbers. The 1906 wording is public domain. The structured dump is the [Open Scriptures Hebrew Lexicon](https://github.com/openscriptures/HebrewLexicon) (**CC BY 4.0** — markup and Strong’s mapping, not the 1906 book). Credit the Open Scriptures Hebrew Bible Project. Do not treat that dump as public domain.
- **Thayer’s** 1889 Greek-English lexicon, keyed to Strong’s numbers. The 1889 wording is public domain. The dump is [nigelmsipa/thayers-greek-lexicon-dataset](https://github.com/nigelmsipa/thayers-greek-lexicon-dataset) (**CC0 1.0**). Unmapped lemmas are joined to Strong’s numbers with [morphgnt/strongs-dictionary-xml](https://github.com/morphgnt/strongs-dictionary-xml) (**CC0**). Thayer is Unitarian and Westcott-Hort-based; Strong’s remains the TR/KJV gloss.
- **Webster’s 1828** English dictionary (Noah Webster; dump from [DataWar/1828-dictionary](https://github.com/DataWar/1828-dictionary), MIT)
- **Easton**, **Smith**, **Hitchcock’s Names**, and the **American Tract Society** dictionaries
- Topical works (Nave, Torrey, Daily Light, Spurgeon, Josephus, and the rest of that set)
- An FTS5 index of KJV verses (older unpacked copies are backfilled on open)

To rebuild the Webster module from the DataWar SQL dump:

```bash
gunzip -k data/bible-app.sqlite.gz   # if you do not already have data/bible-app.sqlite
python3 data/import-webster1828.py --sql /path/to/dictionary_webster1828.sql --db data/bible-app.sqlite
gzip -9 -c data/bible-app.sqlite > data/bible-app.sqlite.gz
```

To rebuild the BDB and Thayer lexicon modules:

```bash
gunzip -k data/bible-app.sqlite.gz   # if you do not already have data/bible-app.sqlite
python3 data/import-bdb-thayer.py --db data/bible-app.sqlite
gzip -9 -c data/bible-app.sqlite > data/bible-app.sqlite.gz
```

The importer stores one `entries` row per Strong’s code (`H430`, `G26`) under modules `BDB` and `Thayer` (`kind=lexicon`). HTML is stripped to plain text. BDB comes from Open Scriptures `BrownDriverBriggs.xml` joined through `LexicalIndex.xml`. Thayer comes from the CC0 JSONL; leftover lemmas are matched to Strong’s via the MorphGNT Greek dictionary.
