#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
REPO = os.environ.get("GITHUB_REPOSITORY", "vise-rust/vise")
LAST_RUN = ROOT / ".local" / "parity-runner" / "last-run.json"


def git(*arguments, check=True):
    result = subprocess.run(["git", *arguments], cwd=ROOT, capture_output=True, text=True)
    if check and result.returncode:
        sys.exit(f"git {' '.join(arguments)}: {result.stderr.strip()}")
    return result


def green_runs():
    result = subprocess.run(["gh", "run", "list", "--repo", REPO, "--workflow", "parity-main", "--branch", "main",
                             "--status", "success", "--limit", "30", "--json", "headSha"], capture_output=True, text=True)
    if result.returncode:
        print(f"note: no parity-main history ({result.stderr.strip()})", file=sys.stderr)
        return []
    return [run["headSha"] for run in json.loads(result.stdout or "[]")]


def base(head="HEAD", runs=None):
    for sha in green_runs() if runs is None else runs:
        if git("merge-base", "--is-ancestor", sha, head, check=False).returncode == 0:
            return git("rev-parse", sha).stdout.strip()
    return git("rev-parse", f"{head}^1").stdout.strip()


def first_bad(commits, fails):
    good, bad = -1, len(commits) - 1
    while bad - good > 1:
        middle = (good + bad) // 2
        if fails(commits[middle]):
            bad = middle
        else:
            good = middle
    return commits[bad]


def harness_exists(name):
    return (ROOT / "tools" / f"test_{name.split('#')[0]}_parity.py").exists()


def run_harnesses(names, timeout):
    present = [name for name in names if harness_exists(name)]
    if not present:
        return False
    return subprocess.run([sys.executable, "tools/parity.py", *present, "--timeout", str(timeout)], cwd=ROOT).returncode != 0


def bisect(since, timeout, comment):
    failed = []
    if LAST_RUN.exists():
        failed = json.loads(LAST_RUN.read_text(encoding="utf-8")).get("failed") or []
    if not failed:
        print("No failed harness recorded: nothing to bisect.")
        return 1
    head = git("rev-parse", "HEAD").stdout.strip()
    commits = git("rev-list", "--first-parent", "--reverse", f"{since}..{head}").stdout.split()
    if not commits:
        print("No merge since the base.")
        return 1

    def fails(commit):
        print(f"--- {commit[:9]} {git('log', '-1', '--format=%s', commit).stdout.strip()}", flush=True)
        git("checkout", "-q", "--detach", commit)
        return run_harnesses(failed, timeout)

    def blame_nobody(why, code):
        line = f"parity-main: {', '.join(failed)} {why}: no merge is named."
        print(line)
        if os.environ.get("GITHUB_STEP_SUMMARY"):
            with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
                summary.write(f"### {line}\n")
        return code

    try:
        if not fails(head):
            return blame_nobody(f"pass when run again at {head[:9]}: a flake, counted as a pass", 0)
        if fails(since):
            return blame_nobody(f"fail at the base {since[:9]} too: the runner or a flake", 1)
        culprit = first_bad(commits, fails) if len(commits) > 1 else commits[0]
    finally:
        git("checkout", "-q", "--detach", head)
    subject = git("log", "-1", "--format=%s", culprit).stdout.strip()
    number = re.search(r"Merge pull request #(\d+)", subject)
    line = (f"parity-main: {', '.join(failed)} first fail at {culprit[:9]} ({subject}), "
            f"of {len(commits)} merges since {since[:9]}.")
    print(line)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(f"### {line}\n")
    if comment and number:
        url = f"{os.environ.get('GITHUB_SERVER_URL', 'https://github.com')}/{REPO}/actions/runs/{os.environ.get('GITHUB_RUN_ID', '')}"
        body = (f"🤖 Agent: parity-main found that this merge breaks {', '.join(f'`{name}`' for name in failed)} on main "
                f"(bisected over {len(commits)} merges: {url}). Releases wait until parity-main passes again.")
        subprocess.run(["gh", "pr", "comment", number.group(1), "--repo", REPO, "--body", body])
    return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("base")
    bisecting = commands.add_parser("bisect")
    bisecting.add_argument("since", help="the last commit known to pass (the run's base)")
    bisecting.add_argument("--timeout", type=int, default=2400, help="seconds per harness")
    bisecting.add_argument("--comment", action="store_true", help="comment on the breaking merge's pull request")
    args = parser.parse_args()
    if args.command == "base":
        print(base())
        return 0
    return bisect(args.since, args.timeout, args.comment)


if __name__ == "__main__":
    sys.exit(main())
