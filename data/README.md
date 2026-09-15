# Data

`bible-app.sqlite.gz` is the shipped King James library. The reader unpacks it on
first launch to `~/.local/share/bible-app/bible-app.sqlite`. You can also gunzip
it next to this file or point `BIBLE_APP_DB` at an unpacked copy.

The database contains:

- **KJV** verse text (public domain, 1769)
- **Matthew Henry** comments keyed to KJV verses
- **Treasury of Scripture Knowledge** notes and cross-references
- **Strong’s** 1890 Hebrew/Greek definitions mapped onto KJV words
- **Easton**, **Smith**, **Hitchcock’s Names**, and the **American Tract Society** dictionaries
- Topical works (Nave, Torrey, Daily Light, Spurgeon, Josephus, and the rest of that set)
- An FTS5 index of KJV verses (older unpacked copies are backfilled on open)
