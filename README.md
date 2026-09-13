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

The importer brings in **KJV** and **Matthew Henry**. Other public-domain modules (Strong’s, TSK, more commentaries, …) come in later phases. Copyrighted Bibles and modules are skipped.

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
- Last position is stored in `~/.config/bible-app/state.toml`

## Status

- [x] Phase 0: import KJV → SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [ ] Phase 3: Strong’s, TSK cross-references (Matthew Henry window is in)
- [ ] Phase 4: remaining public-domain dictionaries and topics
