# Data

`bible-app.sqlite.gz` is the shipped King James library. The reader unpacks it on
first launch to `~/.local/share/bible-app/bible-app.sqlite`. You can also gunzip
it next to this file or point `BIBLE_APP_DB` at an unpacked copy.

The database contains:

- **KJV** verse text (public domain, 1769)
- **Matthew Henry** comments keyed to KJV verses
- **Treasury of Scripture Knowledge** notes and cross-references
- **Strong’s** 1890 Hebrew/Greek definitions mapped onto KJV words
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
