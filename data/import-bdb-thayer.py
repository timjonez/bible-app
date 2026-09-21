#!/usr/bin/env python3
"""Load BDB and Thayer, keyed by Strong's codes, into bible-app.sqlite.

Brown-Driver-Briggs 1906 text is public domain. The structured dump is the
Open Scriptures Hebrew Lexicon (CC BY 4.0): credit the Open Scriptures
Hebrew Bible Project. Do not treat that dump as public domain.

Thayer 1889 text is public domain. The Strong's-keyed JSONL dump is
nigelmsipa/thayers-greek-lexicon-dataset (CC0 1.0). Unmapped lemmas are
joined to Strong's numbers via morphgnt/strongs-dictionary-xml (CC0).

HTML is stripped to plain text to match Easton and the other modules.
"""

from __future__ import annotations

import argparse
import html as htmlmod
import json
import re
import sqlite3
import sys
import unicodedata
import urllib.request
import xml.etree.ElementTree as ET
from collections import defaultdict
from html.parser import HTMLParser
from pathlib import Path

BDB_XML_URL = (
    "https://raw.githubusercontent.com/openscriptures/HebrewLexicon/"
    "master/BrownDriverBriggs.xml"
)
BDB_INDEX_URL = (
    "https://raw.githubusercontent.com/openscriptures/HebrewLexicon/"
    "master/LexicalIndex.xml"
)
THAYER_JSONL_URL = (
    "https://raw.githubusercontent.com/nigelmsipa/"
    "thayers-greek-lexicon-dataset/master/data/thayer_lexicon.jsonl"
)
STRONGS_GREEK_URL = (
    "https://raw.githubusercontent.com/morphgnt/strongs-dictionary-xml/"
    "master/strongsgreek.xml"
)

BDB_NS = "{http://openscriptures.github.com/morphhb/namespace}"

BDB_MODULE = "BDB"
BDB_TITLE = "Brown-Driver-Briggs Hebrew Lexicon"
BDB_LICENSE = "CC BY 4.0"

THAYER_MODULE = "Thayer"
THAYER_TITLE = "Thayer's Greek-English Lexicon"
THAYER_LICENSE = "CC0-1.0"

SKIP_XML_TAGS = {"status", "page"}


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
    if "<" not in raw:
        return compact_text(htmlmod.unescape(raw).replace("\xa0", " "))
    parser = TextExtractor()
    try:
        parser.feed(raw)
        parser.close()
        text = "".join(parser.parts)
    except Exception:
        text = re.sub(r"<[^>]+>", "", raw)
    return compact_text(htmlmod.unescape(text).replace("\xa0", " "))


def compact_text(text: str) -> str:
    text = re.sub(r"[ \t]+\n", "\n", text)
    text = re.sub(r"\n{3,}", "\n\n", text)
    text = re.sub(r"[ \t]{2,}", " ", text)
    return text.strip()


def download(url: str, dest: Path) -> Path:
    if dest.is_file() and dest.stat().st_size > 0:
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    print(f"downloading {url}", file=sys.stderr)
    with urllib.request.urlopen(url) as resp:
        dest.write_bytes(resp.read())
    return dest


def xml_tag(el: ET.Element) -> str:
    return el.tag.split("}", 1)[-1]


def bdb_entry_text(el: ET.Element) -> str:
    parts: list[str] = []

    def walk(e: ET.Element) -> None:
        tag = xml_tag(e)
        if tag in SKIP_XML_TAGS:
            return
        if tag == "sense":
            n = e.get("n")
            stem = e.get("stem")
            if n:
                parts.append(f"\n{n}. ")
            elif stem:
                parts.append(f"\n{stem}: ")
            else:
                parts.append("\n")
        if e.text:
            parts.append(e.text)
        for child in e:
            walk(child)
            if child.tail:
                parts.append(child.tail)

    walk(el)
    return compact_text("".join(parts))


def collect_bdb(bdb_xml: Path, index_xml: Path) -> list[tuple[str, str]]:
    bdb_root = ET.parse(bdb_xml).getroot()
    by_id: dict[str, str] = {}
    for el in bdb_root.iter(f"{BDB_NS}entry"):
        eid = el.get("id")
        if not eid:
            continue
        text = bdb_entry_text(el)
        if text:
            by_id[eid] = text

    index_root = ET.parse(index_xml).getroot()
    ids_by_num: dict[int, list[str]] = defaultdict(list)
    for xref in index_root.iter(f"{BDB_NS}xref"):
        strong = xref.get("strong")
        bdb_id = xref.get("bdb")
        if not strong or not bdb_id or not strong.isdigit():
            continue
        ids_by_num[int(strong)].append(bdb_id)

    out: list[tuple[str, str]] = []
    for num in sorted(ids_by_num):
        seen: set[str] = set()
        chunks: list[str] = []
        for bdb_id in ids_by_num[num]:
            text = by_id.get(bdb_id)
            if not text or text in seen:
                continue
            seen.add(text)
            chunks.append(text)
        if not chunks:
            continue
        out.append((f"H{num}", "\n\n".join(chunks)))
    print(f"BDB: {len(out)} Strong's-keyed entries", file=sys.stderr)
    return out


def strip_accents(s: str) -> str:
    return "".join(
        c for c in unicodedata.normalize("NFD", s) if unicodedata.category(c) != "Mn"
    )


def norm_greek(s: str) -> str:
    s = strip_accents(s or "").lower()
    return re.sub(r"[^α-ω]+", "", s)


def greek_lemma_to_strongs(xml_path: Path) -> dict[str, list[int]]:
    root = ET.parse(xml_path).getroot()
    out: dict[str, list[int]] = defaultdict(list)
    for entry in root.findall("entries/entry"):
        raw = entry.get("strongs")
        greek = entry.find("greek")
        if raw is None or greek is None:
            continue
        try:
            num = int(raw)
        except ValueError:
            continue
        key = norm_greek(greek.get("unicode") or "")
        if key and num not in out[key]:
            out[key].append(num)
    return out


def parse_thayer_codes(raw) -> list[int]:
    if not raw:
        return []
    if isinstance(raw, (int, float)):
        n = int(raw)
        return [n] if n > 0 else []
    if isinstance(raw, str):
        raw = [raw]
    out: list[int] = []
    for item in raw:
        s = str(item).strip().upper().replace(" ", "")
        if s.startswith("G"):
            s = s[1:]
        if s.isdigit():
            n = int(s)
            if n > 0 and n not in out:
                out.append(n)
    return out


def lemma_fallback_nums(obj: dict, greek_map: dict[str, list[int]]) -> list[int]:
    lemma = obj.get("lemma") or obj.get("lemma_normalized") or ""
    keys = [norm_greek(lemma)]
    first = lemma.split(",")[0].split()[0] if lemma else ""
    keys.append(norm_greek(first))
    for key in keys:
        if key and key in greek_map:
            return list(greek_map[key])
    return []


def format_thayer_text(obj: dict) -> str:
    lemma = compact_text(str(obj.get("lemma") or ""))
    grammar = compact_text(str(obj.get("grammar") or ""))
    body = html_to_text(str(obj.get("text") or ""))
    if not body:
        paras = obj.get("paragraphs") or []
        if isinstance(paras, list):
            body = html_to_text("\n\n".join(str(p) for p in paras if p))
    head = lemma
    if grammar:
        head = f"{lemma}, {grammar}" if lemma else grammar
    if head and body:
        if body.lower().startswith(lemma.lower()):
            return body
        return compact_text(f"{head}\n\n{body}")
    return body or head


def collect_thayer(jsonl: Path, strongs_greek: Path) -> list[tuple[str, str]]:
    greek_map = greek_lemma_to_strongs(strongs_greek)
    texts_by_num: dict[int, list[str]] = defaultdict(list)
    explicit = 0
    fallback = 0
    skipped = 0
    with jsonl.open(encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if not line:
                continue
            obj = json.loads(line)
            nums = parse_thayer_codes(obj.get("strongs"))
            if nums:
                explicit += 1
            else:
                nums = lemma_fallback_nums(obj, greek_map)
                if nums:
                    fallback += 1
            if not nums:
                skipped += 1
                continue
            text = format_thayer_text(obj)
            if len(text) < 8:
                skipped += 1
                continue
            for num in nums:
                if text not in texts_by_num[num]:
                    texts_by_num[num].append(text)
    out = [
        (f"G{num}", "\n\n".join(chunks))
        for num, chunks in sorted(texts_by_num.items())
        if chunks
    ]
    print(
        f"Thayer: {len(out)} Strong's-keyed entries "
        f"(explicit {explicit}, lemma-matched {fallback}, skipped {skipped})",
        file=sys.stderr,
    )
    return out


def write_module(
    conn: sqlite3.Connection,
    module: str,
    kind: str,
    title: str,
    license_str: str,
    entries: list[tuple[str, str]],
) -> None:
    conn.execute("DELETE FROM entries WHERE module = ?", (module,))
    conn.execute("DELETE FROM modules WHERE id = ?", (module,))
    conn.execute(
        "INSERT INTO modules (id, kind, title, license) VALUES (?, ?, ?, ?)",
        (module, kind, title, license_str),
    )
    conn.executemany(
        "INSERT INTO entries (module, i, headword, text) VALUES (?, ?, ?, ?)",
        (
            (module, int(head[1:]), head, text)
            for head, text in entries
        ),
    )


def write_sqlite(
    db: Path, bdb: list[tuple[str, str]], thayer: list[tuple[str, str]]
) -> None:
    conn = sqlite3.connect(db)
    try:
        conn.execute("BEGIN")
        write_module(conn, BDB_MODULE, "lexicon", BDB_TITLE, BDB_LICENSE, bdb)
        write_module(
            conn, THAYER_MODULE, "lexicon", THAYER_TITLE, THAYER_LICENSE, thayer
        )
        # Shipped gz does not include FTS; the reader backfills on open.
        conn.execute("DROP TABLE IF EXISTS entries_fts")
        conn.commit()
        conn.execute("VACUUM")
    finally:
        conn.close()


def main() -> int:
    here = Path(__file__).resolve().parent
    cache = Path("/tmp/bdb-thayer-src")
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bdb-xml", type=Path, help="BrownDriverBriggs.xml")
    parser.add_argument("--bdb-index", type=Path, help="LexicalIndex.xml")
    parser.add_argument("--thayer-jsonl", type=Path, help="thayer_lexicon.jsonl")
    parser.add_argument(
        "--strongs-greek",
        type=Path,
        help="morphgnt strongsgreek.xml for lemma fallback",
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

    bdb_xml = args.bdb_xml or download(BDB_XML_URL, cache / "BrownDriverBriggs.xml")
    bdb_index = args.bdb_index or download(BDB_INDEX_URL, cache / "LexicalIndex.xml")
    thayer_jsonl = args.thayer_jsonl or download(
        THAYER_JSONL_URL, cache / "thayer_lexicon.jsonl"
    )
    strongs_greek = args.strongs_greek or download(
        STRONGS_GREEK_URL, cache / "strongsgreek.xml"
    )

    bdb = collect_bdb(bdb_xml, bdb_index)
    thayer = collect_thayer(thayer_jsonl, strongs_greek)
    if len(bdb) < 8000:
        print(f"too few BDB entries ({len(bdb)}); refusing to write", file=sys.stderr)
        return 1
    if len(thayer) < 3000:
        print(
            f"too few Thayer entries ({len(thayer)}); refusing to write",
            file=sys.stderr,
        )
        return 1
    write_sqlite(args.db, bdb, thayer)
    print(f"wrote {len(bdb)} BDB and {len(thayer)} Thayer entries → {args.db}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
