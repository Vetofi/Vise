#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field

MARKER = "<!-- overlap-note -->"
MAX_LISTED = 5
MAX_FILES = 100
MAX_CANDIDATES = 40
HOT = 5
MIN_SHARED = 2
MIN_SHARE = 0.1
SAFE_GIT = ["-c", "core.hooksPath=/dev/null", "-c", "core.fsmonitor=false", "-c", "diff.external=",
            "-c", "core.quotePath=false"]
ATTRIBUTES = "*.rs diff=rust\n*.py diff=python\n*.java diff=java\n"
NOT_WORK = re.compile(r"/examples/|^tests/|/tests/|^tools/test_|_tests?\.(?:rs|py)$")
IGNORED = re.compile(r"(?:^|/)(?:Cargo\.lock|Cargo\.toml|lib\.rs|mod\.rs|main\.rs)$|\.(?:json|tsv|snap|png)$")
HUNK = re.compile(r"^@@ -\d+(?:,(\d+))? \+\d+(?:,\d+)? @@ ?(.*)$")
SKIP_LINE = re.compile(r"^\+\s*(?:$|//|#\[|#(?!\[)|@)")
NEW_ITEM = re.compile(r"^\+\s*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|const|unsafe|extern)\s+)*"
                      r"(?:fn|def|class|impl|struct|enum|trait|mod|macro_rules!)\b")
NAME = re.compile(r"\b(?:fn|def)\s+([^\s(<{:]+)")


@dataclass
class Pull:
    number: int
    title: str
    author: str
    draft: bool
    head: str
    files: list[str]
    changed_files: int
    closes: set[int] = field(default_factory=set)
    funcs: set[tuple[str, str]] = field(default_factory=set)


def functions(diff: str) -> set[tuple[str, str]]:
    out: set[tuple[str, str]] = set()
    path, header, removed, added = None, "", 0, []

    def close_hunk():
        first = next((line for line in added if not SKIP_LINE.match(line)), "")
        if path and header and (removed or not NEW_ITEM.match(first)):
            m = NAME.search(header)
            if m and not NOT_WORK.search(path):
                out.add((path, m.group(1)))

    for line in diff.splitlines():
        if line.startswith("diff --git "):
            close_hunk()
            path, header, removed, added = None, "", 0, []
        elif line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
        elif (m := HUNK.match(line)):
            close_hunk()
            removed = 1 if m.group(1) is None else int(m.group(1))
            header, added = m.group(2), []
        elif line.startswith("+") and header:
            added.append(line)
    close_hunk()
    return out


def hot(pulls: list[Pull]) -> set[tuple[str, str]]:
    count: dict[tuple[str, str], int] = {}
    for p in pulls:
        for f in p.funcs:
            count[f] = count.get(f, 0) + 1
    return {f for f, n in count.items() if n >= HOT}


def overlaps(me: Pull, others: list[Pull], stacked=lambda a, b: False):
    common = hot([me, *others])
    found = []
    for o in others:
        if o.number == me.number or o.changed_files > MAX_FILES:
            continue
        issues, funcs = me.closes & o.closes, (me.funcs & o.funcs) - common
        if not issues and (len(funcs) < MIN_SHARED or len(funcs) < MIN_SHARE * max(len(me.funcs), len(o.funcs))):
            continue
        if (issues or funcs) and not stacked(me, o):
            found.append((o, issues, funcs))
    found.sort(key=lambda t: (-len(t[1]) * 100 - len(t[2]), -t[0].number))
    return found[:MAX_LISTED]


def note(found) -> str | None:
    if not found:
        return None
    lines = [MARKER, "🤖 Overlap: these open pull requests touch the same work. Have a look, and talk to each other "
             "in the threads if it's the same change; nothing to do if it isn't."]
    for o, issues, funcs in found:
        what = []
        if issues:
            what.append("also closes " + ", ".join(f"#{i}" for i in sorted(issues)))
        if funcs:
            shown = sorted(funcs)[:4]
            more = f" and {len(funcs) - len(shown)} more" if len(funcs) > len(shown) else ""
            what.append("changes " + ", ".join(f"`{f}` in `{p.rsplit('/', 1)[-1]}`" for p, f in shown) + more)
        draft = " (draft)" if o.draft else ""
        lines.append(f"- #{o.number}{draft} by {o.author}: " + "; ".join(what))
    lines.append("\n<sub>Edited on every push; it lists at most 5. tools/ci/overlap_note.py</sub>")
    return "\n".join(lines)


CLEARED = MARKER + "\n🤖 Overlap: none any more."


def gh(*args: str) -> str:
    r = subprocess.run(["gh", *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"gh {args[0]} {args[1] if len(args) > 1 else ''}: {r.stderr.strip()[:300]}")
    return r.stdout.strip()


def git(*args: str, check: bool = True) -> str:
    r = subprocess.run(["git", *SAFE_GIT, *args], capture_output=True, text=True)
    if check and r.returncode != 0:
        raise RuntimeError(f"git {args[0]}: {r.stderr.strip()[:300]}")
    return r.stdout


def has_commit(sha: str) -> bool:
    return bool(sha) and subprocess.run(["git", *SAFE_GIT, "cat-file", "-e", f"{sha}^{{commit}}"],
                                        capture_output=True).returncode == 0


OPEN_PULLS = """
query($owner: String!, $name: String!, $cursor: String) {
  repository(owner: $owner, name: $name) {
    pullRequests(first: 50, states: OPEN, baseRefName: "main", after: $cursor) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title isDraft headRefOid changedFiles author { login }
        closingIssuesReferences(first: 10) { nodes { number } }
        files(first: 100) { nodes { path } }
      }
    }
  }
}"""


def open_pulls(repo: str) -> list[Pull]:
    owner, name = repo.split("/")
    out, cursor = [], None
    while True:
        args = ["api", "graphql", "-F", f"owner={owner}", "-F", f"name={name}", "-f", f"query={OPEN_PULLS}"]
        if cursor:
            args += ["-F", f"cursor={cursor}"]
        page = json.loads(gh(*args))["data"]["repository"]["pullRequests"]
        for n in page["nodes"]:
            out.append(Pull(n["number"], n["title"], (n["author"] or {}).get("login", "ghost"), n["isDraft"],
                            n["headRefOid"], [f["path"] for f in (n["files"] or {}).get("nodes", [])],
                            n["changedFiles"], {i["number"] for i in n["closingIssuesReferences"]["nodes"]}))
        if not page["pageInfo"]["hasNextPage"]:
            return out
        cursor = page["pageInfo"]["endCursor"]


def candidates(me: Pull, pulls: list[Pull]) -> list[Pull]:
    mine = {p for p in me.files if not IGNORED.search(p)}
    scored = []
    for o in pulls:
        if o.number == me.number or o.changed_files > MAX_FILES:
            continue
        shared = len(mine & set(o.files))
        if shared or me.closes & o.closes:
            scored.append((shared + 100 * bool(me.closes & o.closes), o))
    scored.sort(key=lambda t: (-t[0], -t[1].number))
    return [o for _, o in scored[:MAX_CANDIDATES]]


def fill_functions(pulls: list[Pull], base: str = "origin/main") -> None:
    with tempfile.NamedTemporaryFile("w", suffix=".attributes", delete=False) as f:
        f.write(ATTRIBUTES)
    attrs = ["-c", f"core.attributesFile={f.name}"]
    try:
        missing = [p.head for p in pulls if not has_commit(p.head)]
        if missing:
            git("fetch", "--quiet", "--no-tags", "origin", *missing, check=False)
        for p in pulls:
            if not has_commit(p.head):
                continue
            files = [x for x in p.files if not IGNORED.search(x)]
            if not files:
                continue
            diff = git(*attrs, "diff", "-U0", "--no-color", "--no-ext-diff", "--no-textconv", f"{base}...{p.head}",
                       "--", *files, check=False)
            p.funcs = functions(diff)
    finally:
        os.unlink(f.name)


def stacked(a: Pull, b: Pull) -> bool:
    return any(subprocess.run(["git", *SAFE_GIT, "merge-base", "--is-ancestor", x.head, y.head],
                              capture_output=True).returncode == 0 for x, y in ((a, b), (b, a)))


def compare(me: Pull, pulls: list[Pull]):
    others = candidates(me, pulls)
    fill_functions([me, *others])
    return overlaps(me, others, stacked)


class Api:
    def __init__(self, repo: str):
        self.repo = repo

    def own_note(self, n: int) -> int | None:
        raw = gh("api", "--paginate", f"repos/{self.repo}/issues/{n}/comments?per_page=100", "--jq",
                 f'.[] | select(.user.login == "github-actions[bot]" and (.body | startswith("{MARKER}"))) | .id')
        ids = raw.split()
        return int(ids[0]) if ids else None

    def write(self, n: int, comment: int | None, text: str) -> None:
        if comment:
            gh("api", "-X", "PATCH", f"repos/{self.repo}/issues/comments/{comment}", "-f", f"body={text}")
        else:
            gh("api", f"repos/{self.repo}/issues/{n}/comments", "-f", f"body={text}")


def event() -> int:
    with open(os.environ["GITHUB_EVENT_PATH"]) as f:
        number = json.load(f)["pull_request"]["number"]
    repo = os.environ["REPO"]
    pulls = open_pulls(repo)
    me = next((p for p in pulls if p.number == number), None)
    if me is None:
        return 0
    text = note(compare(me, pulls))
    api = Api(repo)
    existing = api.own_note(number)
    if text is None and existing is None:
        return 0
    api.write(number, existing, text or CLEARED)
    return 0


if __name__ == "__main__":
    verb, rest = (sys.argv[1:2] or [""])[0], sys.argv[2:]
    if verb == "event" and not rest:
        sys.exit(event())
    if verb in ("show", "survey"):
        pulls = open_pulls(os.environ.get("REPO", "vise-rust/vise"))
        wanted = {int(x) for x in rest} if verb == "show" else {p.number for p in pulls}
        fill_functions([p for p in pulls if p.changed_files <= MAX_FILES])
        result = {}
        for me in pulls:
            if me.number in wanted:
                found = overlaps(me, candidates(me, pulls), stacked)
                result[me.number] = [(o.number, sorted(i), sorted(f), len(me.funcs), len(o.funcs))
                                     for o, i, f in found]
                if verb == "show":
                    print(f"#{me.number}:\n{note(found) or '(no note)'}\n")
        if verb == "survey":
            print(json.dumps(result))
        sys.exit(0)
    sys.exit(__doc__)
