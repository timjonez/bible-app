# bible-app

A native Linux reader for the King James Version plus public-domain study texts.

The GUI reads a SQLite database this project produces. It never opens a Power Bible CD directory at runtime.

## Import (once)

The CD dump is only a source for `bible-app-import`:

```bash
cargo run -p bible-app-import -- \
  --from /home/tim/BibleCD \
  --out data/bible-app.sqlite
```

Phase 0 imports **KJV only**. Other public-domain modules (commentaries, Strong’s, TSK, …) come in later phases. Copyrighted Bibles and modules are skipped.

See `data/README.md`.

## Status

- [x] Phase 0: import KJV → SQLite
- [ ] Phase 1: GTK4 + Libadwaita passage window
- [ ] Phase 2: search
- [ ] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [ ] Phase 4: remaining public-domain dictionaries and topics
