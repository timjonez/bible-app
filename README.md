# bible-app

A native Linux reader for the King James Version plus public-domain study texts.

The GUI reads the shipped SQLite database. On first launch it unpacks
`data/bible-app.sqlite.gz` to `~/.local/share/bible-app/bible-app.sqlite`.

## Run the reader

```bash
cargo run -p bible-app
```

Override the database path with `BIBLE_APP_DB` if you already have an unpacked copy.

- Paragraphs (default) flows consecutive verses as prose; toggle off for one verse per block. Alt+Left / Alt+Right still change chapter.
- Alt+Left / Alt+Right: previous / next chapter
- Back / Forward: jump through navigation history
- Ctrl+L then type `John 3:16` and Enter
- Ctrl+F to search the KJV; Enter on a result jumps to that verse
- Ctrl+- / Ctrl++: smaller / larger text
- Click a verse (number or body) to select it; copy, MHC, and TSK follow that verse
- Copy copies the current verse plus a `Book chapter:verse (KJV)` citation
- Click a tagged KJV word for its Strong’s lemma, pronunciation, number, and definition. Lemmas are not inserted beside words in the chapter.
- Translator notes appear as a dagger on the matched phrase; hover or click the mark for the note
- TSK superscripts on a matched phrase open that phrase’s destinations; hover previews the heading and a short dest list
- A small M at the end of a verse (when a Matthew Henry comment starts there) opens Matthew Henry
- MHC in the header opens Matthew Henry for the current verse in a second window
- TSK opens the Treasury of Scripture Knowledge; click a cross-reference to jump
- Dict opens Easton and the other dictionaries and topics; type a headword and click a result
- Last position, font size, and paragraphs are stored in `~/.config/bible-app/state.toml`

The database includes **KJV**, **Matthew Henry**, the **Treasury of Scripture Knowledge**, **Strong’s**, the public-domain dictionaries (Easton, Smith, Hitchcock’s Names, American Tract Society), and topical works (Nave, Torrey, Spurgeon, and the rest of that set).

See `data/README.md`.

## Status

- [x] Phase 0: KJV SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [x] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [x] Phase 4: public-domain dictionaries and topics
- [x] Chapter reader: inline TSK/MHC, KJV italics, history, copy, font size, verse highlight
