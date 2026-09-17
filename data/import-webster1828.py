#!/usr/bin/env python3
"""Load Webster 1828 from the DataWar MIT SQL dump into bible-app.sqlite.

Source: https://github.com/DataWar/1828-dictionary
  v2015/SQL/02-database-insert/dictionary_webster1828.sql

The 1828 wording is public domain. This dump is MIT-licensed.
HTML is stripped to plain text to match Easton and the other modules.
"""

from __future__ import annotations

import argparse
import html as htmlmod
import re
import sqlite3
import sys
import urllib.request
from html.parser import HTMLParser
from pathlib import Path

DUMP_URL = (
    "https://raw.githubusercontent.com/DataWar/1828-dictionary/main/"
    "v2015/SQL/02-database-insert/dictionary_webster1828.sql"
)
MODULE_ID = "Webster"
MODULE_TITLE = "Webster's 1828 Dictionary"
MODULE_LICENSE = "MIT"

SKIP_HEADING = re.compile(r"did you mean", re.I)
SKIP_CONTENT = re.compile(r"searchresults|id=['\"]SearchResults", re.I)


class TextExtractor(HTMLParser):
    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.parts: list[str] = []
        self.skip = 0

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag in {"p", "div", "br", "dd", "dt", "li", "tr", "h1", "h2", "h3", "blockquote"}:
            self.parts.append("\n")
        elif tag == "script":
            self.skip += 1

    def handle_endtag(self, tag: str) -> None:
        if tag == "script" and self.skip:
            self.skip -= 1
        if tag in {"p", "div", "dd", "dt", "li", "tr"}:
            self.parts.append("\n")

    def handle_data(self, data: str) -> None:
        if not self.skip:
            self.parts.append(data)


def html_to_text(raw: str) -> str:
    parser = TextExtractor()
    try:
        parser.feed(raw)
        parser.close()
        text = "".join(parser.parts)
    except Exception:
        text = re.sub(r"<[^>]+>", "", raw)
    text = htmlmod.unescape(text).replace("\xa0", " ")
    text = re.sub(r"[ \t]+\n", "\n", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    text = re.sub(r"[ \t]{2,}", " ", text)
    return text.strip()


def sql_unescape(s: str) -> str:
    out: list[str] = []
    i = 0
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s):
            nxt = s[i + 1]
            out.append(
                {"n": "\n", "r": "\r", "t": "\t", "0": "\0"}.get(nxt, nxt)
            )
            i += 2
        else:
            out.append(s[i])
            i += 1
    return "".join(out)


def parse_rows(sql: str):
    """Yield (id, word, heading, content) from phpMyAdmin INSERT dumps."""
    n = len(sql)
    i = 0
    while True:
        idx = sql.find("VALUES", i)
        if idx < 0:
            return
        i = idx + 6
        while i < n:
            while i < n and sql[i] in " \t\r\n,;":
                i += 1
            if i >= n:
                break
            if sql.startswith("INSERT", i):
                break
            if sql[i] == "-":
                break
            if sql[i] != "(":
                break
            i += 1
            fields: list[str] = []
            while i < n:
                while i < n and sql[i] in " \t\r\n":
                    i += 1
                if i >= n:
                    break
                if sql[i] == "'":
                    i += 1
                    buf: list[str] = []
                    while i < n:
                        ch = sql[i]
                        if ch == "\\" and i + 1 < n:
                            buf.append(ch)
                            buf.append(sql[i + 1])
                            i += 2
                            continue
                        if ch == "'":
                            i += 1
                            break
                        buf.append(ch)
                        i += 1
                    fields.append(sql_unescape("".join(buf)))
                else:
                    j = i
                    while i < n and sql[i] not in ",)":
                        i += 1
                    fields.append(sql[j:i].strip())
                while i < n and sql[i] in " \t\r\n":
                    i += 1
                if i < n and sql[i] == ",":
                    i += 1
                    continue
                if i < n and sql[i] == ")":
                    i += 1
                    break
            if len(fields) >= 7:
                yield fields[0], fields[1], fields[5], fields[6]


def keep_entry(heading: str, content: str) -> bool:
    if SKIP_HEADING.search(heading):
        return False
    if SKIP_CONTENT.search(content):
        return False
    return True


def load_sql(path: Path | None) -> str:
    if path is not None:
        return path.read_text(encoding="latin-1")
    print(f"downloading {DUMP_URL}", file=sys.stderr)
    with urllib.request.urlopen(DUMP_URL) as resp:
        return resp.read().decode("latin-1")


def collect_entries(sql: str) -> list[tuple[str, str]]:
    seen: set[tuple[str, str]] = set()
    out: list[tuple[str, str]] = []
    skipped = 0
    for _id, word, heading, content in parse_rows(sql):
        if not keep_entry(heading, content):
            skipped += 1
            continue
        text = html_to_text(content)
        if len(text) < 8:
            skipped += 1
            continue
        head = heading.strip() or word.strip()
        if not head:
            skipped += 1
            continue
        key = (head.casefold(), text)
        if key in seen:
            skipped += 1
            continue
        seen.add(key)
        out.append((head, text))
    print(f"kept {len(out)} entries, skipped {skipped}", file=sys.stderr)
    return out


def write_sqlite(db: Path, entries: list[tuple[str, str]]) -> None:
    conn = sqlite3.connect(db)
    try:
        conn.execute("BEGIN")
        conn.execute("DELETE FROM entries WHERE module = ?", (MODULE_ID,))
        conn.execute("DELETE FROM modules WHERE id = ?", (MODULE_ID,))
        conn.execute(
            "INSERT INTO modules (id, kind, title, license) VALUES (?, 'dictionary', ?, ?)",
            (MODULE_ID, MODULE_TITLE, MODULE_LICENSE),
        )
        conn.executemany(
            "INSERT INTO entries (module, i, headword, text) VALUES (?, ?, ?, ?)",
            ((MODULE_ID, i, head, text) for i, (head, text) in enumerate(entries)),
        )
        conn.commit()
        conn.execute("VACUUM")
    finally:
        conn.close()


def main() -> int:
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--sql",
        type=Path,
        help="path to dictionary_webster1828.sql (downloads if omitted)",
    )
    parser.add_argument(
        "--db",
        type=Path,
        default=here / "bible-app.sqlite",
        help="unpacked bible-app.sqlite to update",
    )
    args = parser.parse_args()
    if not args.db.is_file():
        print(f"database not found: {args.db}", file=sys.stderr)
        return 1
    entries = collect_entries(load_sql(args.sql))
    if len(entries) < 50_000:
        print(f"too few entries ({len(entries)}); refusing to write", file=sys.stderr)
        return 1
    write_sqlite(args.db, entries)
    print(f"wrote {len(entries)} Webster entries → {args.db}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
