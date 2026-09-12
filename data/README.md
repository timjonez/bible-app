# Data

`bible-app.sqlite` is generated locally and is not committed.

```bash
cargo run -p bible-app-import -- --from /path/to/source-dump --out data/bible-app.sqlite
```

The importer:

- Copies **KJV** verse text into SQLite (public domain, 1769).
- Skips every other Bible (this app is KJV-only).
- Skips works that are not public domain.
- Defers allowlisted commentaries/dictionaries/topics until later phases.

Point the GUI at this file with `BIBLE_APP_DB` or the default XDG path.

The importer also builds an FTS5 index of KJV verses. Opening an older database backfills that index.
