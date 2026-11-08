#!/usr/bin/env python3

import argparse
import json
import os
import shutil
import subprocess
import sys

import lab

COMMANDS = {
    "classes": ("search-android-classes", ["query"]),
    "class": ("inspect-android-class", ["class_name"]),
    "method": ("inspect-android-method", ["class_name", "method_name"]),
    "callers": ("trace-android-references", ["class_name", "method_name"]),
    "refs": ("trace-android-references", ["class_name"]),
}


def engine(lock):
    pin = lock["jadx_headless_mcp"]
    jar = lab.LOCAL / "tools" / f"jadx-headless-mcp-{pin['version']}-all.jar"
    return lab.download(pin["url"], jar, pin["sha256"], "sha256")


def target(game_dir, name):
    source = game_dir / "jars" / f"{name}.jar"
    if not source.exists():
        raise SystemExit(f"{source} is missing; run cargo run --release -p vise-prepare -- --prepare")
    copy = lab.LOCAL / "rea" / f"{name}-{lab.digest(source, 'sha256')[:16]}.apk"
    if not copy.exists():
        copy.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, copy)
    return copy


def rea_command(lock):
    pin = lock["rea"]
    installed = shutil.which("rea")
    if installed:
        version = subprocess.run([installed, "--version"], capture_output=True, text=True).stdout.strip()
        if version == pin["version"]:
            return [installed]
    npx = shutil.which("npx")
    if not npx:
        raise SystemExit("npx is missing: install Node.js 22.19+ (or the pinned `rea` command)")
    return [npx, "-y", f"{pin['package']}@{pin['version']}"]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--jar", default="client", choices=["client", "server-game"])
    parser.add_argument("--overload-index", type=int)
    parser.add_argument("--raw", action="store_true", help="print REA's whole evidence record")
    parser.add_argument("command", choices=COMMANDS)
    parser.add_argument("arguments", nargs="+")
    args = parser.parse_args()
    subcommand, names = COMMANDS[args.command]
    if len(args.arguments) != len(names):
        parser.error(f"{args.command} takes {' '.join(names).upper()}")
    lock, game_dir = lab.config()
    command = rea_command(lock) + [subcommand, str(target(game_dir, args.jar))]
    if args.command in ("callers", "refs"):
        command += [args.arguments[0]] + (["--method-name", args.arguments[1]] if args.command == "callers" else [])
    else:
        command += args.arguments
    if args.overload_index is not None:
        command += ["--overload-index", str(args.overload_index)]
    environment = dict(os.environ, REA_JADX_MCP_JAR=str(engine(lock)))
    result = subprocess.run(command + ["--json"], capture_output=True, text=True, env=environment)
    try:
        record = json.loads(result.stdout)
    except json.JSONDecodeError:
        sys.stderr.write(result.stdout + result.stderr)
        return result.returncode or 1
    if "normalized_result" not in record:
        details = record.get("details", {})
        reasons = [issue["message"] for issue in details.get("issues", [])] or [details.get("reason")]
        print(record.get("message"), *filter(None, reasons), sep="\n", file=sys.stderr)
        return 1
    body = record if args.raw else record["normalized_result"]
    if not args.raw:
        body.pop("engine", None)
    if args.command == "method" and not args.raw and body.get("source", {}).get("text"):
        print(f"// {body['class_name']}.{body['method']['reported_signature']}"
              f" (overload {body['method']['overload_index']} of {body['overload_count']}, {body['source']['status']})")
        print(body["source"]["text"])
    else:
        print(json.dumps(body, indent=2))
    return 0


if __name__ == "__main__":
    sys.exit(main())
