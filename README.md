# bible-app

A native Linux reader for the King James Version plus public-domain study texts.

The GUI reads the shipped SQLite database. On first launch it unpacks
`data/bible-app.sqlite.gz` to `~/.local/share/bible-app/bible-app.sqlite`.

## Run the reader

```bash
cargo run -p bible-app
```

Override the database path with `BIBLE_APP_DB` if you already have an unpacked copy.

- Paragraphs (default) flows consecutive verses as prose; turn it off from the menu for one verse per block. Alt+Left / Alt+Right still change chapter.
- Header book and chapter dropdowns jump to a passage; type in the popup to search
- Alt+Left / Alt+Right: previous / next chapter
- History back / forward (header icons, Alt+Shift+Left / Alt+Shift+Right, or mouse back/forward)
- Ctrl+L then type `John 3:16` and Enter
- Ctrl+F searches the KJV by default without leaving the chapter; a scope menu in the overlay also searches commentary (MHC/TSK), dictionaries, and topics. Enter on a KJV hit jumps to that verse; commentary opens MHC or TSK; dictionary and topic hits open the library window
- Ctrl+- / Ctrl++: smaller / larger text (also in the menu)
- Copy the current verse with citation (Ctrl+C / menu / right-click), not a header button. A chapter selection copies that verse range.
- Click a verse (number or body) to select it; MHC and TSK follow that verse
- Click a tagged KJV word for Strong’s first, then BDB (Hebrew) and/or Thayer (Greek) when that code is in the library, plus a tab per matching dictionary (Easton, Smith, Webster, …). Each tab can open that word in the library window. The Strong’s tab can list every KJV verse for that number. Lemmas are not inserted beside words in the chapter.
- Translator notes appear as a dagger on the matched phrase; hover or click the mark for the note
- TSK superscripts on a matched phrase open that phrase’s destinations; hover previews the heading and a short dest list
- A verse number is a link when a Matthew Henry comment starts there; click it to open Matthew Henry
- Menu → Commentary opens Matthew Henry or the Treasury of Scripture Knowledge for the current verse in a second window
- Menu → Lexicon → Brown-Driver-Briggs or Thayer; type a Strong’s code such as `H430` or `G26`
- Menu → Dictionary → Webster’s 1828, Easton, or another dictionary; type a headword in the search bar
- Menu → Topics → Nave, Torrey, and the other topical works
- Bookmarks, notes, and highlights are local, verse-anchored, and exportable as Markdown. They live in `~/.local/share/bible-app/user.sqlite`, not in the shipped library database. Right-click a verse to bookmark, highlight, or add a note; Menu → Bookmarks / Notes / Export notes…
- Last position, font size, and paragraphs are stored in `~/.config/bible-app/state.toml`

The database includes **KJV**, **Matthew Henry**, the **Treasury of Scripture Knowledge**, **Strong’s**, **Brown-Driver-Briggs** (1906), **Thayer** (1889), **Webster’s 1828**, the public-domain Bible dictionaries (Easton, Smith, Hitchcock’s Names, American Tract Society), and topical works (Nave, Torrey, Spurgeon, and the rest of that set). Thayer is Unitarian and based on Westcott-Hort; Strong’s stays the TR/KJV gloss. See `data/README.md` for dump licenses (Open Scriptures BDB is CC BY 4.0).

See `data/README.md`.

## Status

- [x] Phase 0: KJV SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [x] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [x] Phase 4: public-domain dictionaries and topics
- [x] Chapter reader: inline TSK/MHC, KJV italics, history, font size, verse highlight
