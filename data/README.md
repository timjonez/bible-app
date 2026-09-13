# Data

`bible-app.sqlite` is generated locally and is not committed.

```bash
cargo run -p bible-app-import -- --from /path/to/PowerBibleCD --out data/bible-app.sqlite
```

The importer:

- Copies **KJV** verse text into SQLite (public domain, 1769).
- Copies **Matthew Henry** comments keyed to KJV verses.
- Copies **Treasury of Scripture Knowledge** notes and builds cross-references from TSK (not Power Bible’s xref database).
- Copies **Strong’s** 1890 Hebrew/Greek definitions and maps them onto KJV words (`KJV.bt8`).
- Copies **Easton**, **Smith**, **Hitchcock’s Names**, and the **American Tract Society** dictionaries as headword entries.
- Skips every other Bible (this app is KJV-only).
- Skips works that are not public domain.
- Defers remaining allowlisted commentaries and topics until later phases.

Point the GUI at this file with `BIBLE_APP_DB` or the default XDG path.

The importer also builds an FTS5 index of KJV verses. Opening an older database backfills that index.
