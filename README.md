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
- Ctrl+L then type `John 3:16` and Enter (Shift+Enter opens that passage beside the current one)
- Ctrl+F searches the KJV by default without leaving the chapter; a scope menu in the overlay also searches commentary (MHC/TSK), dictionaries, and topics. Enter on a KJV hit jumps to that verse in the focused passage; Shift+Enter opens it beside. Commentary opens the MHC or TSK tab; dictionary and topic hits open the library tab
- Tabs hold KJV passages and study views (Matthew Henry, TSK, library, bookmarks/notes, Strong’s occurrences). The tab bar stays hidden until a second tab or a split is open
- Split (one at a time) shows two views side by side — two passages, or a passage plus MHC. Open beside from a TSK destination, a search hit, Ctrl+L, or the tab menu
- Detach a tab into its own window from the tab menu (Open in a window), or drag a tab out
- Header book/chapter pickers, history, font, paragraphs, copy, and marks apply to the focused passage tab
- Ctrl+- / Ctrl++: smaller / larger text (also in the menu)
- Copy the current verse with citation (Ctrl+C / menu / right-click), not a header button. A chapter selection copies that verse range.
- Click a verse (number or body) to select it; MHC and TSK tabs follow that verse by default (toggle Follow verse on the tab menu). Library, notes, and occurrence lists stay put
- Click a tagged KJV word for Strong’s plus a tab per matching dictionary (Easton, Smith, Webster, …) and topic (Nave, Torrey, …) when the headword matches. Each tab can open that word in the library. The Strong’s tab can list every KJV verse for that number. Lemmas are not inserted beside words in the chapter.
- Translator notes appear as a dagger on the matched phrase; hover or click the mark for the note
- TSK superscripts on a matched phrase open that phrase’s destinations; hover previews the heading and a short dest list. Destinations can jump in the current passage or Open beside
- A verse number is a link when a Matthew Henry comment starts there; click it to open the Matthew Henry tab
- Menu → Commentary opens Matthew Henry or the Treasury of Scripture Knowledge for the current verse as a tab
- Menu → Dictionary → Webster’s 1828, Easton, or another dictionary; type a headword in the search bar
- Menu → Topics → Nave, Torrey, and the other topical works
- Bookmarks, notes, and highlights are local, verse-anchored, and exportable as Markdown. They live in `~/.local/share/bible-app/user.sqlite`, not in the shipped library database. Right-click a verse to bookmark, highlight, or add a note; Menu → Bookmarks / Notes / Export notes…
- Last position, font size, and paragraphs are stored in `~/.config/bible-app/state.toml`

The database includes **KJV**, **Matthew Henry**, the **Treasury of Scripture Knowledge**, **Strong’s**, **Webster’s 1828**, the public-domain Bible dictionaries (Easton, Smith, Hitchcock’s Names, American Tract Society), and topical works (Nave, Torrey, Spurgeon, and the rest of that set).

See `data/README.md`.

## Status

- [x] Phase 0: KJV SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [x] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [x] Phase 4: public-domain dictionaries and topics
- [x] Chapter reader: inline TSK/MHC, KJV italics, history, font size, verse highlight
