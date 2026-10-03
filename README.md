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
- Book and chapter dropdowns on each passage jump to a chapter; type in the popup to search
- Alt+Left / Alt+Right: previous / next chapter of the focused passage
- History back and forward sit at the left of each passage (Alt+Shift+Left / Alt+Shift+Right, or mouse back/forward). They stay visible and turn on once that passage has somewhere to go
- Ctrl+L then type `John 3:16` and Enter (Shift+Enter opens that passage beside the current one)
- Ctrl+F or the search icon in the header opens a Search tab beside the chapter, another one if Search is already open. Phrase is the default match, so `ear` finds ear, earth, hear, and heart. The same menu offers all words, any word, and exact word (the typed token only). A trailing `*` still prefixes a shorter stem such as `lo*` in Exact word. `H430` lists King James verses for that Strong's number, `John 3:16` offers Go to, and `John/light` limits the search to that book. Book chips and a range menu narrow a long hit list. The scope menu covers the KJV, commentary, dictionaries, topics, notes, and the whole library. Click a hit to open that verse in a split next to the list; later clicks reuse that pane. Right-click for Open in a new tab. Arrow keys preview a hit in the chapter beside Search. Enter stays on the hit and closes Search. Esc, while Search is the selected tab, returns to the previous passage. Shift+Enter opens a verse beside the current one
- The chapter opens as a tab. The tab bar is the top row of the window, beside the menu: closing the last tab leaves the window open. Tabs hold KJV passages and study views (Matthew Henry, TSK, library, bookmarks, notes, Strong’s occurrences, search). Each bar has New tab. Split is the icon on the tab. New tab (+) or Ctrl+T opens a blank tab, which is how an empty window gets a passage again: the book and chapter menus match a passage (type in the book menu to filter), or choose Search, Matthew Henry, the Treasury, the library, notes, or bookmarks. That choice replaces the blank tab, and opens another even if one of that kind is already open
- Split is the icon on a tab. It splits that one tab, and the window keeps a single tab bar. One split per window. Splitting a chapter shows the same chapter on the right of that tab. Splitting a study tab places it beside the chapter, inside the chapter's tab. A small close button on the right view dismisses it. The tab menu still has Open beside
- The tab menu's Open in a window moves the selected tab into another reader window. Dragging a tab out does the same, and a split tab takes the view inside it along. That window can hold one split too
- Each passage centers its book and chapter menus, with previous chapter on the left and next chapter on the right. Matthew Henry and the Treasury use that same bar and show the whole chapter, scrolled to the verse they follow. A dictionary, lexicon, or topic centers its headword search, with previous and next entry on either side. History sits at the left of each of those bars. Alt+Left / Alt+Right and Alt+Shift+Left / Alt+Shift+Right follow whichever of those tabs is focused. The menu is on the top row with the tabs. Font, paragraphs, copy, and marks apply to the focused passage tab
- Ctrl+- / Ctrl++: smaller / larger text
- Drag either edge of the chapter to set the column width. That width is remembered. The edges hide while that tab is split; drag the divider between the two views to resize them
- Copy the current verse with citation (Ctrl+C / right-click), not a header button. A chapter selection copies that verse range.
- Click a verse (number or body) to select it; MHC and TSK tabs follow that verse by default (toggle Follow verse on the tab menu). Library, notes, and occurrence lists stay put
- Click a tagged KJV word for Strong’s, then BDB (Hebrew) and/or Thayer (Greek) when that code is in the library, plus each matching dictionary (Easton, Smith, Webster, …) and topic (Nave, Torrey, …) when the headword matches. Sources stack on the left of the popover; the open source is on the right. Each source can open that word in the library. Strong’s can list every KJV verse for that number. Lemmas are not inserted beside words in the chapter.
- Translator notes appear as a dagger on the matched phrase; hover or click the mark for the note
- TSK superscripts on a matched phrase open that phrase’s destinations in a popover: references on the left, the selected verse on the right. Hover previews the heading and a short dest list. Right-click a reference to Open it in the current passage or Open in new tab
- A verse number is a link when a Matthew Henry comment starts there; click it to open a Matthew Henry tab
- Menu opens Matthew Henry, the Treasury of Scripture Knowledge, bookmarks, notes, or a lexicon, dictionary, or topic as a new tab in the current tab bar, even if one of that kind is already open. Verse clicks and Search still open a split when the window has none. Split stays the icon on a tab
- Menu → Lexicon → Brown-Driver-Briggs or Thayer; type a Strong’s code such as `H430` or `G26`
- Menu → Dictionary → Webster’s 1828, Easton, or another dictionary; type a headword in the search bar
- Menu → Topics → Nave, Torrey, and the other topical works
- Bookmarks, notes, and highlights are local, verse-anchored, and exportable as Markdown. They live in `~/.local/share/bible-app/user.sqlite`, not in the shipped library database. Select words and right-click to highlight that selection; right-click a verse to bookmark it or add a note. Add note and Edit note open a dialog on that verse so you stay in the chapter. Menu → Bookmarks and Menu → Notes each open their own tab. Click a bookmark to open that verse beside the list; right-click for a new tab or Remove. Menu → Notes puts the list on the left and the note on the right; a click opens the note. Right-click a note, or the menu on the editor, to open that verse in a split, open it in a new tab, or delete the note. Export notes is on the Notes tab.
- Last position, font size, and paragraphs are stored in `~/.config/bible-app/state.toml`
- Colors follow the desktop. On Omarchy the window and the chapter use the current theme (`~/.local/state/omarchy/current/theme/colors.toml`) and update when that theme changes. `BIBLE_APP_THEME` can point at another `colors.toml`. Elsewhere the app follows the libadwaita light/dark scheme and accent color.

The database includes **KJV**, **Matthew Henry**, the **Treasury of Scripture Knowledge**, **Strong’s**, **Brown-Driver-Briggs** (1906), **Thayer** (1889), **Webster’s 1828**, the public-domain Bible dictionaries (Easton, Smith, Hitchcock’s Names, American Tract Society), and topical works (Nave, Torrey, Spurgeon, and the rest of that set). Thayer is Unitarian and based on Westcott-Hort; Strong’s stays the TR/KJV gloss. See `data/README.md` for dump licenses (Open Scriptures BDB is CC BY 4.0).

See `data/README.md`.

## Status

- [x] Phase 0: KJV SQLite
- [x] Phase 1: GTK4 + Libadwaita passage window
- [x] Phase 2: search
- [x] Phase 3: Strong’s, TSK cross-references, Matthew Henry
- [x] Phase 4: public-domain dictionaries and topics
- [x] Chapter reader: inline TSK/MHC, KJV italics, history, font size, verse highlight
