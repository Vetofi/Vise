#!/usr/bin/env python3

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[1]
RESULT = ROOT / ".local" / "gate.json"


def steps(args):
    yield "public tree", [sys.executable, "tools/check_public_tree.py"]
    yield "rustfmt", ["cargo", "+stable", "fmt", "--all", "--", "--check"]
    yield "clippy", ["cargo", "+stable", "clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"]
    if args.quick:
        yield "python tests", [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-q"]
        return
    yield "rust tests", ["cargo", "+stable", "test", "--locked", "--workspace", "--lib", "--bins", "--tests"]
    yield "doc tests", ["cargo", "+stable", "test", "--locked", "--workspace", "--doc"]
    yield "python tests", [sys.executable, "-m", "unittest", "discover", "-s", "tests", "-q"]
    if not args.no_parity:
        command = [sys.executable, "tools/parity.py", *(["-j", str(args.jobs)] if args.jobs is not None else [])]
        if args.flake_base:
            command += ["--flake-check", args.flake_base]
        yield "vanilla harnesses", command


def head():
    return subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, capture_output=True, text=True).stdout.strip()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--no-parity", action="store_true", help="skip the vanilla harnesses")
    parser.add_argument("--quick", action="store_true", help="only formatting, lints and Python tests")
    parser.add_argument("--known-failures", default="", help="comma-separated harnesses allowed to keep failing")
    parser.add_argument("-j", "--jobs", type=int, help="harnesses at once on this machine (default: tools/parity.py's)")
    parser.add_argument("--flake-base", default="parity-base", help="harnesses new or changed since the merge base "
                        "with this ref must pass 3 runs in a row ('' to skip; default: parity-base)")
    args = parser.parse_args()
    logs = ROOT / ".local" / "gate"
    logs.mkdir(parents=True, exist_ok=True)
    commit, dirty = head(), bool(subprocess.run(["git", "status", "--porcelain"], cwd=ROOT, capture_output=True, text=True).stdout.strip())
    results = []
    passed = True
    for name, command in steps(args):
        started = time.monotonic()
        log = logs / (name.replace(" ", "-") + ".log")
        print(f"{name}...", end=" ", flush=True)
        if name == "vanilla harnesses":
            run_id = uuid.uuid4().hex
            subprocess.run(command, cwd=ROOT, env=dict(os.environ, PARITY_RUN_ID=run_id))
            summary = ROOT / ".local/parity-runner/last-run.json"
            finished = json.loads(summary.read_text()) if summary.exists() else {}
            if finished.get("run") != run_id:
                print("tools/parity.py stopped before finishing its run")
                status = 1
            else:
                known = set(filter(None, args.known_failures.split(",")))
                failing = set(finished["failed"])
                new = sorted(name for name in failing if name not in known and name.split("#")[0] not in known)
                if failing:
                    print(f"Failing harnesses: {', '.join(sorted(failing))}; new: {', '.join(new) or 'none'}")
                status = 1 if new else 0
        else:
            with log.open("w") as stream:
                status = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT).returncode
        seconds = round(time.monotonic() - started)
        results.append({"step": name, "passed": status == 0, "seconds": seconds})
        if status:
            passed = False
            print(f"FAILED ({seconds}s)")
            if name != "vanilla harnesses":
                print("".join(log.read_text(errors="replace").splitlines(keepends=True)[-40:]))
                print(f"Full log: {log}")
            break
        print(f"ok ({seconds}s)")
    RESULT.write_text(json.dumps({"commit": commit, "dirty": dirty, "passed": passed, "quick": args.quick,
                                  "parity": not (args.quick or args.no_parity), "steps": results,
                                  "finished": time.strftime("%Y-%m-%dT%H:%M:%S")}, indent=1))
    print("Gate passed." if passed else "Gate failed.")
    return 0 if passed else 1


if __name__ == "__main__":
    sys.exit(main())
