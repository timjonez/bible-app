# bible-app

A native Linux reader for the King James Bible, with a local library of public-domain commentary, lexicons, dictionaries, and topical works.

It is a GTK 4 / libadwaita desktop app. The whole library lives in a shipped SQLite file, so reading and study stay offline after the first unpack.

## What it has

The database includes:

- **King James Version** (1769), with italics for supplied words and translator notes on the verse
- **Matthew Henry’s Commentary**, keyed to KJV verses
- **Treasury of Scripture Knowledge** cross-references, keyed to phrases in the verse
- **Strong’s** Hebrew and Greek numbers on KJV words
- **Brown-Driver-Briggs** (1906) Hebrew lexicon and **Thayer** (1889) Greek lexicon, both keyed to Strong’s codes
- **Webster’s 1828** English dictionary
- Bible dictionaries: **Easton**, **Smith**, **Hitchcock’s Names**, **American Tract Society**
- Topical works: **Nave**, **Torrey**, **Spurgeon**, **Daily Light**, **Josephus**, and the rest of that set

Licenses for the dumps (Open Scriptures BDB is CC BY 4.0) are in [`data/README.md`](data/README.md).

## What it can do

**Read a chapter.** Book and chapter menus jump anywhere; type in the book popup to filter. Consecutive verses flow as prose by default, or one verse per block from the menu. Drag either edge of the chapter to set the column width. Font size, last position, and paragraph mode are remembered.

**Open several passages at once.** Chapters and study views live in tabs. A window has one side-by-side split: the chapter stays on the left, and Search, Henry, the Treasury, and the library share the right-hand pane. Drag a tab out, or use Open in a window, for a second reader window.

**Look a word up in place.** Click a tagged KJV word for Strong’s, then BDB or Thayer when that code is in the library, plus matching dictionary and topic articles. Sources sit on the left of the popover; the open article is on the right. See all opens Search for every KJV verse tagged with that number, so a hit can be opened beside the list or in a new tab. Scripture references in those articles are links and jump in the current passage.

**Follow cross-references and commentary.** A TSK superscript on a phrase opens its destinations: references on the left, the selected verse on the right. Right-click a verse and choose Matthew Henry when commentary starts there; Henry then opens on the right of the chapter, sharing that pane with Search, the Treasury, and the library. Pin a split Henry or Treasury pane to follow the chapter; a standalone commentary tab walks Henry or the Treasury on its own. Bible references in Matthew Henry, the Treasury, and library articles are links, including Thayer’s roman-numeral chapters (`Mt. ii. 4`); click one to open that verse beside the study tab, or right-click for a new tab or window. Translator notes appear as a dagger on the matched phrase.

**Search the library.** Contains match is the default, so `ear` finds *ear*, *earth*, *hear*, and *heart*. The same menu offers all words, any word, and exact word. Scope covers the KJV, commentary, dictionaries, topics, notes, or the whole library. `H430` lists verses for that Strong’s number; `John 3:16` offers Go to; `John/light` limits the search to that book.

**Keep local marks.** Right-click a verse to bookmark it or add a note. Select words and highlight them. A bookmark underlines the verse number; a note washes it. Bookmarks, notes, and highlights are verse-anchored, stored separately from the library, and exportable as Markdown.

**Follow the desktop theme.** On Omarchy the window tracks the current theme. Elsewhere it follows the libadwaita light/dark scheme and accent color.

## Run

```bash
cargo run -p bible-app
```

Needs a C toolchain plus the GTK 4 and libadwaita development libraries. On Debian/Ubuntu:

```bash
sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

On first launch the app unpacks `data/bible-app.sqlite.gz` to `~/.local/share/bible-app/bible-app.sqlite`. Point `BIBLE_APP_DB` at an unpacked copy to skip that.

## Shortcuts

| Keys | Action |
| --- | --- |
| Ctrl+L | Go to a reference (`John 3:16`); Shift+Enter opens it beside |
| Ctrl+F | Search |
| Ctrl+T | New tab |
| Alt+Left / Alt+Right | Previous / next chapter (or dictionary entry) |
| Alt+Shift+Left / Alt+Shift+Right | History back / forward |
| Ctrl+- / Ctrl++ | Smaller / larger text |
| Ctrl+C | Copy the selection, or the current verse, with citation |
| Ctrl+D | Bookmark |
| Ctrl+Shift+N | Add/Edit note |

Mouse back/forward buttons also walk history. In Search, arrow keys preview a hit, Enter opens it beside Search and keeps the list, Esc returns to the previous passage.

## Files

| Path | Contents |
| --- | --- |
| `~/.local/share/bible-app/bible-app.sqlite` | Unpacked library |
| `~/.local/share/bible-app/user.sqlite` | Bookmarks, notes, highlights |
| `~/.config/bible-app/state.toml` | Last position, font size, paragraphs |
| `BIBLE_APP_THEME` | Optional path to an Omarchy-style `colors.toml` |
