#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
README = ROOT / "README.md"
REPO = "vise-rust/vise"
ISSUES = list(range(3512, 3522))
LINE = re.compile(r"^\*\*Player-visible gaps: (\d+) open\*\*.*$", re.M)
BADGE = re.compile(r"(https://img\.shields\.io/badge/player--visible%20gaps-)(\d+)(%20open-)")
OPEN_ROW = re.compile(r"^\s*[-*] \[ \] ", re.M)
DONE_ROW = re.compile(r"^\s*[-*] \[[xX]\] ", re.M)
AREA = re.compile(r"port gaps:\s*(.+?)\s*\(\d+\)\s*$")


def rows(body: str) -> tuple[int, int]:
    return len(OPEN_ROW.findall(body or "")), len(DONE_ROW.findall(body or ""))


def area(title: str) -> str:
    m = AREA.search(title)
    return m.group(1) if m else title


def fetch() -> list[dict]:
    out = []
    for n in ISSUES:
        raw = subprocess.run(["gh", "api", f"repos/{REPO}/issues/{n}", "--jq", "{title, body, state}"],
                             capture_output=True, text=True, check=True).stdout
        issue = json.loads(raw)
        still, done = rows(issue["body"])
        if issue["state"] == "closed":
            still, done = 0, still + done
        out.append({"number": n, "area": area(issue["title"]), "open": still, "closed": done})
    return out


def block(lists: list[dict]) -> str:
    still = sum(l["open"] for l in lists)
    total = still + sum(l["closed"] for l in lists)
    parts = "; ".join(f"{l['area']} [{l['open']}](https://github.com/{REPO}/issues/{l['number']})"
                      for l in sorted(lists, key=lambda l: -l["open"]))
    return (f"**Player-visible gaps: {still} open** of the {total} the [port ledger](tools/port_ledger.py) found, "
            "reading every vanilla 26.3 method against Vise's code: vanilla behaviour a player would notice that "
            f"Vise doesn't have yet. By area: {parts}. This number goes down as gaps close "
            "(`tools/parity_gaps.py`); the harness results below say that what is tested matches.")


def written(text: str) -> tuple[int, list[int]] | None:
    m = LINE.search(text)
    if not m:
        return None
    counts = [int(x) for x in re.findall(r" \[(\d+)\]\(https://github\.com/[^)]+/issues/\d+\)", m.group(0))]
    return int(m.group(1)), counts


def rewrite(text: str, new_block: str, still: int) -> str:
    if LINE.search(text):
        text = LINE.sub(lambda _: new_block, text, count=1)
    else:
        text = text.replace("## Project status\n\n", f"## Project status\n\n{new_block}\n\n", 1)
    if BADGE.search(text):
        return BADGE.sub(lambda m: f"{m.group(1)}{still}{m.group(3)}", text)
    parity = re.search(r'  <img src="https://img\.shields\.io/badge/vanilla%20parity-[^\n]*\n', text)
    badge = (f'  <a href="#project-status"><img src="https://img.shields.io/badge/player--visible%20gaps-{still}'
             f'%20open-F0795A" alt="Player-visible gaps open"></a>\n')
    return text[:parity.end()] + badge + text[parity.end():] if parity else text


def main(argv: list[str]) -> int:
    if argv == ["count"]:
        lists = fetch()
        for l in lists:
            print(f"#{l['number']}  {l['open']:4} open  {l['closed']:4} closed  {l['area']}")
        print(f"total {sum(l['open'] for l in lists)} open")
        return 0
    if argv in (["readme"], ["readme", "--write"]):
        lists = fetch()
        new = block(lists)
        if argv[1:] != ["--write"]:
            print(new)
            return 0
        text = README.read_text(encoding="utf-8")
        README.write_text(rewrite(text, new, sum(l["open"] for l in lists)), encoding="utf-8")
        print("README.md: gap block written")
        return 0
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
