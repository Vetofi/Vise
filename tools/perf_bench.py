#!/usr/bin/env python3
import argparse
import bisect
import datetime
import json
import math
import os
import platform
import re
import shutil
import socket
import statistics
import subprocess
import sys
import tempfile
import threading
import time
import zipfile
from pathlib import Path

try:
    from . import lab, runtime
except ImportError:
    import lab
    import runtime

ROOT = lab.ROOT
WORK = ROOT / ".local/perf-bench"
VANILLA = lab.GAME
SEED = 1234
WORLD = "PerfBench"
SCENES = [
    ("village", (-960.0, 125.0, 80.0), (-960.0, 125.0, 272.0), 35.0),
    ("forest", (-1100.0, 120.0, -560.0), (-1100.0, 120.0, -368.0), 25.0),
    ("ocean", (-300.0, 85.0, -420.0), (-300.0, 85.0, -228.0), 10.0),
    ("mountain", (560.0, 260.0, 860.0), (752.0, 260.0, 860.0), 30.0),
]
OPTIONS = {
    "version": "{data_version}",
    "graphicsPreset": '"custom"',
    "renderDistance": "{distance}",
    "simulationDistance": "12",
    "biomeBlendRadius": "2",
    "prioritizeChunkUpdates": "1",
    "ao": "true",
    "renderClouds": '"true"',
    "cloudRange": "64",
    "particles": "0",
    "mipmapLevels": "4",
    "entityShadows": "true",
    "entityDistanceScaling": "1.0",
    "menuBackgroundBlurriness": "5",
    "cutoutLeaves": "true",
    "improvedTransparency": "false",
    "weatherRadius": "10",
    "maxAnisotropyBit": "1",
    "textureFiltering": "1",
    "maxFps": "260",
    "enableVsync": "false",
    "inactivityFpsLimit": '"minimized"',
    "fov": "0.0",
    "guiScale": "0",
    "fullscreen": "false",
    "pauseOnLostFocus": "false",
    "onboardAccessibility": "false",
    "skipMultiplayerWarning": "true",
    "tutorialStep": "none",
    "narrator": "0",
    "soundCategory_master": "0.0",
    "realmsNotifications": "false",
}
LAUNCHER_JVM = ["-XX:+UnlockExperimentalVMOptions", "-XX:+UseG1GC", "-XX:G1NewSizePercent=20",
                "-XX:G1ReservePercent=20", "-XX:MaxGCPauseMillis=50", "-XX:G1HeapRegionSize=32M"]
PERF_MODS = ["sodium", "lithium", "ferrite-core", "immediatelyfast"]
NEOFORGE_DATA_VERSION = "3955"


REPORT_FORMAT, REPORT_VERSION = "vise-benchmark-report", 1
HISTOGRAM_EDGES_MS = [0, 1, 2, 3, 4.17, 5, 6, 7, 8.33, 10, 12, 14, 16.67, 20, 25, 33.33, 40,
                      50, 66.67, 100, 150, 250, 500, 1000]
T95 = [12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
       2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
       2.052, 2.048, 2.045, 2.042]


def rnd(v):
    return math.copysign(math.floor(abs(v) * 1000 + 0.5), v) / 1000


def frames(phase, repeat, frame_ms, cpu_ms=(), gpu_ms=()):
    times = sorted(t for t in frame_ms if math.isfinite(t))
    n = len(times)
    if n == 0:
        return None
    total = sum(times)
    pct = lambda values, p: values[min(len(values), max(1, math.ceil(len(values) * p))) - 1]

    def low(share):
        slowest = times[n - max(1, math.ceil(n * share)):]
        return 1000.0 * len(slowest) / sum(slowest)

    histogram = [0] * len(HISTOGRAM_EDGES_MS)
    for t in times:
        histogram[max(1, bisect.bisect_right(HISTOGRAM_EDGES_MS, t)) - 1] += 1
    entry = {
        "phase": phase, "repeat": repeat, "frames": n, "seconds": rnd(total / 1000.0),
        "avg_fps": rnd(n * 1000.0 / total), "low_1pct_fps": rnd(low(0.01)), "low_01pct_fps": rnd(low(0.001)),
        "p50_ms": rnd(pct(times, 0.5)), "p95_ms": rnd(pct(times, 0.95)), "p99_ms": rnd(pct(times, 0.99)),
        "p999_ms": rnd(pct(times, 0.999)), "max_ms": rnd(times[-1]),
    }
    for key, values in (("cpu_p50_ms", cpu_ms), ("gpu_p50_ms", gpu_ms)):
        if values:
            entry[key] = rnd(pct(sorted(values), 0.5))
    entry["histogram"] = histogram
    return entry


def summary(values):
    values = [v for v in values if v is not None]
    n = len(values)
    if n == 0:
        return None
    mean = sum(values) / n
    sd = ci = None
    if n > 1:
        sd = math.sqrt(sum((v - mean) ** 2 for v in values) / (n - 1))
        ci = rnd((T95[n - 2] if n - 1 <= len(T95) else 1.96) * sd / math.sqrt(n))
        sd = rnd(sd)
    return {"n": n, "mean": rnd(mean), "stdev": sd, "min": rnd(min(values)), "max": rnd(max(values)), "ci95": ci}


def rss_bytes(pid):
    if sys.platform == "win32":
        return windows_working_set(pid)
    out = subprocess.run(["ps", "-o", "rss=", "-p", str(pid)], capture_output=True, text=True)
    return int(out.stdout.split()[0]) * 1024 if out.stdout.strip() else None


def windows_working_set(pid):
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [("cb", wintypes.DWORD), ("PageFaultCount", wintypes.DWORD)] + [
            (name, ctypes.c_size_t) for name in (
                "PeakWorkingSetSize", "WorkingSetSize", "QuotaPeakPagedPoolUsage", "QuotaPagedPoolUsage",
                "QuotaPeakNonPagedPoolUsage", "QuotaNonPagedPoolUsage", "PagefileUsage", "PeakPagefileUsage")]

    kernel32, psapi = ctypes.WinDLL("kernel32", use_last_error=True), ctypes.WinDLL("psapi")
    kernel32.OpenProcess.restype = wintypes.HANDLE
    kernel32.CloseHandle.argtypes = [wintypes.HANDLE]
    psapi.GetProcessMemoryInfo.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    handle = kernel32.OpenProcess(0x1000, False, pid)
    if not handle:
        return None
    try:
        counters = Counters(cb=ctypes.sizeof(Counters))
        if psapi.GetProcessMemoryInfo(handle, ctypes.byref(counters), counters.cb):
            return counters.WorkingSetSize
        return None
    finally:
        kernel32.CloseHandle(handle)


def load_average():
    return os.getloadavg()[0] if hasattr(os, "getloadavg") else None


def phys_footprint(pid):
    if sys.platform != "darwin":
        return None
    out = subprocess.run(["footprint", "-f", "bytes", str(pid)], capture_output=True, text=True)
    m = re.search(r"phys_footprint:\s*(\d+)", out.stdout)
    return int(m.group(1)) if m else None


class Sampler(threading.Thread):

    def __init__(self, pid):
        super().__init__(daemon=True)
        self.pid, self.samples, self.footprints, self.stop = pid, [], [], threading.Event()

    def run(self):
        tick = 0
        while not self.stop.is_set():
            if (rss := rss_bytes(self.pid)) is not None:
                self.samples.append((time.time(), rss))
            if tick % 8 == 0 and (fp := phys_footprint(self.pid)) is not None:
                self.footprints.append((time.time(), fp))
            tick += 1
            self.stop.wait(0.25)

    def summary(self, windows):
        if not self.samples:
            return {}
        during = [rss for t, rss in self.samples if any(a <= t <= b for a, b in windows)]
        out = {
            "rss_peak_mb": max(rss for _, rss in self.samples) / 2**20,
            "rss_steady_mb": statistics.median(during) / 2**20 if during else None,
            "rss_samples": len(self.samples),
        }
        if self.footprints:
            steady = [fp for t, fp in self.footprints if any(a <= t <= b for a, b in windows)]
            out["footprint_peak_mb"] = max(fp for _, fp in self.footprints) / 2**20
            out["footprint_steady_mb"] = statistics.median(steady) / 2**20 if steady else None
        return out


def nvidia_memory_mb(pid):
    if not shutil.which("nvidia-smi"):
        return None
    out = subprocess.run(["nvidia-smi", "--query-compute-apps=pid,used_memory", "--format=csv,noheader,nounits"],
                         capture_output=True, text=True)
    for line in out.stdout.splitlines():
        p, _, mem = line.partition(",")
        if p.strip() == str(pid):
            try:
                return float(mem)
            except ValueError:
                return None
    return None


def write_options(game_dir, distance, version=None):
    game_dir.mkdir(parents=True, exist_ok=True)
    data_version = lab.version_info()["world_version"]
    options = dict(OPTIONS, version=version) if version else OPTIONS
    text = "".join(f"{k}:{v.format(distance=distance, data_version=data_version)}\n" for k, v in options.items())
    (game_dir / "options.txt").write_text(text)
    return text


SIZES = [(1280, 720), (1920, 1080)]


def parse_sizes(text):
    return [tuple(int(v) for v in size.lower().split("x")) for size in text.split(",") if size]


def size_label(label, width, height):
    if (width, height) == SIZES[0]:
        return label
    return f"{label}-{height}p" if width * 9 == height * 16 else f"{label}-{width}x{height}"


def scene_list(args):
    picked = args.scenes.split(",") if args.scenes else [s[0] for s in SCENES]
    return [s for s in SCENES if s[0] in picked]


def yaw(start, end):
    return math.degrees(math.atan2(-(end[0] - start[0]), end[2] - start[2]))


def control_path():
    if hasattr(socket, "AF_UNIX"):
        return Path("/tmp") / f"vise-bench-{os.getpid()}.sock"
    return Path(tempfile.gettempdir()) / f"vise-bench-{os.getpid()}.json"


def read_line(s):
    data = b""
    while not data.endswith(b"\n"):
        chunk = s.recv(1 << 20)
        if not chunk:
            break
        data += chunk
    return data


class Control:
    def __init__(self, path):
        self.path = path

    def connect(self, timeout):
        if self.path.suffix != ".json":
            s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            s.settimeout(timeout)
            s.connect(str(self.path))
            return s
        endpoint = json.loads(self.path.read_text(encoding="utf-8"))
        s = socket.create_connection(("127.0.0.1", endpoint["port"]), timeout=timeout)
        s.sendall(f"auth {endpoint['token']}\n".encode())
        if b'"ok"' not in read_line(s):
            s.close()
            raise OSError(f"the game refused the token of {self.path}")
        return s

    def __call__(self, line, timeout=600):
        with self.connect(timeout) as s:
            s.sendall((line + "\n").encode())
            data = read_line(s)
        reply = json.loads(data)
        if isinstance(reply, dict) and "error" in reply:
            raise RuntimeError(f"{line}: {reply['error']}")
        return reply


def poll(control, check, limit, interval=0.05):
    deadline = time.time() + limit
    while time.time() < deadline:
        try:
            status = control("status", timeout=10)
            if check(status):
                return status, time.time()
        except (OSError, ValueError, RuntimeError):
            pass
        time.sleep(interval)
    raise TimeoutError(f"not ready after {limit} s")


QUIET_MESHES_PER_S = 100


def view_chunks(distance):
    return int(0.8 * math.pi * distance ** 2)


def wait_loaded(control, near, limit, min_chunks=0):
    deadline = time.time() + limit
    last, changed, previous, meshing = None, time.time(), None, time.time()
    first, moved, began = None, False, time.time()
    while time.time() < deadline:
        try:
            s = control("status", timeout=10)
        except (OSError, ValueError, RuntimeError):
            s = {}
        player = s.get("player")
        now = time.time()
        if not player or not near(player["position"]) or s.get("pending_chunks") != 0 or s.get("screen") == "title":
            last, changed, previous, meshing = None, now, None, now
        else:
            counts = (s["chunks"], s["generated_chunks"])
            first = first or counts
            moved = moved or counts != first or (s["chunks"] >= min_chunks > 0 and now - began > 5)
            if min_chunks and (not moved or s["chunks"] < min_chunks):
                last, changed, previous, meshing = None, now, None, now
                time.sleep(0.05)
                continue
            if counts != last:
                last, changed = counts, now
            stages = s.get("streaming")
            sample = (now, s["meshed_sections"])
            if stages is not None:
                if any(v for k, v in stages.items() if k != "protos"):
                    meshing = now
                quiet = 0.5
            else:
                if previous and sample[1] - previous[1] > QUIET_MESHES_PER_S * (now - previous[0]):
                    meshing = now
                quiet = 1.0
            previous = sample
            if now - changed >= 1.0 and now - meshing >= quiet:
                return max(changed, meshing)
        time.sleep(0.05)
    raise TimeoutError(f"not loaded after {limit} s")


def run_vise(args, distance):
    game_dir = WORK / "vise"
    shutil.rmtree(game_dir, ignore_errors=True)
    options = write_options(game_dir, distance)
    sock = control_path()
    sock.unlink(missing_ok=True)
    client = Path(args.build).resolve() / ("vise-client.exe" if sys.platform == "win32" else "vise-client")
    command = [str(client), "--resources", str(Path(args.resources).resolve()),
               "--control-socket", str(sock), "--no-pause", "--weather", "clear", "--time", "6000"]
    if (args.width, args.height) != SIZES[0]:
        command += ["--width", str(args.width), "--height", str(args.height)]
    if args.offscreen:
        command.append("--offscreen")
    for carry in dev_mods(args):
        command += ["--dev-mod", str(carry)]
    env = lab.game_env(VISE_TELEMETRY_URL="off")
    if args.warm:
        with (WORK / f"vise-rd{distance}-warm.log").open("w") as log:
            proc = subprocess.Popen(command, cwd=game_dir, env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                poll(Control(sock), lambda s: s.get("screen") == "title", 600)
                try:
                    Control(sock)("quit", timeout=60)
                except (OSError, ValueError, RuntimeError):
                    pass
            finally:
                stop(proc)
                sock.unlink(missing_ok=True)
    log = (WORK / f"vise-rd{distance}.log").open("w")
    started = time.time()
    proc = subprocess.Popen(command, cwd=game_dir, env=env, stdout=log, stderr=subprocess.STDOUT)
    sampler = Sampler(proc.pid)
    sampler.start()
    control = Control(sock)
    result = {"options_txt": options, "scenes": {}}
    windows = []
    try:
        title, at = poll(control, lambda s: s.get("screen") == "title" and s.get("renderer"), 300)
        result["renderer"] = title["renderer"]
        result["startup"] = {"title_s": at - started}
        control(f"open {WORLD} {SEED} creative")
        result["startup"]["world_s"] = wait_loaded(control, lambda p: True, args.timeout, view_chunks(distance)) - started
        for name, start, end, pitch in scene_list(args):
            control("time 6000")
            meshed = control("status")["meshed_sections"]
            control("tp {} {} {}".format(*start))
            tp = time.time()
            control("fly {0} {1} {2} {0} {1} {2} 0.1 {3}".format(*start, pitch))
            control(f"look {yaw(start, end):.3f} {pitch}")
            near = lambda p: sum((a - b) ** 2 for a, b in zip(p, start)) < 4.0
            scene = {"load_s": wait_loaded(control, near, args.timeout, view_chunks(distance)) - tp}
            loaded_meshes = control("status")["meshed_sections"]
            scene["meshes_load"] = loaded_meshes - meshed
            time.sleep(args.settle)
            loaded_meshes = control("status")["meshed_sections"]
            begin = time.time()
            summary = control("fly {} {} {} {} {} {} {} {} raw".format(*start, *end, args.seconds, pitch))
            windows.append((begin, time.time()))
            scene.update({k: summary.get(k) or [] for k in ("frame_ms", "cpu_ms", "gpu_ms")})
            status = control("status")
            scene["meshes_flight"] = status["meshed_sections"] - loaded_meshes
            renderer = status.get("renderer") or {}
            scene["gpu_mb"] = nvidia_memory_mb(proc.pid) or (renderer.get("gpu_allocated_bytes") or 0) / 2**20 or None
            result["scenes"][name] = scene
            print_scene(args.game, distance, name, scene)
    finally:
        try:
            control("quit", timeout=60)
        except (OSError, ValueError, RuntimeError):
            pass
        stop(proc)
        sampler.stop.set()
        sock.unlink(missing_ok=True)
    result["memory"] = sampler.summary(windows)
    args.release = release_version(WORK / f"vise-rd{distance}.log")
    return result


def release_version(log):
    first = log.read_text(errors="replace").partition("\n")[0].strip() if log.exists() else ""
    m = re.fullmatch(r"Vise ((?:v\d|nightly-)\S*)", first)
    return m and m.group(1)


def dev_mods(args):
    if not args.port_work:
        return []
    return sorted(p for p in (Path(args.port_work) / "mods").glob("*/carry") if (p / "vise-mod.toml").exists())


def vanilla_manifest():
    if not (VANILLA / "manifest.json").exists():
        sys.exit(f"{VANILLA} missing: prepare Vise first (tools/prepare_native_client.py)")
    return json.loads((VANILLA / "manifest.json").read_text())


def vanilla_classpath(manifest):
    jars = [VANILLA / "jars/client.jar"]
    for artifact in lab.client_library_artifacts(manifest):
        jars.append(VANILLA / "libraries/client" / artifact["path"])
    return jars


def manifest_jvm_flags(manifest):
    return [a for a in manifest.get("arguments", {}).get("jvm", []) if isinstance(a, str) and a.startswith("-XX:")]


def extract_natives(jars, target):
    target.mkdir(parents=True, exist_ok=True)
    for jar in jars:
        if "natives-" not in jar.name:
            continue
        with zipfile.ZipFile(jar) as archive:
            for name in archive.namelist():
                if name.endswith((".so", ".dylib", ".jnilib", ".dll")):
                    out = target / Path(name).name
                    if not out.exists():
                        out.write_bytes(archive.read(name))


def build_bench_mod(jars):
    source = ROOT / "mods/vise/src/bench"
    build = WORK / "mod"
    classes = build / "classes"
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir(parents=True)
    sources = sorted(map(str, (source / "java").rglob("*.java")))
    javac = Path(os.environ["JAVA_HOME"]) / "bin/javac" if os.environ.get("JAVA_HOME") else "javac"
    subprocess.run([str(javac), "--release", "25", "-proc:none", "-encoding", "UTF-8", "-d", str(classes),
                    "-classpath", os.pathsep.join(map(str, jars)), *sources], check=True)
    output = build / "vise-bench.jar"
    with zipfile.ZipFile(output, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(classes.rglob("*.class")):
            archive.write(path, path.relative_to(classes).as_posix())
        for path in sorted((source / "resources").rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(source / "resources").as_posix())
    return output


def perf_mod_jars():
    lock = json.loads((ROOT / "tools/benchmark-deps/mods.lock.json").read_text())
    jars, needs_api = [], False
    for name in PERF_MODS:
        project = lock["projects"][name]
        jar = runtime.fetch(project["url"], ROOT / ".local/pack-cache" / project["filename"],
                            project["hashes"]["sha512"], "sha512")
        with zipfile.ZipFile(jar) as archive:
            depends = json.loads(archive.read("fabric.mod.json")).get("depends", {})
        needs_api |= any(d == "fabric-api" or d.startswith("fabric-") and d != "fabricloader" for d in depends)
        jars.append(jar)
    if needs_api:
        api = lock["projects"]["fabric-api"]
        jars.append(runtime.fetch(api["url"], ROOT / ".local/pack-cache" / api["filename"],
                                  api["hashes"]["sha512"], "sha512"))
    return jars


def run_vanilla(args, distance):
    manifest = vanilla_manifest()
    game_jars = vanilla_classpath(manifest)
    fabric = runtime.fabric_libraries()
    mod = build_bench_mod(game_jars + fabric)
    game_dir = WORK / args.game
    shutil.rmtree(game_dir, ignore_errors=True)
    options = write_options(game_dir, distance)
    mods = game_dir / "mods"
    mods.mkdir()
    shutil.copy(mod, mods)
    loaded_mods = ["vise-bench"]
    if args.game == "vanilla-mods":
        for jar in perf_mod_jars():
            shutil.copy(jar, mods)
            loaded_mods.append(jar.name)
    natives = WORK / "natives"
    extract_natives(game_jars, natives)
    script = WORK / "script.json"
    script.write_text(json.dumps({
        "world": WORLD, "seed": SEED, "settle": args.settle, "seconds": args.seconds, "timeout": args.timeout,
        "scenes": [{"name": n, "from": a, "to": b, "pitch": p} for n, a, b, p in scene_list(args)],
    }))
    events = WORK / f"{args.game}-rd{distance}.events.jsonl"
    events.unlink(missing_ok=True)
    assets = WORK / "assets"
    (assets / "indexes").mkdir(parents=True, exist_ok=True)
    java = Path(os.environ["JAVA_HOME"]) / "bin/java" if os.environ.get("JAVA_HOME") else "java"
    command = [str(java), f"-Xmx{args.heap}", *LAUNCHER_JVM,
               *runtime.conditional_client_jvm_options(manifest), *manifest_jvm_flags(manifest),
               "--enable-native-access=ALL-UNNAMED",
               f"-Djava.library.path={natives}", f"-Dorg.lwjgl.system.SharedLibraryExtractPath={natives}",
               f"-Dvise.bench.script={script}", f"-Dvise.bench.out={events}",
               "-Dfabric.log.disableAnsi=true",
               "-cp", os.pathsep.join(map(str, game_jars + fabric)),
               "net.fabricmc.loader.impl.launch.knot.KnotClient",
               "--username", "Bench", "--uuid", "00000000000000000000000000000000", "--accessToken", "0",
               "--offlineDeveloperMode", "--version", lab.VERSION, "--gameDir", str(game_dir),
               "--assetsDir", str(assets), "--assetIndex", manifest["assetIndex"]["id"],
               "--width", str(args.width), "--height", str(args.height)]
    env = dict(os.environ, ALSOFT_DRIVERS="null")
    if sys.platform.startswith("linux"):
        libs = [p for p in (os.environ.get("NIX_LD_LIBRARY_PATH", ""), "/run/opengl-driver/lib") if p]
        env["LD_LIBRARY_PATH"] = ":".join(libs + [env.get("LD_LIBRARY_PATH", "")]).strip(":")
    log = (WORK / f"{args.game}-rd{distance}.log").open("w")
    return driven(args, distance, command, game_dir, env, log, events, options, loaded_mods)


def driven(args, distance, command, game_dir, env, log, events, options, loaded_mods):
    started = time.time()
    proc = subprocess.Popen(command, cwd=game_dir, env=env, stdout=log, stderr=subprocess.STDOUT)
    sampler = Sampler(proc.pid)
    sampler.start()
    nvidia = None
    try:
        deadline = started + args.timeout * (len(scene_list(args)) + 2)
        while proc.poll() is None and time.time() < deadline:
            time.sleep(0.5)
            if nvidia is None:
                nvidia = nvidia_memory_mb(proc.pid)
    finally:
        stop(proc)
        sampler.stop.set()
    lines = [json.loads(l) for l in events.read_text().splitlines()] if events.exists() else []
    by = lambda name: [e for e in lines if e["event"] == name]
    if not by("done") or by("timeout"):
        raise RuntimeError(f"{args.game} run failed: {lines[-1:] if lines else 'no events'}; see {log.name}")
    title, world = by("title")[0], by("world")[0]
    result = {
        "options_txt": options, "mods": loaded_mods, "heap": args.heap,
        "renderer": {k: title[k] for k in ("gpu", "vendor", "backend", "driver", "size",
                                           "render_distance", "simulation_distance")},
        "startup": {"title_s": title["epoch_ms"] / 1000 - started,
                    "world_s": world["ready_epoch_ms"] / 1000 - started},
        "scenes": {},
    }
    windows = []
    loaded = {e["scene"]: e for e in by("loaded")}
    flights = {e["scene"]: e for e in by("fly")}
    for flown in by("flown"):
        name = flown["scene"]
        windows.append((flights[name]["epoch_ms"] / 1000, flown["epoch_ms"] / 1000))
        scene = {"load_s": loaded[name]["load_ms"] / 1000, "frame_ms": flown["frame_ms"],
                 "chunks": flown["chunks"], "gpu_mb": nvidia}
        result["scenes"][name] = scene
        print_scene(args.game, distance, name, scene)
    result["memory"] = sampler.summary(windows)
    return result


def run_neoforge(args, distance):
    import neoforge_oracle
    home = neoforge_oracle.client()[0]
    mod = neoforge_oracle.build_bench(home)
    game_dir = WORK / args.game
    shutil.rmtree(game_dir, ignore_errors=True)
    options = write_options(game_dir, distance, version=NEOFORGE_DATA_VERSION)
    mods = game_dir / "mods"
    mods.mkdir()
    shutil.copy(mod, mods)
    loaded_mods = ["vise-bench"]
    for folder in filter(None, (args.mod_jars, args.deps)):
        for jar in sorted(Path(folder).glob("*.jar")):
            shutil.copy(jar, mods)
            loaded_mods.append(jar.name)
    natives = WORK / "natives-neoforge"
    script = WORK / "script.json"
    script.write_text(json.dumps({
        "world": WORLD, "seed": SEED, "settle": args.settle, "seconds": args.seconds, "timeout": args.timeout,
        "scenes": [{"name": n, "from": a, "to": b, "pitch": p} for n, a, b, p in scene_list(args)],
    }))
    events = WORK / f"{args.game}-rd{distance}.events.jsonl"
    events.unlink(missing_ok=True)
    assets = WORK / "assets"
    (assets / "indexes").mkdir(parents=True, exist_ok=True)
    java = Path(os.environ["JAVA_HOME"]) / "bin/java" if os.environ.get("JAVA_HOME") else "java"
    _, command = neoforge_oracle.client_command(
        java, args.heap, game_dir, natives, assets,
        jvm=[*LAUNCHER_JVM, f"-Dvise.bench.script={script}", f"-Dvise.bench.out={events}"],
        game=["--width", str(args.width), "--height", str(args.height)])
    natives.mkdir(parents=True, exist_ok=True)
    extract_natives(neoforge_oracle.client()[3], natives)
    env = dict(os.environ, ALSOFT_DRIVERS="null")
    for attempt in (1, 2):
        log = (WORK / f"{args.game}-rd{distance}.log").open("w")
        try:
            return driven(args, distance, command, game_dir, env, log, events, options, loaded_mods)
        except RuntimeError:
            if attempt == 2 or events.exists():
                raise
            shutil.rmtree(game_dir / "crash-reports", ignore_errors=True)


def stop(proc):
    try:
        proc.wait(timeout=60)
    except subprocess.TimeoutExpired:
        proc.terminate()
        try:
            proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            proc.kill()


def print_scene(game, distance, name, scene):
    s = frames(name, 0, scene["frame_ms"]) or {}
    print(f"  {game:<12} rd {distance:>2} {name:<9} load {scene['load_s']:6.1f} s  {s.get('avg_fps', 0):7.1f} fps"
          f"  1% {s.get('low_1pct_fps', 0):6.1f}  0.1% {s.get('low_01pct_fps', 0):6.1f}"
          f"  p50 {s.get('p50_ms', 0):6.2f}  p99 {s.get('p99_ms', 0):7.2f} ms", flush=True)


def hardware():
    info = {"os": {"darwin": "macos", "win32": "windows"}.get(sys.platform, sys.platform),
            "arch": {"arm64": "aarch64", "AMD64": "x86_64"}.get(platform.machine(), platform.machine()),
            "cpu_threads": os.cpu_count()}
    if sys.platform == "darwin":
        q = lambda *cmd: subprocess.run(cmd, capture_output=True, text=True).stdout.strip()
        info.update(os_version=q("sw_vers", "-productVersion"), cpu=q("sysctl", "-n", "machdep.cpu.brand_string"),
                    ram_mb=int(q("sysctl", "-n", "hw.memsize") or 0) >> 20)
    elif sys.platform == "win32":
        info.update(os_version=windows_version(), cpu=windows_cpu(), ram_mb=windows_ram_bytes() >> 20)
    else:
        release = dict(l.split("=", 1) for l in Path("/etc/os-release").read_text().splitlines() if "=" in l)
        info["os_version"] = f"{release.get('PRETTY_NAME', '').strip(chr(34))} (kernel {platform.release()})"
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                info["cpu"] = line.split(":", 1)[1].strip()
                break
        info["ram_mb"] = os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES") >> 20
    return info


def windows_version():
    out = subprocess.run(["cmd", "/C", "ver"], capture_output=True, text=True).stdout.strip()
    return out or f"Windows {platform.version()}"


def windows_cpu():
    import winreg
    try:
        with winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0") as key:
            return str(winreg.QueryValueEx(key, "ProcessorNameString")[0]).strip()
    except OSError:
        return platform.processor()


def windows_ram_bytes():
    import ctypes

    class Status(ctypes.Structure):
        _fields_ = [("dwLength", ctypes.c_uint32), ("dwMemoryLoad", ctypes.c_uint32)] + [
            (name, ctypes.c_uint64) for name in ("ullTotalPhys", "ullAvailPhys", "ullTotalPageFile",
                                                 "ullAvailPageFile", "ullTotalVirtual", "ullAvailVirtual",
                                                 "ullAvailExtendedVirtual")]

    status = Status(dwLength=ctypes.sizeof(Status))
    return status.ullTotalPhys if ctypes.windll.kernel32.GlobalMemoryStatusEx(ctypes.byref(status)) else 0


def report(args, distance, runs, app):
    first = runs[0]
    renderer = first.get("renderer") or {}
    hw = hardware()
    if renderer.get("gpu"):
        hw["gpu"] = {"name": renderer["gpu"], "vendor": str(renderer.get("vendor") or ""),
                     "driver": str(renderer.get("driver") or ""), "backend": str(renderer.get("backend") or "").lower(),
                     "kind": renderer.get("device_type") or ""}
    size = renderer.get("size")
    conditions = {"scenario": "perf_bench:" + ",".join(first["scenes"]), "seed": str(SEED),
                  "render_distance": distance, "simulation_distance": 12, "vsync": False,
                  "offscreen": bool(args.offscreen), "repeats": len(runs), "warmup_s": args.settle}
    if first["load_before"] is not None:
        conditions["load_avg_1m"] = rnd(first["load_before"])
    if size:
        conditions["window"] = list(size)
    metrics, entries = {}, []

    def metric(name, values):
        if (m := summary(values)) is not None:
            metrics[name] = m

    metric("startup.title_ms", [r["startup"]["title_s"] * 1000 for r in runs])
    metric("startup.world_ms", [r["startup"]["world_s"] * 1000 for r in runs])
    for name in first["scenes"]:
        scenes = [r["scenes"][name] for r in runs if name in r["scenes"]]
        flights = [frames(name, i, s["frame_ms"], s.get("cpu_ms", ()), s.get("gpu_ms", ())) for i, s in enumerate(scenes)]
        entries += [f for f in flights if f]
        metric(f"chunks.{name}.load_ms", [s["load_s"] * 1000 for s in scenes])
        metric(f"chunks.{name}.meshed_count", [s.get("meshes_load") for s in scenes])
        for key in ("avg_fps", "low_1pct_fps", "low_01pct_fps"):
            metric(f"fps.{name}.{key}", [f[key] for f in flights if f])
        metric(f"memory.{name}.gpu_mb", [s.get("gpu_mb") for s in scenes])
    metric("memory.rss_peak_mb", [r["memory"].get("rss_peak_mb") for r in runs])
    metric("memory.rss_steady_mb", [r["memory"].get("rss_steady_mb") for r in runs])
    metric("memory.footprint_peak_mb", [r["memory"].get("footprint_peak_mb") for r in runs])
    metric("memory.footprint_steady_mb", [r["memory"].get("footprint_steady_mb") for r in runs])
    return {"format": REPORT_FORMAT, "version": REPORT_VERSION, "kind": "game",
            "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "app": app, "hardware": hw, "conditions": conditions, "metrics": metrics, "frames": entries,
            "label": args.label}


def table(labels):
    runs = {}
    for label in labels:
        for path in sorted((ROOT / "benchmarks/perf").glob(f"{label}-rd*.json"), key=lambda p: int(p.stem.rsplit("rd", 1)[1])):
            r = json.loads(path.read_text())
            runs.setdefault(label, {})[r["conditions"]["render_distance"]] = r
    distances = sorted({d for by in runs.values() for d in by})
    cell = lambda m, digits=0: "–" if not m else f"{m['mean']:.{digits}f}" + (f" ±{m['ci95']:.{digits}f}" if m["ci95"] else "")
    print("| RD | scene | " + " | ".join(f"{l}: avg / 1% / 0.1% low FPS" for l in labels) + " |")
    print("| --- | --- |" + " --- |" * len(labels))
    for d in distances:
        scenes = sorted({k.split(".")[1] for by in runs.values() if d in by for k in by[d]["metrics"] if k.startswith("fps.")})
        for scene in scenes:
            cells = []
            for l in labels:
                m = runs.get(l, {}).get(d, {}).get("metrics", {})
                cells.append(" / ".join(cell(m.get(f"fps.{scene}.{k}")) for k in ("avg_fps", "low_1pct_fps", "low_01pct_fps")))
            print(f"| {d} | {scene} | " + " | ".join(cells) + " |")
    print()
    print("| RD | " + " | ".join(f"{l}: title / world s, mean scene load s, RSS steady / peak MB, footprint steady / peak MB, load avg" for l in labels) + " |")
    print("| --- |" + " --- |" * len(labels))
    for d in distances:
        cells = []
        for l in labels:
            r = runs.get(l, {}).get(d)
            if not r:
                cells.append("–")
                continue
            m = r["metrics"]
            loads = [v["mean"] for k, v in m.items() if k.endswith(".load_ms")]
            sec = lambda k: f"{m[k]['mean'] / 1000:.1f}" if k in m else "–"
            mb = lambda k: f"{m[k]['mean']:.0f}" if k in m else "–"
            cells.append(f"{sec('startup.title_ms')} / {sec('startup.world_ms')}, "
                         f"{statistics.mean(loads) / 1000 if loads else 0:.1f}, "
                         f"{mb('memory.rss_steady_mb')} / {mb('memory.rss_peak_mb')}, "
                         f"{mb('memory.footprint_steady_mb')} / {mb('memory.footprint_peak_mb')}, "
                         f"{r['conditions'].get('load_avg_1m', '–')}")
        print(f"| {d} | " + " | ".join(cells) + " |")


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--game", choices=["vise", "vanilla", "vanilla-mods", "neoforge"], default="vise")
    p.add_argument("--label", help="result name (default <game>-<host>)")
    p.add_argument("--distances", default="8,16,32")
    p.add_argument("--scenes", help="comma-separated subset of " + ",".join(s[0] for s in SCENES))
    p.add_argument("--seconds", type=float, default=12.0, help="flight duration per scene")
    p.add_argument("--settle", type=float, default=2.0, help="seconds held at the start before each flight")
    p.add_argument("--timeout", type=float, default=300.0, help="seconds allowed for each load")
    p.add_argument("--sizes", default=",".join(f"{w}x{h}" for w, h in SIZES),
                   help="window sizes, each a set of reports (default 1280x720 and 1920x1080)")
    p.add_argument("--heap", default="2G", help="vanilla -Xmx (the official launcher's default is 2G)")
    p.add_argument("--offscreen", action="store_true",
                   help="Vise renders into a texture and does not wait for the window (locked macOS sessions)")
    p.add_argument("--resources", default=str(lab.RESOURCES))
    p.add_argument("--build", default=str(ROOT / "target/release"))
    p.add_argument("--repeats", type=int, default=3, help="games per render distance; the report gives means and 95%% CIs")
    p.add_argument("--port-work", help="vise: also load the mods carried over in this vise-port-pack work folder")
    p.add_argument("--warm", action="store_true",
                   help="vise: measure a later start, after one start to the title screen built the caches")
    p.add_argument("--mod-jars", help="neoforge: the mod jars to load (the user's own; never committed)")
    p.add_argument("--deps", help="neoforge: their required libraries")
    p.add_argument("--table", nargs="+", metavar="LABEL", help="print markdown tables of saved runs, and exit")
    args = p.parse_args()
    if args.table:
        table(args.table)
        return
    args.label = args.label or f"{args.game}-{platform.node().split('.')[0]}"
    WORK.mkdir(parents=True, exist_ok=True)
    run = {"vise": run_vise, "neoforge": run_neoforge}.get(args.game, run_vanilla)
    label = args.label
    for args.width, args.height in parse_sizes(args.sizes):
        args.label = size_label(label, args.width, args.height)
        for d in [int(x) for x in args.distances.split(",")]:
            runs = []
            for _ in range(args.repeats):
                load_before = load_average()
                result = run(args, d)
                result["load_before"], result["load_after"] = load_before, load_average()
                runs.append(result)
            out = ROOT / "benchmarks/perf" / f"{args.label}-rd{d}.json"
            out.parent.mkdir(parents=True, exist_ok=True)
            out.write_text(json.dumps(report(args, d, runs, app(args)), indent=1) + "\n")
            print(out)


def app(args):
    if args.game == "vise":
        commit = subprocess.run(["git", "rev-parse", "--short", "HEAD"], cwd=ROOT,
                                capture_output=True, text=True).stdout.strip() or None
        manifest = (ROOT / "crates/vise-client/Cargo.toml").read_text()
        version = next(l.split('"')[1] for l in manifest.splitlines() if l.startswith("version"))
        mods = [{"name": p.parent.name, "version": "carried"} for p in dev_mods(args)]
        if getattr(args, "release", None):
            version, commit = args.release, None
        return {"name": "vise-client", "version": version, "commit": commit, **({"mods": mods} if mods else {})}
    if args.game == "neoforge":
        import neoforge_oracle
        jars = [j.name for f in filter(None, (args.mod_jars, args.deps)) for j in sorted(Path(f).glob("*.jar"))]
        return {"name": "minecraft", "version": "1.21.1",
                "mods": [{"name": "neoforge", "version": neoforge_oracle.NEOFORGE}] + [{"name": j} for j in jars]}
    lock = json.loads((ROOT / "tools/benchmark-deps/mods.lock.json").read_text())
    mods = [{"name": "fabric-loader", "version": lock["fabric_loader"]}]
    if args.game == "vanilla-mods":
        mods += [{"name": n, "version": lock["projects"][n]["version"]} for n in PERF_MODS]
    return {"name": "minecraft", "version": lab.VERSION, "mods": mods}


if __name__ == "__main__":
    main()
