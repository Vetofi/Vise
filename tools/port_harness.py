#!/usr/bin/env python3

from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import re
import shutil
import subprocess

try:
    from . import lab, port_regen
except ImportError:
    import lab
    import port_regen

RESOURCES_URL = "https://resources.download.minecraft.net"
PANORAMA = [f"minecraft/textures/gui/title/background/panorama_{i}.png" for i in range(6)] + [
    "minecraft/textures/gui/title/background/panorama_overlay.png"]


def wanted_asset(name):
    return name == "minecraft/sounds.json" or name.startswith("minecraft/sounds/") or name in PANORAMA


def launcher_assets(tree, manifest, jobs=8):
    info = manifest["assetIndex"]
    index = lab.download(info["url"], tree / "assets" / "indexes" / f"{info['id']}.json", info["sha1"])
    objects = json.loads(Path(index).read_text())["objects"]
    hashes = sorted({entry["hash"] for name, entry in objects.items() if wanted_asset(name)})

    def fetch(checksum):
        lab.download(f"{RESOURCES_URL}/{checksum[:2]}/{checksum}", tree / "assets" / "objects" / checksum[:2] / checksum,
                     checksum)

    with ThreadPoolExecutor(jobs) as pool:
        list(pool.map(fetch, hashes))
    return len(hashes)


def own_reports(root):
    reports = root / ".local" / "datagen" / "out" / "reports"
    if reports.is_symlink():
        reports.unlink()


def prepare_resources(root, version, store):
    own_reports(root)
    log = root / ".local" / "port-regen" / "vise-prepare.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, CARGO_TARGET_DIR=str(root / ".local" / "port-target"))
    jobs = os.environ.get("FLEET_LOAD_JOBS")
    try:
        port_regen.run(["cargo", "build", "--release", "-p", "vise-prepare", *(["-j", jobs] if jobs else [])],
                       root, env=env)
        binary = root / ".local/port-target/release/vise-prepare"
        port_regen.run([binary, "--root", root], root, log=log)
        found = json.loads((store / "prepared.json").read_text())["paths"]
        manifest = json.loads(Path(found["version_json"]).read_text())
        tree = store / "launcher"
        if tree.exists():
            launcher_assets(tree, manifest)
            for step in ("sounds", "panorama"):
                port_regen.run([binary, step, "--root", root, "--minecraft-dir", tree], root,
                               log=log.with_name(f"vise-prepare-{step}.log"))
        return {"status": "ok"}
    except (RuntimeError, OSError, ValueError, KeyError) as error:
        output = getattr(error, "output", str(error))
        return {"status": "failed", "error": str(error)[-2500:],
                "compile_errors": port_regen.compile_errors(output + named_logs(output))}


def named_logs(output):
    text = ""
    for name in sorted(set(re.findall(r"see ((?:/|[A-Za-z]:[\\/])\S+\.log)", output))):
        path = Path(name)
        if path.is_file():
            text += path.read_text(errors="replace")[-(1 << 20):]
    return text


def run_harnesses(root, names=None, timeout=None):
    command = [port_regen.python(), "tools/parity.py", *(names or ["--all"])]
    if timeout:
        command += ["--timeout", str(timeout)]
    env = dict(os.environ, PARITY_HOSTS="")
    log = root / ".local" / "port-regen" / "parity.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as stream:
        subprocess.run(command, cwd=root, stdout=stream, stderr=subprocess.STDOUT, env=env)
    return root / ".local" / "parity-runner" / "state.json"


def harness_logs(logs, harness):
    return sorted([*logs.glob(f"{harness}.log"), *logs.glob(f"{harness}#*.log")])


def statuses(state):
    found = {}
    for name, record in state.get("harnesses", {}).items():
        harness = name.split("#")[0]
        found[harness] = "fail" if record.get("status") != "pass" or found.get(harness) == "fail" else "pass"
    return found


def classify(state, logs, baseline=None, names=None):
    now = statuses(state)
    before = statuses(baseline) if baseline else {}
    result = {}
    for harness, status in sorted(now.items()):
        if names and harness not in names:
            continue
        entry = {"status": status, "baseline": before.get(harness)}
        if status != "pass":
            text = "".join(path.read_text(errors="replace")[-(1 << 20):] for path in harness_logs(logs, harness))
            errors = port_regen.compile_errors(text)
            if errors:
                entry["status"] = "oracle"
                entry["compile_errors"] = errors
        entry["new"] = entry["status"] != "pass" and entry["baseline"] != "fail"
        result[harness] = entry
    return result


def summary(results):
    counts = {"pass": 0, "oracle": 0, "fail": 0, "new": 0, "known": 0}
    for entry in results.values():
        counts[entry["status"]] += 1
        if entry["status"] != "pass":
            counts["new" if entry["new"] else "known"] += 1
    return counts


def harnesses(version, store, root, baseline=None, names=None, timeout=None, previous=None):
    root = Path(root)
    if not (root / ".git").exists():
        raise SystemExit(f"No candidate at {root}; run version_port.py regen {version} first")
    pinned = json.loads((root / "research-lock.json").read_text())["minecraft"]["version"]
    if pinned != version:
        raise SystemExit(f"{root} pins {pinned}, not {version}")
    if previous:
        prepared, names = dict(previous["prepare"]), names or list(previous["harnesses"])
        if prepared["status"] == "failed" and not prepared.get("compile_errors"):
            prepared["compile_errors"] = port_regen.compile_errors(named_logs(prepared.get("error", "")))
        state = json.loads((root / ".local" / "parity-runner" / "state.json").read_text())
    else:
        print(f"vise-prepare in {root}", flush=True)
        prepared = prepare_resources(root, version, store)
        print(f"  {prepared['status']}", flush=True)
        print("tools/parity.py " + (" ".join(names) if names else "--all"), flush=True)
        state = json.loads(run_harnesses(root, names, timeout).read_text())
    base = json.loads(Path(baseline).read_text()) if baseline else None
    results = classify(state, root / ".local" / "parity-runner" / "logs", base, names)
    return {"candidate": str(root), "commit": port_regen.git("rev-parse", "HEAD", cwd=root).strip(),
            "baseline": str(baseline) if baseline else None, "prepare": prepared, "harnesses": results,
            "summary": summary(results)}


def keep_logs(result, destination):
    logs = Path(result["candidate"]) / ".local" / "parity-runner" / "logs"
    destination.mkdir(parents=True, exist_ok=True)
    for harness, entry in result["harnesses"].items():
        if entry["status"] != "pass":
            for path in harness_logs(logs, harness):
                shutil.copyfile(path, destination / path.name)
