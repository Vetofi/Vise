#!/usr/bin/env python3

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import signal
import socket
import subprocess
import sys
import time

try:
    from . import parity_reach
except ImportError:
    import parity_reach

ROOT = Path(__file__).resolve().parents[1]
LOCAL = ROOT / ".local"
STATE_DIR = LOCAL / "parity-runner"
STATE = STATE_DIR / "state.json"
TARGET = LOCAL / "parity-target"
PROFILE = {
    "CARGO_PROFILE_RELEASE_LTO": "false",
    "CARGO_PROFILE_RELEASE_CODEGEN_UNITS": "16",
    "CARGO_PROFILE_RELEASE_INCREMENTAL": "true",
    "CARGO_PROFILE_DEV_OPT_LEVEL": "3",
}
SHIM = STATE_DIR / "shim"
SHARED = Path(os.environ.get("PARITY_SHARED") or (LOCAL / "minecraft").resolve().parent / "parity-shared")
MACHINE_JOBS = int(os.environ.get("PARITY_MACHINE_JOBS") or max(1, (os.cpu_count() or 4) // 2))
HOST = socket.gethostname().split(".")[0]
DEFAULT_MEMORY = 3 << 30
FAILURE_TAILS, FAILURE_TAIL_LINES = 10, 30


def harness_settings(path):
    import ast
    text = Path(path).read_text(encoding="utf-8")
    settings = {}
    if "PARITY_" in text:
        for node in ast.parse(text).body:
            if isinstance(node, ast.Assign) and len(node.targets) == 1 and isinstance(node.targets[0], ast.Name) \
                    and node.targets[0].id in ("PARITY_SHARDS", "PARITY_RUST_SCOPE"):
                settings[node.targets[0].id] = ast.literal_eval(node.value)
    return settings


SHARDS = {}


def harnesses():
    found = {}
    for path in sorted((ROOT / "tools").glob("test_*_parity.py")):
        name = path.name[len("test_"):-len("_parity.py")]
        count = harness_settings(path).get("PARITY_SHARDS", 1)
        if count > 1:
            for index in range(1, count + 1):
                found[f"{name}#{index}"] = path
                SHARDS[f"{name}#{index}"] = f"{index}/{count}"
        else:
            found[name] = path
    return found


def base_name(name):
    return name.split("#")[0]


def load_state():
    return json.loads(STATE.read_text(encoding="utf-8")) if STATE.exists() else {"harnesses": {}, "hashes": {}}


def save_state(state):
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    temporary = STATE.with_suffix(".tmp")
    temporary.write_text(json.dumps(state, indent=1, sort_keys=True))
    temporary.replace(STATE)


class Hasher:

    def __init__(self, cache):
        self.cache = cache

    def __call__(self, path):
        real, _, keep = str(path).partition(KEEP)
        try:
            stat = os.stat(real)
        except FileNotFoundError:
            return "missing"
        stamp = [stat.st_size, stat.st_mtime_ns]
        cached = self.cache.get(str(path))
        if cached and cached[0] == stamp:
            return cached[1]
        if keep:
            digest = hashlib.sha256(crate_root_without_modules(Path(real).read_text(encoding="utf-8"), set(keep.split(","))).encode()).hexdigest()
        else:
            with open(real, "rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
        self.cache[str(path)] = [stamp, digest]
        return digest


KEEP = "#keep:"
MODULE_DECLARATION = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;\s*$")


def crate_root_without_modules(text, keep):
    lines, pending = [], []
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith(("#[", "///", "//!")) and not stripped.startswith("#!["):
            pending.append(line)
            continue
        declared = MODULE_DECLARATION.match(line)
        if declared and declared.group(1) not in keep:
            pending = []
            continue
        lines += pending + [line]
        pending = []
    return "\n".join(lines + pending)


def environment_key(hasher):
    def version(command):
        try:
            result = subprocess.run(command, capture_output=True, text=True, cwd=ROOT)
            return (result.stdout + result.stderr).strip()
        except FileNotFoundError:
            return "missing"
    lock = json.loads((ROOT / "research-lock.json").read_text(encoding="utf-8"))
    version_dir = LOCAL / "minecraft" / lock["minecraft"]["version"]
    parts = [version(["rustc", "-V"]), version(["java", "-version"]), version(["javac", "-version"]),
             json.dumps(PROFILE, sort_keys=True), hasher(ROOT / "research-lock.json"), hasher(ROOT / "Cargo.lock"),
             hasher(version_dir / "prepared.json"), hasher(version_dir / "manifest.json")]
    return hashlib.sha256("\n".join(parts).encode()).hexdigest()


def key(record, hasher, environment):
    digest = hashlib.sha256(environment.encode())
    for path in sorted(record["dependencies"]):
        digest.update(f"{path}\0{hasher(ROOT / path)}\n".encode())
    return digest.hexdigest()


def inside(path, root):
    try:
        Path(path).resolve().relative_to(root)
        return True
    except (ValueError, OSError):
        return False


def dep_info(binary):
    info = Path(str(binary) + ".d")
    if not info.exists():
        return []
    sources = []
    for line in info.read_text(encoding="utf-8").splitlines():
        _, separator, rest = line.partition(": ")
        if separator:
            sources += [part.replace("\\ ", " ") for part in re.split(r"(?<!\\) ", rest.strip()) if part]
    return sources


def artifact_path(kind, name, profile):
    directory = "debug" if profile == "dev" else profile
    return TARGET / directory / ("examples" if kind == "example" else "") / name


SELECTION_FLAGS = {"-p", "--package", "-F", "--features"}
SELECTION_SWITCHES = {"--workspace", "--all", "--all-features", "--no-default-features"}


def artifacts_of(arguments):
    arguments = arguments[:arguments.index("--")] if "--" in arguments else arguments
    profile = "release" if "--release" in arguments else "dev"
    selection = []
    found = []
    index = 1
    while index < len(arguments):
        argument = arguments[index]
        flag, _, value = argument.partition("=")
        if flag in SELECTION_FLAGS | {"--profile", "--example", "--bin"} and not value and index + 1 < len(arguments):
            value = arguments[index + 1]
            index += 1
        if flag == "--profile":
            profile = value
        elif flag in SELECTION_FLAGS:
            selection += ["-p" if flag == "--package" else "--features" if flag == "-F" else flag, value]
        elif flag in SELECTION_SWITCHES:
            selection.append("--workspace" if flag == "--all" else flag)
        elif flag in ("--example", "--bin"):
            found.append([flag[2:], value])
        elif flag in ("--examples", "--bins"):
            found.append([flag[2:], ""])
        index += 1
    examples = any(kind in ("example", "examples") for kind, _ in found)
    return [[kind, name, profile, selection, examples] for kind, name in found]


RUST_PATH = re.compile(r"(?<![\w:])((?:crate|self|super)(?:::super)*|[a-z_][a-z0-9_]*)::((?:[A-Za-z_]\w*::)*[A-Za-z_]\w*)")
RUST_TYPE = re.compile(r"\b(?:struct|enum|trait|union|type)\s+([A-Za-z_]\w*)")
RUST_MACRO_USE = re.compile(r"\b([A-Za-z_]\w*)!\s*[(\[{]")
RUST_MACRO_DEFINITION = re.compile(r"macro_rules!\s+([A-Za-z_]\w*)")


RUST_LITERAL_START = re.compile(r"//|/\*|(?<![\w])b?r#*\"|\"|'")
RUST_CODE_CACHE = {}


def rust_code(text):
    cached = RUST_CODE_CACHE.get(text)
    if cached is not None:
        return cached
    out, i, n = [], 0, len(text)
    def blank(chunk):
        return re.sub(r"[^\n]", " ", chunk)
    while i < n:
        match = RUST_LITERAL_START.search(text, i)
        if not match:
            out.append(text[i:])
            break
        out.append(text[i:match.start()])
        i, token = match.start(), match.group()
        if token == "//":
            end = text.find("\n", i)
            end = n if end < 0 else end
        elif token == "/*":
            depth, end = 1, i + 2
            while end < n and depth:
                nested = re.compile(r"/\*|\*/").search(text, end)
                if not nested:
                    end = n
                    break
                depth += 1 if nested.group() == "/*" else -1
                end = nested.end()
        elif token.endswith('"') and "r" in token:
            closing = '"' + "#" * token.count("#")
            end = text.find(closing, match.end())
            end = n if end < 0 else end + len(closing)
        elif token == '"':
            literal = re.compile(r'"(?:\\.|[^"\\])*"', re.DOTALL).match(text, i)
            end = literal.end() if literal else n
        else:
            literal = re.compile(r"'(?:\\(?:u\{[0-9a-fA-F]*\}|x..|.)|[^\\'\n])'").match(text, i)
            if not literal:
                out.append("'")
                i += 1
                continue
            end = literal.end()
        out.append(blank(text[i:end]))
        i = end
    result = "".join(out)
    RUST_CODE_CACHE[text] = result
    return result


def inline_modules(code):
    import bisect
    marks, stack, depth = [0], [], 0
    states = [()]
    opening = {match.end() - 1: match.group(1) for match in re.finditer(r"\bmod\s+(\w+)\s*\{", code)}
    for index, c in enumerate(code):
        if c == "{":
            depth += 1
            if index in opening:
                stack.append((opening[index], depth))
                marks.append(index); states.append(tuple(name for name, _ in stack))
        elif c == "}":
            if stack and stack[-1][1] == depth:
                stack.pop()
                marks.append(index); states.append(tuple(name for name, _ in stack))
            depth -= 1
    return lambda offset: list(states[bisect.bisect_right(marks, offset) - 1])


def impl_targets(header, text, workspace):
    header = re.split(r"\bwhere\b", header)[0].strip().replace("->", "  ")
    parameters = set()
    if header.startswith("<"):
        depth, start = 0, 1
        for index, c in enumerate(header):
            if depth == 1 and c in ",>":
                parameter = re.match(r"\s*(?:const\s+)?([A-Za-z_]\w*)", header[start:index])
                if parameter:
                    parameters.add(parameter.group(1))
                start = index + 1
            depth += c == "<"
            depth -= c == ">"
            if depth == 0:
                header = header[index + 1:]
                break
    names = set()
    for part in re.split(r"\bfor\b", header, maxsplit=1):
        path = re.sub(r"<.*", "", part.strip().lstrip("!&").replace("dyn ", "").strip(), flags=re.DOTALL).strip()
        segments = [segment for segment in path.split("::") if segment]
        if not segments:
            continue
        head, last = segments[0], segments[-1]
        if not re.fullmatch(r"[A-Za-z_]\w*", last):
            names.update(set(re.findall(r"[A-Za-z_]\w*", part)) - {"mut", "const"})
            continue
        if len(segments) > 1 and head not in ("crate", "self", "super") and head not in workspace:
            continue
        if len(segments) == 1:
            if re.search(rf"\b(?:struct|enum|trait|union)\s+{last}\b", text):
                continue
            imported = re.search(rf"\buse\s+(\w+)[^;]*\b{last}\b[^;]*;", text)
            if imported and imported.group(1) not in ("crate", "self", "super", *workspace):
                continue
        names.add(last)
    return names - parameters


def narrow_rust_sources(sources, scope):
    crates = {path.parent.name.replace("-", "_"): path.parent for path in (ROOT / "crates").glob("*/Cargo.toml")}
    def scoped(path):
        return any(path == prefix or (prefix.endswith("/") and path.startswith(prefix)) for prefix in scope)
    rust = [path for path in sources if path.startswith("crates/") and path.endswith(".rs")]
    entry = [path for path in rust if "/examples/" in path or "/src/bin/" in path or path.endswith("/src/main.rs")]
    inner = [path for path in rust if scoped(path)]
    outside = [path for path in rust if path not in inner and path not in entry
               and not path.endswith("/src/lib.rs")]

    def module_file(crate_dir, parts):
        for count in range(len(parts), 0, -1):
            for candidate in (crate_dir / "src" / Path(*parts[:count - 1]) / f"{parts[count - 1]}.rs",
                              crate_dir / "src" / Path(*parts[:count]) / "mod.rs"):
                if candidate.is_file():
                    return candidate.relative_to(ROOT).as_posix(), count
        return None, 0

    def module_path(path):
        parts = list(PurePosixPath(path).parts[3:-1] + (PurePosixPath(path).with_suffix("").name,))
        return parts[:-1] if parts[-1] in ("mod", "lib") else parts

    defined_types, used_macros, kept_modules = set(), set(), {}
    for path in inner + entry:
        text = rust_code((ROOT / path).read_text(encoding="utf-8", errors="replace"))
        crate_name = PurePosixPath(path).parts[1].replace("-", "_")
        nesting = inline_modules(text)
        if re.search(r"\binclude!\s*\(", text):
            return sources, f"{path} includes source"
        used_macros.update(RUST_MACRO_USE.findall(text))
        if path in inner:
            defined_types.update(RUST_TYPE.findall(text))
            kept_modules.setdefault(crate_name, set()).add(module_path(path)[0] if module_path(path) else "")
            for line in text.splitlines():
                declared = MODULE_DECLARATION.match(line)
                if declared:
                    child, _ = module_file(crates[crate_name], module_path(path) + [declared.group(1)])
                    if child and not scoped(child):
                        return sources, f"{path} declares module {declared.group(1)} outside the scope"
        for match in RUST_PATH.finditer(text):
            head, rest = match.groups()
            parts = rest.split("::")
            if head in crates:
                base, crate_name = [], head
            elif head.startswith(("crate", "self", "super")) and path in inner:
                own, crate_name = module_path(path) + nesting(match.start()), PurePosixPath(path).parts[1].replace("-", "_")
                ups = head.count("super")
                if head != "crate" and ups <= len(nesting(match.start())):
                    continue
                base = [] if head == "crate" else own[:len(own) - ups]
            else:
                continue
            target, covered = module_file(crates[crate_name], base + parts)
            if target is None:
                root = crates[crate_name] / "src" / "lib.rs"
                item = (base + parts)[0]
                defined_in_root = root.is_file() and re.search(
                    rf"^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const|static|fn|struct|enum|type|trait|union)\s+{item}\b",
                    root.read_text(encoding="utf-8"), re.MULTILINE)
                if not defined_in_root:
                    return sources, f"{path} names {head}::{rest}, which the crate root doesn't define itself"
                continue
            if not scoped(target):
                return sources, f"{path} names {head}::{rest} ({target})"
    for path in outside:
        text = rust_code((ROOT / path).read_text(encoding="utf-8", errors="replace"))
        defined = set(RUST_MACRO_DEFINITION.findall(text)) & used_macros
        if defined:
            return sources, f"the scope uses macro {sorted(defined)[0]}! defined in {path}"
        for header in re.findall(r"^\s*(?:unsafe\s+)?impl\b([^;{]*)", text, re.MULTILINE):
            touched = impl_targets(header, text, set(crates)) & defined_types
            if touched:
                return sources, f"{path} implements something for {sorted(touched)[0]}, defined in the scope"
    kept = [path for path in sources if path not in outside and not path.endswith("/src/lib.rs")]
    for path in sources:
        if path.endswith("/src/lib.rs"):
            modules = kept_modules.get(PurePosixPath(path).parts[1].replace("-", "_")) or {"-"}
            kept.append(f"{path}{KEEP}{','.join(sorted(modules))}")
    return sorted(kept), None


PROBE_SOURCE = re.compile(r"/examples/probe/(?:(support)/)?(\w+)\.rs$")
PROBE_TARGETS = {"vise-client": "client_probe", "vise-protocol": "protocol_probe", "vise-world": "world_probe"}
OLD_PROBE_SOURCE = re.compile(r"crates/([\w-]+)/examples/probe[/.]")


def probes_run(commands):
    names = set()
    for arguments in commands:
        program = Path(arguments[0]).name.removesuffix(".exe") if arguments else ""
        if program in PROBE_TARGETS.values() and len(arguments) > 1:
            names.add(arguments[1])
        elif program == "cargo" and "--" in arguments and len(arguments) > arguments.index("--") + 1 \
                and any(kind == "example" and name in PROBE_TARGETS.values() for kind, name, *_ in artifacts_of(arguments)):
            names.add(arguments[arguments.index("--") + 1])
    return names


def recorded_artifacts(record):
    artifacts = []
    for artifact in record.get("artifacts", []):
        if artifact[:2] != ["example", "probe"]:
            artifacts.append(artifact)
            continue
        selection = artifact[3] if len(artifact) > 3 else []
        crates = [value for flag, value in zip(selection, selection[1:]) if flag == "-p" and value in PROBE_TARGETS] \
            or sorted({match.group(1) for path in record.get("dependencies", [])
                       if (match := OLD_PROBE_SOURCE.match(path)) and match.group(1) in PROBE_TARGETS})
        artifacts += [["example", PROBE_TARGETS[crate], *artifact[2:]] for crate in crates] or [artifact]
    return artifacts


def only_probes_run(paths, names):
    if not names:
        return paths
    names, used = {*names, "dispatch"}, ""
    while True:
        ran = [path for path in paths if (match := PROBE_SOURCE.search(path)) and not match.group(1) and match.group(2) in names]
        used = "".join((ROOT / path).read_text(encoding="utf-8", errors="replace") for path in ran)
        included = {*re.findall(r'include!\("(\w+)\.rs"\)', used), *re.findall(r"\bcrate::(\w+)", used)} - names
        if not included:
            break
        names |= included
    return [path for path in paths if not (match := PROBE_SOURCE.search(path))
            or (f"support::{match.group(2)}" in used if match.group(1) else match.group(2) in names)]


def dependencies(trace, scope=None):
    found = set(trace["modules"])
    found.update(trace["read"])
    artifacts = []
    for arguments in trace["commands"]:
        program = Path(arguments[0]).name if arguments else ""
        if program == "cargo" and ("build" in arguments or "run" in arguments):
            artifacts += artifacts_of(arguments)
        if arguments and inside(arguments[0], TARGET):
            found.update(dep_info(Path(arguments[0])))
        if program == "javac":
            found.update(java_sources(arguments))
        for argument in arguments[1:]:
            if isinstance(argument, str) and os.path.isfile(argument):
                found.add(argument)
    for kind, name, profile, *_ in artifacts:
        if name:
            found.update(dep_info(artifact_path(kind, name, profile)))
    kept = set()
    for path in found:
        path = Path(path) if os.path.isabs(path) else ROOT / path
        if inside(path, ROOT) and not inside(path, LOCAL) and not inside(path, ROOT / "target") and path.is_file() \
                and "__pycache__" not in path.parts:
            kept.add(path.resolve().relative_to(ROOT).as_posix())
    unique = []
    for artifact in artifacts:
        if artifact not in unique:
            unique.append(artifact)
    kept = only_probes_run(sorted(kept), probes_run(trace["commands"]))
    kept, note = narrow_rust_sources(kept, scope) if scope else (kept, None)
    if note:
        print(f"note: PARITY_RUST_SCOPE not applied: {note}", flush=True)
    return kept, unique


def java_sources(arguments):
    sources = [argument for argument in arguments if str(argument).endswith(".java")]
    if "-d" in arguments and "-sourcepath" in arguments:
        output = Path(arguments[arguments.index("-d") + 1])
        source_roots = arguments[arguments.index("-sourcepath") + 1].split(os.pathsep)
        for compiled in output.rglob("*.class") if output.is_dir() else []:
            relative = compiled.relative_to(output).with_suffix("")
            top = relative.with_name(relative.name.split("$")[0] + ".java")
            for root in source_roots:
                if (Path(root) / top).is_file():
                    sources.append(str(Path(root) / top))
    return sources


def run_environment(reuse_oracles=False):
    environment = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), PYTHONDONTWRITEBYTECODE="1", VISE_NO_AUDIO="1", **PROFILE)
    environment.pop("CARGO_BUILD_JOBS", None)
    if reuse_oracles:
        SHIM.mkdir(parents=True, exist_ok=True)
        for name in ("java", "javac"):
            link = SHIM / name
            if not link.is_symlink():
                link.symlink_to(ROOT / "tools" / "parity_shim.py")
        environment.update(PARITY_SHIM_REAL_PATH=os.environ.get("PATH", ""), PARITY_SHIM_CACHE=str(STATE_DIR / "oracle-cache"),
                           PATH=f"{SHIM}{os.pathsep}{os.environ.get('PATH', '')}")
    return environment


def default_build_jobs():
    if os.environ.get("PARITY_BUILD_JOBS"):
        return int(os.environ["PARITY_BUILD_JOBS"])
    jobs = os.cpu_count() or 8
    if os.environ.get("FLEET_LOAD_JOBS", "").isdigit():
        jobs = min(jobs, max(1, int(os.environ["FLEET_LOAD_JOBS"])))
    return jobs


def target_packages():
    result = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT, capture_output=True,
                            text=True)
    if result.returncode:
        return {}
    return {(kind, target["name"]): package["name"] for package in json.loads(result.stdout)["packages"]
            for target in package["targets"] for kind in target["kind"] if kind in ("example", "bin")}


def prebuild_commands(artifacts, packages=None):
    groups = {}
    for artifact in artifacts:
        if packages and artifact[1] and tuple(artifact[:2]) not in packages:
            continue
        if len(artifact) == 3 and (packages or {}).get(tuple(artifact[:2])):
            kind, name, profile = artifact
            artifact = [kind, name, profile, ["-p", packages[(kind, name)]], kind == "example"]
        if len(artifact) < 5:
            continue
        kind, name, profile, selection, examples = artifact
        targets = groups.setdefault((profile, tuple(selection), examples), [])
        target = [f"--{kind}", name] if name else [f"--{kind}"]
        if target not in targets:
            targets.append(target)
    return [["cargo", "build", "--locked", "--profile", profile, *selection, *(part for target in targets for part in target)]
            for (profile, selection, _), targets in sorted(groups.items())]


def prebuild(artifacts, jobs):
    environment = run_environment()
    environment["CARGO_BUILD_JOBS"] = str(jobs)
    errors = ""
    packages = target_packages()
    for command in prebuild_commands(artifacts, packages):
        result = subprocess.run(command, cwd=ROOT, env=environment, capture_output=True, text=True)
        if result.returncode:
            errors += result.stderr
    return not errors, errors


LARGE_OUTPUT = 64 << 20
KEPT_ORACLE_OUTPUT = 256 << 20
OWN_DIRECTORIES = {"parity-target", "parity-runner", "rea", "logs", "gate"}


def outputs_listing(name, fresh=False):
    path = STATE_DIR / "oracle-outputs" / f"{name}.txt"
    path.parent.mkdir(parents=True, exist_ok=True)
    if fresh:
        path.write_text("")
    return path


def clear_large_outputs(name, trace):
    local = LOCAL.resolve()
    keep = {os.path.realpath(line) for line in outputs_listing(name).read_text(encoding="utf-8").splitlines() if line}
    own = name.split("#")[0].replace("_", "-")
    directories = set()
    for command in trace["commands"]:
        for argument in command[1:]:
            path = Path(os.path.realpath(argument if os.path.isabs(argument) else ROOT / argument))
            try:
                top = path.relative_to(local).parts[0]
            except (ValueError, IndexError):
                continue
            if top not in OWN_DIRECTORIES and (top.startswith(own) or top == f"rust-{own}-parity"):
                directories.add(local / top)
    for directory in directories:
        for file in (directory.rglob("*") if directory.is_dir() else []):
            try:
                size = file.stat().st_size if file.is_file() and not file.is_symlink() else 0
                if size >= LARGE_OUTPUT and (str(file) not in keep or size > KEPT_ORACLE_OUTPUT):
                    file.unlink()
            except OSError:
                pass


def publish(name, record):
    if record["status"] != "pass" or "key" not in record:
        return
    for path, data in ((SHARED / "passes" / name / f"{record['key']}.json", record),
                       (SHARED / "dependencies" / f"{name}.json", {k: record[k] for k in ("dependencies", "artifacts")})):
        path.parent.mkdir(parents=True, exist_ok=True)
        temporary = path.with_name(f"{path.name}.{os.getpid()}.tmp")
        temporary.write_text(json.dumps(data))
        temporary.replace(path)


def machine_slot():
    import fcntl
    slots = SHARED / "slots"
    slots.mkdir(parents=True, exist_ok=True)
    for index in range(MACHINE_JOBS):
        handle = open(slots / f"{index}.lock", "w")
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return handle
        except OSError:
            handle.close()
    return None


def memory_available():
    meminfo = Path("/proc/meminfo")
    if meminfo.exists():
        for line in meminfo.read_text(encoding="utf-8").splitlines():
            if line.startswith("MemAvailable:"):
                return int(line.split()[1]) * 1024
    try:
        text = subprocess.run(["vm_stat"], capture_output=True, text=True).stdout
        page = int(re.search(r"page size of (\d+) bytes", text).group(1))
        pages = sum(int(re.search(rf"^{label}:\s+(\d+)", text, re.MULTILINE).group(1))
                    for label in ("Pages free", "Pages inactive", "Pages speculative", "Pages purgeable"))
        return pages * page
    except (OSError, AttributeError, ValueError):
        return 16 << 30


REMOTE_LOCAL_JOBS = 3
REMOTE_LOCAL_PEAK = 800 << 20
MEMORY_RESERVE = 2 << 30


def default_local_jobs(remote=False):
    if os.environ.get("PARITY_LOCAL_JOBS"):
        return int(os.environ["PARITY_LOCAL_JOBS"])
    jobs = REMOTE_LOCAL_JOBS if remote else max(1, (os.cpu_count() or 4) // 3)
    if os.environ.get("FLEET_LOAD_JOBS", "").isdigit():
        jobs = min(jobs, max(1, int(os.environ["FLEET_LOAD_JOBS"])))
    return jobs


def outcome(name, script, passed, elapsed, trace_path, hasher=None):
    record = {"status": "pass" if passed else "fail", "seconds": round(elapsed, 1),
              "finished": time.strftime("%Y-%m-%dT%H:%M:%S"), "host": HOST}
    try:
        trace = json.loads(Path(trace_path).read_text(encoding="utf-8"))
    except (OSError, ValueError):
        trace = None
    if trace:
        record["dependencies"], record["artifacts"] = dependencies(trace, harness_settings(script).get("PARITY_RUST_SCOPE"))
        record["peak_bytes"] = trace["peak_bytes"]
        if passed:
            clear_large_outputs(name, trace)
        if hasher is not None:
            record["hashes"] = {path: hasher(ROOT / path) for path in record["dependencies"]}
    else:
        record["dependencies"], record["artifacts"] = [], []
    return record


class LocalExecutor:

    def __init__(self, jobs, memory_budget, timeout, max_peak=None):
        self.name, self.jobs, self.memory_budget, self.timeout = HOST, jobs, memory_budget, timeout
        self.max_peak = max_peak
        self.running = {}
        self.solo = set()
        self.dead = False
        self.available, self.measured = 0, 0.0
        (STATE_DIR / "logs").mkdir(parents=True, exist_ok=True)
        (STATE_DIR / "traces").mkdir(parents=True, exist_ok=True)

    def free(self):
        if any(harness_of(label) in self.solo for label in self.running):
            return 0
        return self.jobs - len(self.running)

    def accepts(self, name):
        return name not in self.solo or not self.running

    def fits(self, expected, only_here=True):
        if self.max_peak is not None and expected > self.max_peak and not only_here:
            return False
        if time.monotonic() - self.measured > 1:
            self.available, self.measured = memory_available(), time.monotonic()
        if not self.running and (self.max_peak is None or only_here):
            return True
        return sum(item["expected"] for item in self.running.values()) + expected <= self.memory_budget \
            and self.available - expected >= MEMORY_RESERVE

    def start(self, name, script, reuse, expected, shard=None):
        self.measured = 0.0
        slot = machine_slot()
        if slot is None:
            return False
        trace = STATE_DIR / "traces" / f"{name}.json"
        trace.unlink(missing_ok=True)
        log = (STATE_DIR / "logs" / f"{name}.log").open("w")
        environment = run_environment(reuse)
        game_home = STATE_DIR / "game-home" / name
        shutil.rmtree(game_home, ignore_errors=True)
        game_home.mkdir(parents=True)
        environment.setdefault("VISE_HOME", str(game_home))
        environment["PARITY_SHIM_OUTPUTS"] = str(outputs_listing(name, fresh=True))
        environment.pop("PARITY_SHARD", None)
        if shard:
            environment["PARITY_SHARD"] = shard
        process = subprocess.Popen([sys.executable, str(Path(__file__)), "--trace", str(trace), str(script)],
                                   cwd=ROOT, env=environment, stdin=subprocess.DEVNULL, stdout=log,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        self.running[name] = {"process": process, "started": time.monotonic(), "log": log, "expected": expected,
                              "slot": slot, "script": script}
        return True

    def poll(self):
        finished = []
        for name, item in list(self.running.items()):
            process, elapsed = item["process"], time.monotonic() - item["started"]
            if process.poll() is None and elapsed < self.timeout:
                continue
            if process.poll() is None:
                os.killpg(process.pid, 9)
                process.wait()
                print(f"--- {name}: timed out after {self.timeout}s", file=item["log"])
            item["log"].close()
            item["slot"].close()
            del self.running[name]
            finished.append((name, item["script"], process.returncode == 0, elapsed, STATE_DIR / "traces" / f"{name}.json"))
        return finished

    def stop(self):
        for item in self.running.values():
            try:
                os.killpg(item["process"].pid, 9)
            except ProcessLookupError:
                pass


def host_config():
    path = ROOT / "tools" / "parity_hosts.json"
    return json.loads(path.read_text(encoding="utf-8")) if path.exists() else {"default": [], "hosts": {}, "local_only": {}}


class RemoteExecutor:

    def __init__(self, name, config, artifacts, local_only, timeout):
        import threading
        self.name, self.config, self.local_only, self.timeout = name, config, set(local_only), timeout
        self.jobs, self.memory_budget, self.ready, self.dead = 0, 0, False, False
        self.running, self.events, self.backoff = {}, [], 0.0
        self.lock = threading.Lock()
        self.process = None
        self.mirror = f"{config['root']}/remote/{HOST}{str(ROOT).replace('/', '-')}"
        threading.Thread(target=self._connect, args=(artifacts,), daemon=True).start()

    def _log(self, text):
        print(f"[{self.name}] {text}", flush=True)

    def _connect(self, artifacts):
        import shlex
        ssh = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=10", "-o", "ServerAliveInterval=30", self.config["ssh"]]
        started = time.monotonic()
        shared = f"{self.config['root']}/shared-local"
        mkdir = subprocess.run([*ssh, f"mkdir -p {shlex.quote(self.mirror)}"], capture_output=True, text=True)
        if mkdir.returncode:
            self._log(f"unreachable, running everything here: {mkdir.stderr.strip()[-300:]}")
            self.dead = True
            return
        sync = subprocess.run(
            ["rsync", "-a", "--delete", "--filter=:- .gitignore", "--exclude=/.git", "--exclude=/.local", "--exclude=/target",
             "--exclude=__pycache__", "-e", " ".join(ssh[:-1]), f"--rsync-path={self.config.get('rsync', 'rsync')}",
             f"{ROOT}/", f"{self.config['ssh']}:{self.mirror}/"], capture_output=True, text=True)
        if sync.returncode:
            self._log(f"sync failed, running everything here: {sync.stderr.strip()[-300:]}")
            self.dead = True
            return
        environment = " ".join(f"{key}={shlex.quote(str(value))}" for key, value in self.config.get("environment", {}).items())
        command = (f"{self.config.get('setup', 'true')} && cd {shlex.quote(self.mirror)} && "
                   f"exec nice -n {int(self.config.get('nice', 0))} env PARITY_MACHINE_JOBS={self.config.get('jobs', 8)} "
                   f"{environment} {self.config.get('python', 'python3')} tools/parity.py --worker {shlex.quote(shared)}")
        self.process = subprocess.Popen([*ssh, f"{self.config.get('shell', 'sh -c')} {shlex.quote(command)}"],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, bufsize=1)
        self._send({"prebuild": artifacts, "timeout": self.timeout})
        self._log(f"synced in {time.monotonic() - started:.0f}s, building probes...")
        for line in self.process.stdout:
            try:
                message = json.loads(line)
            except ValueError:
                continue
            with self.lock:
                if "hello" in message:
                    hello = message["hello"]
                    self.jobs = min(self.config.get("jobs", hello["jobs"]), hello["jobs"])
                    self.memory_budget = hello["memory"]
                    self.ready = True
                    if not hello["built"]:
                        self._log("probe build failed; harnesses will report their own build errors:\n" + hello["errors"][-2000:])
                    self._log(f"ready after {time.monotonic() - started:.0f}s: {self.jobs} harnesses at once, "
                              f"{self.memory_budget / (1 << 30):.0f} GB")
                else:
                    self.events.append(message)
        with self.lock:
            self.dead = True

    def _send(self, message):
        try:
            self.process.stdin.write(json.dumps(message) + "\n")
            self.process.stdin.flush()
        except (OSError, ValueError, AttributeError):
            self.dead = True

    def free(self):
        if not self.ready or self.dead or time.monotonic() < self.backoff:
            return 0
        return self.jobs - len(self.running)

    def accepts(self, name):
        return base_name(name) not in self.local_only and base_name(name) not in self.config.get("exclude", {})

    def fits(self, expected, only_here=False):
        return not self.running or sum(self.running.values()) + expected <= self.memory_budget

    def start(self, name, script, reuse, expected, shard=None):
        self.running[name] = expected
        self._send({"run": name, "script": str(Path(script).relative_to(ROOT)), "reuse": reuse, "expected": expected,
                    "shard": shard})
        return True

    def poll(self):
        with self.lock:
            events, self.events = self.events, []
            dead = self.dead
        finished = []
        for message in events:
            name = message.get("done") or message.get("rejected")
            if name not in self.running:
                continue
            del self.running[name]
            if "rejected" in message:
                self.backoff = time.monotonic() + 5
                finished.append((name, None))
            else:
                (STATE_DIR / "logs").mkdir(parents=True, exist_ok=True)
                (STATE_DIR / "logs" / f"{name}.log").write_text(message.get("log", ""))
                finished.append((name, message["record"]))
        if dead and self.running:
            self._log("connection lost; its harnesses run elsewhere")
            finished += [(name, None) for name in self.running]
            self.running = {}
        return finished

    def stop(self):
        if self.process and self.process.poll() is None:
            try:
                self.process.stdin.close()
            except OSError:
                pass
            try:
                self.process.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.process.kill()


def key_from(record, hashes, environment):
    return key(record, lambda path: hashes.get(str(Path(path).relative_to(ROOT)), "missing"), environment)


def harness_of(label):
    return label.split("~")[0]


def keep_first_log(label):
    log = STATE_DIR / "logs" / f"{label}.log"
    kept = log.with_name(f"{label}.first-run.log")
    try:
        log.replace(kept)
    except OSError:
        return log
    return kept


def run(selected, state, hasher, environment_digest, executors, reuse_oracles, rounds=None, known_flaky=None):
    records = state["harnesses"]
    rounds = rounds or {}
    known_flaky = known_flaky or {}
    second_chance = set()
    queue = sorted((name if index == 1 else f"{name}~{index}" for name in selected for index in range(1, rounds.get(name, 1) + 1)),
                   key=lambda label: (-records.get(harness_of(label), {}).get("seconds", 1e9), label))
    results, retried, busy, done = {}, set(), {}, {}
    home = {}
    started_all = time.monotonic()
    local = executors[-1]
    while queue or busy:
        for executor in executors:
            while queue and executor.free() > 0:
                here = {harness_of(label) for label, (other, _) in busy.items() if other is executor}
                pick = next((label for label in queue if harness_of(label) not in here and executor.accepts(harness_of(label))
                             and (label == harness_of(label) or home.get(harness_of(label), executor) in (executor, None)
                                  or home[harness_of(label)].dead)
                             and executor.fits(records.get(harness_of(label), {}).get("peak_bytes", DEFAULT_MEMORY),
                                               not any(other.accepts(harness_of(label)) and not other.dead
                                                       for other in executors[:-1]))), None)
                if pick is None:
                    break
                name = harness_of(pick)
                reuse = reuse_oracles and records.get(name, {}).get("oracle_reuse", True) and pick not in retried \
                    and name not in second_chance
                if not executor.start(pick, selected[name], reuse, records.get(name, {}).get("peak_bytes", DEFAULT_MEMORY),
                                      SHARDS.get(name)):
                    break
                queue.remove(pick)
                busy[pick] = (executor, reuse)
                home.setdefault(name, executor)
        if executors[:-1] and all(executor.dead for executor in executors[:-1]) and local.max_peak is not None:
            local.max_peak, local.jobs = None, max(local.jobs, default_local_jobs())
        time.sleep(0.2)
        finished = []
        for executor in executors:
            if isinstance(executor, LocalExecutor):
                for label, script, passed, elapsed, trace in executor.poll():
                    finished.append((label, outcome(label, script, passed, elapsed, trace)))
            else:
                finished += executor.poll()
        for label, record in finished:
            executor, reused = busy.pop(label)
            name = harness_of(label)
            if record is None:
                queue.insert(0, label)
                continue
            passed = record["status"] == "pass"
            flaky_retry = rounds.get(name, 1) == 1 and base_name(name) in known_flaky
            if not passed and reused and label == name and records.get(name, {}).get("status") != "fail" \
                    and not flaky_retry:
                retried.add(label)
                queue.insert(0, label)
                print(f"retry {label} without reused oracle results", flush=True)
                continue
            record["oracle_reuse"] = records.get(name, {}).get("oracle_reuse", True) and not (passed and label in retried)
            hashes = record.pop("hashes", None)
            if record["dependencies"]:
                record["key"] = key_from(record, hashes, environment_digest) if hashes is not None else \
                    key(record, hasher, environment_digest)
            where = "" if executor is local else f" on {executor.name}"
            tail = "" if passed else f"  ({STATE_DIR / 'logs' / (label + '.log')})"
            print(f"{'PASS' if passed else 'FAIL'} {label} {record['seconds']:.0f}s{where}{tail}", flush=True)
            done.setdefault(name, []).append(record)
            if len(done[name]) < rounds.get(name, 1):
                continue
            runs = done.pop(name)
            if runs[0]["status"] != "pass" and flaky_retry and name not in second_chance:
                second_chance.add(name)
                first_log = keep_first_log(label)
                print(f"RETRIED {name}: failed, and it is a known flaky harness ({known_flaky[base_name(name)]}): "
                      f"running it once more (first run's log: {first_log})", flush=True)
                queue.insert(0, label)
                continue
            record = dict(runs[0], status="pass" if all(run["status"] == "pass" for run in runs) else "fail",
                          rounds=len(runs))
            if name in second_chance:
                record["retried"] = "known flaky"
            elif label in retried:
                record["retried"] = "reused oracle results"
            if len({run["status"] for run in runs}) > 1:
                record["flaky"] = [run["status"] for run in runs]
                print(f"FLAKY {name}: {', '.join(record['flaky'])} in {len(runs)} runs", flush=True)
            records[name] = record
            results[name] = record
            save_state(state)
            publish(name, record)
    return results, time.monotonic() - started_all


def exit_on_termination():
    for name in ("SIGTERM", "SIGHUP"):
        if hasattr(signal, name):
            signal.signal(getattr(signal, name), lambda received, _frame: sys.exit(128 + received))


def worker_main(shared_local):
    import queue as queues
    import threading
    global SHARED
    LOCAL.mkdir(exist_ok=True)
    shared_local = Path.home() / Path(shared_local).expanduser()
    for entry in sorted(shared_local.iterdir()) if shared_local.is_dir() else []:
        if entry.is_dir() and entry.name not in {"parity-shared", "datafix"} and not (LOCAL / entry.name).exists():
            (LOCAL / entry.name).symlink_to(entry)
    if not os.environ.get("PARITY_SHARED"):
        SHARED = (LOCAL / "minecraft").resolve().parent / "parity-shared"
    inbox = queues.Queue()

    def read():
        for line in sys.stdin:
            inbox.put(json.loads(line))
        inbox.put(None)

    threading.Thread(target=read, daemon=True).start()
    channel = sys.stdout
    sys.stdout = sys.stderr

    def send(message):
        channel.write(json.dumps(message) + "\n")
        channel.flush()

    first = inbox.get()
    if first is None:
        return 0
    state = load_state()
    hasher = Hasher(state["hashes"])
    built, errors = prebuild(first["prebuild"], default_build_jobs()) if first["prebuild"] else (True, "")
    executor = LocalExecutor(MACHINE_JOBS, int(memory_available() * 0.75), first["timeout"])
    send({"hello": {"jobs": MACHINE_JOBS, "memory": executor.memory_budget, "built": built, "errors": errors[-4000:]}})
    try:
        return serve(executor, inbox, queues.Empty, send, state, hasher)
    finally:
        executor.stop()


def serve(executor, inbox, empty, send, state, hasher):
    while True:
        try:
            message = inbox.get(timeout=0.2)
        except empty:
            message = False
        if message is None:
            save_state(state)
            return 0
        if message:
            reserve = message["expected"] + (4 << 30)
            if (executor.running and memory_available() < reserve) or \
                    not executor.start(message["run"], ROOT / message["script"], message["reuse"], message["expected"],
                                       message.get("shard")):
                send({"rejected": message["run"]})
        for name, script, passed, elapsed, trace in executor.poll():
            record = outcome(name, script, passed, elapsed, trace, hasher)
            log = (STATE_DIR / "logs" / f"{name}.log").read_bytes()[-(8 << 20):].decode(errors="replace")
            send({"done": name, "record": record, "log": log})
            save_state(state)


def stale(name, state, hasher, environment):
    record = state["harnesses"].get(name)
    if record and record.get("status") == "pass" and "key" in record and key(record, hasher, environment) == record["key"]:
        return False
    candidates = [record] if record and record.get("dependencies") else []
    template = SHARED / "dependencies" / f"{name}.json"
    if template.exists():
        candidates.append(json.loads(template.read_text(encoding="utf-8")))
    for candidate in candidates:
        shared = SHARED / "passes" / name / f"{key(candidate, hasher, environment)}.json"
        if shared.exists():
            state["harnesses"][name] = dict(json.loads(shared.read_text(encoding="utf-8")), shared=True)
            return False
    return True


def trace_main(output, script, arguments):
    import atexit
    import resource
    import runpy
    read, commands = set(), []

    def audit(event, details):
        if event == "open":
            path, mode = details[0], details[1]
            if isinstance(path, (str, bytes, os.PathLike)) and (mode is None or "r" in str(mode) and "+" not in str(mode)):
                read.add(os.fsdecode(path))
        elif event == "subprocess.Popen":
            arguments_ = details[1]
            arguments_ = [arguments_] if isinstance(arguments_, (str, bytes, os.PathLike)) else list(arguments_ or [])
            commands.append([os.fsdecode(a) if isinstance(a, (bytes, os.PathLike)) else str(a) for a in arguments_])

    def write():
        modules = [getattr(module, "__file__", None) for module in list(sys.modules.values())]
        peak = max(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss, resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
        peak //= 1024 if sys.platform == "darwin" else 1
        Path(output).write_text(json.dumps({
            "modules": sorted({m for m in modules if m and inside(m, ROOT)}),
            "read": sorted(path for path in read if os.path.isabs(path) or not path.startswith("<")),
            "commands": commands,
            "peak_bytes": peak * 1024,
        }))

    atexit.register(write)
    sys.addaudithook(audit)
    sys.argv = [script, *arguments]
    sys.path.insert(0, str(Path(script).parent))
    runpy.run_path(script, run_name="__main__")


SHARED_INPUTS = {"research-lock.json", "Cargo.lock"}
MANIFEST_SECTION_KEPT = re.compile(r"\[\[?(?:package|lib|bin|example|test|bench|lints|badges)\b[^\]]*\]\]?$")
WORKSPACE_CRATE = r"(vise[\w-]*|cargo-vise-mod|xtask)"
WORKSPACE_LINE = re.compile(WORKSPACE_CRATE + r'\s*=\s*(?:\{(?![^}]*\bfeatures\b)[^}]*\b(?:path|workspace)\s*=[^}]*\}|true)'
                            r'|' + WORKSPACE_CRATE + r'\.workspace\s*=\s*true|"crates/[^"]+",?')
WORKSPACE_FEATURES = re.compile(WORKSPACE_CRATE + r'\s*=\s*\{(?=[^}]*\b(?:path|workspace)\s*=)(?![^}]*default-features)'
                                r'[^}]*\bfeatures\s*=\s*\[([^\]]*)\][^}]*\}')


def git_show(base, path):
    result = subprocess.run(["git", "show", f"{base}:{path}"], cwd=ROOT, capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else None


def manifest_lines(text):
    lines, section = set(), ""
    name = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', text or "", re.M | re.S)
    for raw in (text or "").splitlines():
        line = re.sub(r"\s+", " ", raw.split("#", 1)[0]).strip()
        if not line:
            continue
        if line.startswith("["):
            section = f"[features] of {name.group(1) if name else '?'}" if line == "[features]" else line
            continue
        if MANIFEST_SECTION_KEPT.match(section) or WORKSPACE_LINE.fullmatch(line) or \
                section == "[workspace]" and (line.startswith("members =") or line.startswith('"') or line == "]"):
            continue
        lines.add((section, line))
    return lines


def lock_packages(text):
    return {block.strip() for block in (text or "").split("[[package]]") if "\nsource = " in block}


def workspace_only_manifests(changed, base):
    manifests = sorted(path for path in changed if path == "Cargo.lock" or path.endswith("Cargo.toml"))
    if not manifests or base is None:
        return set()
    if "Cargo.lock" in manifests and \
            lock_packages(git_show(base, "Cargo.lock")) != lock_packages((ROOT / "Cargo.lock").read_text(encoding="utf-8")):
        return set()
    tomls = [path for path in manifests if path != "Cargo.lock"]
    before = set().union(*(manifest_lines(git_show(base, path)) for path in tomls)) if tomls else set()
    after = set().union(*(manifest_lines((ROOT / path).read_text(encoding="utf-8") if (ROOT / path).exists() else None)
                          for path in tomls)) if tomls else set()
    gated = set()
    for section, line in before ^ after:
        wanted = WORKSPACE_FEATURES.fullmatch(line)
        if wanted:
            gated |= {(wanted.group(1), feature) for feature in re.findall(r'"([^"]+)"', wanted.group(2))}
        elif section.startswith("[features] of ") and re.fullmatch(r'[\w-]+ = \[(?: ?"[\w-]+",?)*\s*\]', line):
            gated.add((section[len("[features] of "):], line.split(" ", 1)[0]))
        else:
            return set()
    return set(manifests) if all(feature_sources_changed(crate, feature, changed) for crate, feature in gated) else set()


def feature_sources_changed(crate, feature, changed):
    listed = subprocess.run(["git", "ls-files", "*Cargo.toml"], cwd=ROOT, capture_output=True, text=True)
    manifests = [ROOT / path for path in listed.stdout.split()] if listed.returncode == 0 else list(ROOT.rglob("Cargo.toml"))
    for manifest in manifests:
        text = manifest.read_text(encoding="utf-8") if manifest.exists() else ""
        if re.search(r'^\[package\][^\[]*?^name\s*=\s*"' + re.escape(crate) + '"', text, re.M | re.S):
            break
    else:
        return False
    table = re.search(r"^\[features\]\n(.*?)(?=^\[|\Z)", text, re.M | re.S)
    implies = {name: re.findall(r'"([^"]+)"', value) for name, value in
               re.findall(r'^([\w-]+)\s*=\s*\[([^\]]*)\]', table.group(1) if table else "", re.M)}
    names, todo = set(), [feature]
    while todo:
        name = todo.pop()
        if name not in names:
            names.add(name)
            todo += [implied for implied in implies.get(name, []) if "/" not in implied and ":" not in implied]
    named = re.compile(r'feature\s*=\s*"(?:' + "|".join(map(re.escape, sorted(names))) + r')"')
    return all(path.relative_to(ROOT).as_posix() in changed for path in manifest.parent.rglob("*.rs")
               if named.search(path.read_text(encoding="utf-8", errors="replace")))


def changed_files(ref):
    def git(*arguments):
        result = subprocess.run(["git", *arguments], cwd=ROOT, capture_output=True, text=True)
        return result.stdout.split("\n") if result.returncode == 0 else None
    base = git("merge-base", "HEAD", ref)
    if not base or not base[0]:
        return None
    return set(filter(None, git("diff", "--name-only", base[0]) or [])) | \
        set(filter(None, git("ls-files", "--others", "--exclude-standard") or []))


def canonical_dependency_path(path: str) -> str:
    prefix, separator, suffix = path.partition(KEEP)
    return prefix.replace("\\", "/") + separator + suffix


def changed_harnesses(found, records, ref):
    changed = changed_files(ref)
    if changed is None:
        print(f"note: no merge base with {ref}; the flake check repeats nothing", flush=True)
        return set()
    found_changed = set()
    for name, script in found.items():
        dependencies = map(canonical_dependency_path, records.get(name, {}).get("dependencies", []))
        specific = [path.split(KEEP)[0] for path in dependencies
                    if path.startswith("mods/vise/src/test/") or "/examples/" in path or "/src/bin/" in path]
        if Path(script).relative_to(ROOT).as_posix() in changed or any(path in changed for path in specific):
            found_changed.add(name)
    return found_changed


def affected_harnesses(found, records, changed, reach=True, base=None):
    if changed & SHARED_INPUTS:
        return set(found)
    graph, stop = None, frozenset()
    cache_path = (SHARED if SHARED.is_dir() else STATE_DIR) / "reach-cache.json"
    if reach and any(path.startswith("crates/") for path in changed):
        try:
            cache = json.loads(cache_path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            cache = {}
        graph = parity_reach.SourceGraph(ROOT, rust_code, cache)
        stop = frozenset(path for path in changed if parity_reach.is_workspace_rust(path)
                         and parity_reach.declarations_changed(ROOT, path, (), base, True))
    budget, started, over = float(os.environ.get("PARITY_REACH_BUDGET") or 180), time.monotonic(), False
    affected = set()
    for name, script in found.items():
        record = records.get(name) or {}
        dependencies = record.get("dependencies")
        template = SHARED / "dependencies" / f"{name}.json"
        if dependencies is None and template.exists():
            record = json.loads(template.read_text(encoding="utf-8"))
            dependencies = record.get("dependencies")
        if dependencies is None or Path(script).relative_to(ROOT).as_posix() in changed:
            affected.add(name)
            continue
        dependencies = [canonical_dependency_path(path) for path in dependencies]
        plain = {path.split(KEEP)[0] for path in dependencies}
        hit = (plain & changed) | parity_reach.unrecorded(ROOT, plain, changed)
        if not hit:
            continue
        if graph is None or any(not path.startswith("crates/") for path in hit):
            affected.add(name)
            continue
        if not over and time.monotonic() - started > budget:
            over = True
            print(f"note: choosing harnesses took over {budget:.0f} s (PARITY_REACH_BUDGET): the rest count every "
                  "source their probes link", flush=True)
        if over:
            affected.add(name)
            continue
        narrowed = parity_reach.narrow(dependencies, recorded_artifacts(dict(record, dependencies=dependencies)),
                                      graph, changed, stop)
        if any(path.split(KEEP)[0] in changed and (children is None or parity_reach.declarations_changed(
                ROOT, path.split(KEEP)[0], children, base, whole)) for path, (whole, children) in narrowed.items()):
            affected.add(name)
    if graph is not None:
        saved = graph.saved_cache()
        if len(cache) + len(saved) < 4 * max(len(saved), 1):
            saved = {**cache, **saved}
        cache_path.parent.mkdir(parents=True, exist_ok=True)
        temporary = cache_path.with_suffix(f".{os.getpid()}.tmp")
        temporary.write_text(json.dumps(saved), encoding="utf-8")
        temporary.replace(cache_path)
    return affected


def platform_skips(config, host=None):
    skips = {} if sys.platform == "linux" else dict(config.get("linux_only", {}))
    skips.update(config.get("hosts", {}).get(HOST if host is None else host, {}).get("exclude", {}))
    return skips


def known_flaky(config):
    return dict(config.get("known_flaky", {}))


def solo_harnesses(config):
    forced = os.environ.get("PARITY_SOLO")
    cpus = len(os.sched_getaffinity(0)) if hasattr(os, "sched_getaffinity") else os.cpu_count() or 1
    if forced == "0" or forced != "1" and cpus > SOLO_CPUS:
        return set()
    return set(config.get("solo", {}))


SOLO_CPUS = 4


def named_only(config):
    return dict(config.get("named_only", {}))


def report_retries(results, flaky):
    def why(name, record):
        if record["retried"] == "known flaky":
            return (f"known flaky: {flaky.get(base_name(name), '?')}; first run's log: "
                    f"{STATE_DIR / 'logs' / (name + '.first-run.log')}")
        return "its first run reused oracle results; it no longer reuses them" if record["status"] == "pass" else \
            "its first run reused oracle results"
    lines = [f"RETRIED {name}: failed, then {'passed' if record['status'] == 'pass' else 'failed again'} on its one "
             f"retry ({why(name, record)})" for name, record in sorted(results.items()) if record.get("retried")]
    for line in lines:
        print(line, flush=True)
    if lines and os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write("### Harnesses that failed once, then ran again\n" + "".join(f"- {line}\n" for line in lines))


def finished_run(failed):
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    (STATE_DIR / "last-run.json").write_text(json.dumps({"run": os.environ.get("PARITY_RUN_ID"), "failed": failed}))


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--trace":
        return trace_main(sys.argv[2], sys.argv[3], sys.argv[4:])
    exit_on_termination()
    if len(sys.argv) > 1 and sys.argv[1] == "--worker":
        return worker_main(sys.argv[2])
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("names", nargs="*", help="harness names, e.g. food or block_update")
    parser.add_argument("--all", action="store_true", help="run every harness from scratch, ignoring earlier passes")
    parser.add_argument("--fresh", action="store_true", help="run the vanilla oracles even if their inputs are unchanged")
    parser.add_argument("--failed", action="store_true", help="rerun the harnesses that failed last time")
    parser.add_argument("--list", action="store_true", help="print what would run, then stop")
    parser.add_argument("--status", action="store_true", help="print each harness's last result")
    parser.add_argument("-j", "--jobs", type=int,
                        help="harnesses at once on this machine (default: PARITY_LOCAL_JOBS, else 3 with other hosts or a "
                             "third of its threads, within $FLEET_LOAD_JOBS); runners here share PARITY_MACHINE_JOBS slots")
    parser.add_argument("--hosts", default=os.environ.get("PARITY_HOSTS"),
                        help="comma-separated machines from tools/parity_hosts.json to run harnesses on as well "
                             "(default: PARITY_HOSTS, else the file's default; '' for this machine only)")
    parser.add_argument("--timeout", type=int, default=3600, help="seconds per harness")
    parser.add_argument("--repeat", type=int, default=1, help="run each selected harness this many times; its runs "
                        "must agree (oracle results are reused after the first unless --fresh)")
    parser.add_argument("--flake-check", metavar="REF", help="harnesses new or changed since the merge base with REF "
                        "must pass --flake-rounds times in a row (tools/gate.py)")
    parser.add_argument("--flake-rounds", type=int, default=3, help="runs required by --flake-check (default 3)")
    parser.add_argument("--changed", metavar="REF", help="only harnesses the changes since the merge base with REF can "
                        "affect (and that have not passed with their current inputs): the pull-request check")
    parser.add_argument("--by-crate", action="store_true", help="with --changed: count every Rust source a probe "
                        "links, not only those it can reach (tools/parity_reach.py)")
    parser.add_argument("--changed-files", metavar="FILE", help="like --changed, for the changed paths listed in FILE, "
                        "one per line: for a copy without Git history (tools/pr_ready.py on another machine)")
    args = parser.parse_args()

    found = harnesses()
    state = load_state()
    hasher = Hasher(state["hashes"])
    environment = environment_key(hasher)
    records = state["harnesses"]
    args.names = [entry for name in args.names for entry in ([name] if name in found else
                  [shard for shard in found if base_name(shard) == name] or [name])]
    unknown = [name for name in args.names if name not in found]
    if unknown:
        parser.error("unknown harnesses: " + ", ".join(unknown))
    if args.status:
        for name in found:
            record = records.get(name)
            status = "never run" if not record else record["status"] + (" (stale)" if stale(name, state, hasher, environment) and record["status"] == "pass" else "")
            seconds = f"{record['seconds']:>7.0f}s" if record else ""
            print(f"{name:<40} {status:<15} {seconds}")
        save_state(state)
        return 0
    if args.names:
        chosen = args.names
    elif args.all:
        chosen = list(found)
    elif args.failed:
        chosen = [name for name in found if records.get(name, {}).get("status") == "fail"]
    elif args.changed or args.changed_files:
        if args.changed_files:
            changed, since = set(Path(args.changed_files).read_text(encoding="utf-8").split("\n")) - {""}, ""
        else:
            changed, since = changed_files(args.changed), f" since the merge base with {args.changed}"
        if changed is None:
            parser.error(f"no merge base with {args.changed}")
        base = None
        if args.changed:
            merge_base = subprocess.run(["git", "merge-base", "HEAD", args.changed], cwd=ROOT, capture_output=True, text=True)
            base = merge_base.stdout.strip() or None
        moved = workspace_only_manifests(changed, base) if not args.by_crate else set()
        if moved:
            print(f"{', '.join(sorted(moved))}: only workspace crates added, removed or moved; their sources count "
                  "instead", flush=True)
        affected = affected_harnesses(found, records, changed - moved, reach=not args.by_crate, base=base)
        chosen = [name for name in found if name in affected and stale(name, state, hasher, environment)]
        print(f"{len(changed)} files changed{since}: {len(affected)} harnesses "
              f"depend on them, {len(chosen)} without a pass for their current inputs.", flush=True)
    else:
        chosen = [name for name in found if stale(name, state, hasher, environment)]
    skips = platform_skips(host_config())
    for name in [name for name in chosen if base_name(name) in skips and not args.names]:
        print(f"skipped on {HOST} ({sys.platform}): {name}: {skips[base_name(name)]}", flush=True)
        chosen.remove(name)
    manual = {} if args.names or args.all else named_only(host_config())
    for name in [name for name in chosen if base_name(name) in manual]:
        print(f"not selected (runs when named, and nightly): {name}: {manual[base_name(name)]}", flush=True)
        chosen.remove(name)
    rounds = {name: args.repeat for name in chosen} if args.repeat > 1 else {}
    if args.flake_check:
        changed = {name for name in changed_harnesses(found, records, args.flake_check) if base_name(name) not in manual}
        for name in sorted(changed):
            if name not in chosen and records.get(name, {}).get("rounds", 1) < args.flake_rounds:
                chosen.append(name)
            if name in chosen:
                rounds[name] = max(rounds.get(name, 1), args.flake_rounds)
        if rounds:
            print(f"Flake check: {', '.join(sorted(rounds))} must pass {args.flake_rounds} times.", flush=True)
    selected = {name: found[name] for name in chosen}
    if args.list or not selected:
        for name in selected:
            print(name)
        print(f"{len(selected)} of {len(found)} harnesses would run" if args.list else "Nothing to run: every harness passed with its current inputs.")
        save_state(state)
        if not args.list:
            finished_run([])
        return 0

    artifacts = []
    for name in selected:
        record = records.get(name)
        if not record or "artifacts" not in record:
            template = SHARED / "dependencies" / f"{name}.json"
            record = json.loads(template.read_text(encoding="utf-8")) if template.exists() else {}
        for artifact in recorded_artifacts(record):
            if artifact not in artifacts:
                artifacts.append(artifact)
    config = host_config()
    hosts = [host for host in (config.get("default", []) if args.hosts is None else args.hosts.split(","))
             if host and host != HOST]
    unknown = [host for host in hosts if host not in config["hosts"]]
    if unknown:
        parser.error("unknown hosts (see tools/parity_hosts.json): " + ", ".join(unknown))
    jobs = default_local_jobs(remote=bool(hosts)) if args.jobs is None else max(0, args.jobs)
    def elsewhere(name):
        return base_name(name) not in config.get("local_only", {}) and \
            any(base_name(name) not in config["hosts"][host].get("exclude", {}) for host in hosts)
    if jobs == 0 and not all(elsewhere(name) for name in selected):
        jobs = 1
    remotes = [RemoteExecutor(host, config["hosts"][host], artifacts, config.get("local_only", {}), args.timeout)
               for host in hosts]
    local = None
    try:
        if jobs:
            print("Building probes...", flush=True)
            started = time.monotonic()
            built, errors = prebuild(artifacts, default_build_jobs())
            if not built:
                print(errors[-4000:])
                print("Probe build failed; harnesses will report their own build errors.", flush=True)
            print(f"Probes built in {time.monotonic() - started:.0f}s.", flush=True)
        where = "".join(f", plus {host}" for host in hosts)
        print(f"Running {len(selected)} harnesses, {jobs} at a time here{where}...", flush=True)
        reuse_oracles = not (args.fresh or args.all)
        local = LocalExecutor(jobs, int(memory_available() * 0.75), args.timeout,
                              REMOTE_LOCAL_PEAK if remotes and args.jobs is None else None)
        local.solo = solo_harnesses(config)
        if local.solo & {harness_of(name) for name in selected}:
            print(f"Alone on this machine ({SOLO_CPUS} CPUs or fewer), after the others: "
                  f"{', '.join(sorted(local.solo & {harness_of(name) for name in selected}))}", flush=True)
        results, elapsed = run(selected, state, hasher, environment_key(hasher), [*remotes, local], reuse_oracles,
                               rounds, known_flaky(config))
    finally:
        for executor in [*remotes, *([local] if local else [])]:
            executor.stop()
    save_state(state)
    failed = sorted(name for name, record in results.items() if record["status"] != "pass")
    finished_run(failed)
    print(f"\n{len(results) - len(failed)} passed, {len(failed)} failed in {elapsed / 60:.1f} min.")
    report_retries(results, known_flaky(config))
    for index, name in enumerate(failed):
        log = STATE_DIR / 'logs' / (name + '.log')
        print(f"  {name}: {log}")
        if index < FAILURE_TAILS:
            try:
                lines = log.read_text(encoding='utf-8', errors='replace').splitlines()[-FAILURE_TAIL_LINES:]
            except OSError:
                lines = []
            for line in lines:
                print(f"    {line[:300]}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
