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

The importer brings in **KJV**, **Matthew Henry**, the **Treasury of Scripture Knowledge**, **Strong’s**, the public-domain dictionaries (Easton, Smith, Hitchcock’s Names, American Tract Society), and topical works (Nave, Torrey, Spurgeon, and the rest of the allowlist). Remaining commentaries stay deferred. Copyrighted Bibles and modules are skipped.

See `data/README.md`.

## Run the reader

```bash
export BIBLE_APP_DB=data/bible-app.sqlite   # or ~/.local/share/bible-app/bible-app.sqlite
cargo run -p bible-app
```

- Alt+Left / Alt+Right: previous / next chapter
- Back / Forward: jump through navigation history
- Ctrl+L then type `John 3:16` and Enter
- Ctrl+F to search the KJV; Enter on a result jumps to that verse
- Ctrl+- / Ctrl++: smaller / larger text
- Copy copies the current verse plus a `Book chapter:verse (KJV)` citation
- Interlinear shows each tagged word’s Strong’s lemma beside the English
- Cross-references under a verse jump on click and preview the destination on hover
- MHC under a verse (when Matthew Henry covers it) opens Matthew Henry
- MHC in the header opens Matthew Henry for the current verse in a second window
- TSK opens the Treasury of Scripture Knowledge; click a cross-reference to jump
- Click an underlined KJV word for its Strong’s number, lemma, and definition
- Dict opens Easton and the other dictionaries and topics; type a headword and click a result
- Last position, font size, and interlinear are stored in `~/.config/bible-app/state.toml`

## Status

- [x] Phase 0: import KJV → SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [x] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [x] Phase 4: public-domain dictionaries and topics
- [x] Chapter reader: inline TSK/MHC, KJV italics, history, copy, font size, interlinear
