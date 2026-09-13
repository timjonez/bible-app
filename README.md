# bible-app

A native Linux reader for the King James Version plus public-domain study texts.

The GUI reads a SQLite database this project produces. It never opens a source directory at runtime.

## Import (once)

A one-off importer builds the SQLite library:

```bash
cargo run -p bible-app-import -- \
  --from /home/tim/source-dump \
  --out data/bible-app.sqlite
```

The importer brings in **KJV**, **Matthew Henry**, the **Treasury of Scripture Knowledge**, **Strong’s**, the public-domain dictionaries (Easton, Smith, Hitchcock’s Names, American Tract Society), and topical works (Nave, Torrey, Spurgeon, and the rest of the allowlist). Remaining commentaries stay deferred. Copyrighted Bibles and modules are skipped.

See `data/README.md`.

## Run the reader

```bash
export BIBLE_APP_DB=data/bible-app.sqlite   # or ~/.local/share/bible-app/bible-app.sqlite
cargo run -p bible-app
```

- Alt+Left / Alt+Right: previous / next chapter
- Ctrl+L then type `John 3:16` and Enter
- Ctrl+F to search the KJV; Enter on a result jumps to that verse
- MHC opens Matthew Henry for the current verse in a second window
- TSK opens the Treasury of Scripture Knowledge; click a cross-reference to jump
- Click an underlined KJV word for its Strong’s number, lemma, and definition
- Dict opens Easton and the other dictionaries and topics; type a headword and click a result
- Last position is stored in `~/.config/bible-app/state.toml`

## Status

- [x] Phase 0: import KJV → SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [x] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [x] Phase 4: public-domain dictionaries and topics
