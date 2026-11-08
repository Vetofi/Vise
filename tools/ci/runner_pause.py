#!/usr/bin/env python3
import json
import os
import re
import subprocess
import time
from pathlib import Path

DIR = Path.home() / ".actions-runners" / "vise"
SERVICE, SLICE = "vise-runner.service", "vise-ci.slice"
MANUAL, STATE, LOG = DIR / "pause", DIR / "paused", DIR / "pause.log"
GAME = re.compile(r"(SteamLaunch|gamescope|wine64-preloader|wineserver|wine-preloader|\.exe(\s|$)|proton\b"
                  r"|net\.minecraft\.client\.main\.Main|minecraft-launcher|prismlauncher)", re.I)
GPU_BUSY = int(os.environ.get("GPU_BUSY", 50))
INTERVAL = int(os.environ.get("INTERVAL", 15))
RESUME_AFTER = int(os.environ.get("RESUME_AFTER", 300))
THROTTLE_CPUS = int(os.environ.get("THROTTLE_CPUS", 2))
NORMAL_CPUS = int(os.environ.get("NORMAL_CPUS", max(1, (os.cpu_count() or 2) // 2)))
INPUT_PAUSE = int(os.environ.get("INPUT_PAUSE", 60))
IDLE_AFTER = int(os.environ.get("IDLE_AFTER", 900))
TEMP_LIMIT = int(os.environ.get("TEMP_LIMIT", 90))


def log(text):
    with LOG.open("a") as handle:
        handle.write(f"{time.strftime('%Y-%m-%dT%H:%M:%S')} {text}\n")


def systemctl(*arguments):
    return subprocess.run(["systemctl", "--user", *arguments], capture_output=True, text=True)


def ci_process(pid):
    try:
        return SLICE in Path(f"/proc/{pid}/cgroup").read_text()
    except OSError:
        return True


class Input:

    def __init__(self):
        self.devices = {}
        self.last = 0.0

    def poll(self):
        try:
            text = Path("/proc/bus/input/devices").read_text()
        except OSError:
            text = ""
        wanted = {f"/dev/input/{handler}" for block in text.split("\n\n") for line in block.splitlines()
                  if line.startswith("H: Handlers=") and re.search(r"\b(kbd|mouse\d*|js\d*)\b", line)
                  for handler in line.split("=", 1)[1].split() if handler.startswith("event")}
        for path in set(self.devices) - wanted:
            os.close(self.devices.pop(path))
        for path in wanted - set(self.devices):
            try:
                self.devices[path] = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
            except OSError:
                pass
        for path, descriptor in list(self.devices.items()):
            try:
                while os.read(descriptor, 4096):
                    self.last = time.monotonic()
            except BlockingIOError:
                pass
            except OSError:
                os.close(self.devices.pop(path))
        return time.monotonic() - self.last


def seat_session():
    sessions = subprocess.run(["loginctl", "list-sessions", "--no-legend"], capture_output=True, text=True).stdout
    for line in sessions.splitlines():
        fields = line.split()
        if "seat0" in fields:
            locked = subprocess.run(["loginctl", "show-session", fields[0], "-p", "LockedHint", "--value"],
                                    capture_output=True, text=True).stdout.strip() == "yes"
            return fields[0], locked
    return None


def at_the_screen(input_idle):
    if input_idle < INPUT_PAUSE:
        return f"input {input_idle:.0f} s ago"
    session = seat_session()
    if session and not session[1] and input_idle < IDLE_AFTER:
        return f"session {session[0]} unlocked, input {input_idle:.0f} s ago"
    return None


def too_hot():
    for monitor in Path("/sys/class/hwmon").glob("hwmon*"):
        try:
            if (monitor / "name").read_text().strip() not in ("coretemp", "k10temp"):
                continue
            readings = []
            for _ in range(5):
                readings.append(int((monitor / "temp1_input").read_text()) / 1000)
                time.sleep(0.5)
        except (OSError, ValueError):
            continue
        celsius = sorted(readings)[2]
        return f"CPU at {celsius:.0f} °C" if celsius >= TEMP_LIMIT else None
    return None


def fleet_load_gaming():
    status = Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")) / "fleet-load" / "status.json"
    try:
        if time.time() - status.stat().st_mtime > 120:
            return None
        return "fleet-load: playing a game" if json.loads(status.read_text())["human"]["gaming"] else None
    except (OSError, ValueError, KeyError, TypeError):
        return None


def game_process():
    uid = os.getuid()
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid != uid:
                continue
            command = (entry / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
        except OSError:
            continue
        if GAME.search(command) or (" --offscreen" not in command and "/vise-launcher/" in command
                                    and "vise-client" in command.split(" ")[0]):
            if not ci_process(int(entry.name)):
                return f"process {entry.name}: {command[:120]}"
    return None


def gpu_users():
    try:
        output = subprocess.run(["nvidia-smi", "pmon", "-c", "2", "-s", "u"], capture_output=True, text=True,
                                timeout=20).stdout
    except (OSError, subprocess.TimeoutExpired):
        return {}
    users = {}
    for line in output.splitlines():
        fields = line.split()
        if line.startswith("#") or len(fields) < 4 or not fields[1].isdigit() or not fields[3].isdigit():
            continue
        if not ci_process(int(fields[1])):
            users[int(fields[1])] = max(users.get(int(fields[1]), 0), int(fields[3]))
    return users


def runner_busy():
    group = systemctl("show", "-p", "ControlGroup", "--value", SERVICE).stdout.strip()
    try:
        pids = Path(f"/sys/fs/cgroup{group}/cgroup.procs").read_text().split() if group else []
    except OSError:
        return False
    for pid in pids:
        try:
            if Path(f"/proc/{pid}/comm").read_text().strip() == "Runner.Worker":
                return True
        except OSError:
            pass
    return False


def throttle(on):
    if on:
        systemctl("set-property", "--runtime", SLICE, "CPUWeight=1", "IOWeight=1", f"CPUQuota={THROTTLE_CPUS * 100}%")
    else:
        systemctl("set-property", "--runtime", SLICE, "CPUWeight=20", "IOWeight=20", f"CPUQuota={NORMAL_CPUS * 100}%")


def pause_reason(hot, keys):
    if MANUAL.exists():
        return f"paused by hand ({MANUAL})"
    reason = at_the_screen(keys.poll()) or too_hot() or fleet_load_gaming() or game_process()
    if reason:
        return reason
    busy = {pid for pid, sm in gpu_users().items() if sm >= GPU_BUSY}
    twice = sorted(busy & set(hot))
    hot.clear()
    hot.update(dict.fromkeys(busy))
    if twice:
        try:
            name = Path(f"/proc/{twice[0]}/comm").read_text().strip()
        except OSError:
            name = "?"
        return f"GPU busy: process {twice[0]} ({name})"
    return None


def main():
    hot, keys = {}, Input()
    last_sign = 0.0
    paused = None
    while True:
        reason = pause_reason(hot, keys)
        now = time.monotonic()
        if reason:
            last_sign = now
        active = systemctl("is-active", "--quiet", SERVICE).returncode == 0
        if reason or now - last_sign < RESUME_AFTER:
            if paused is not True:
                paused = True
                STATE.write_text((reason or "starting") + "\n")
                throttle(True)
                systemctl("stop", "vise-parity-worker-*.scope")
                log(f"pause: {reason or 'waiting ' + str(RESUME_AFTER) + ' s after starting'}"
                    + (f"; a job is running: throttled to {THROTTLE_CPUS} threads, stops after it" if active else ""))
            if active and not runner_busy():
                systemctl("stop", SERVICE)
                log("runner stopped")
        else:
            if paused is not False:
                paused = False
                STATE.unlink(missing_ok=True)
                throttle(False)
                log(f"resume: nobody at the screen and no game for {RESUME_AFTER} s")
            if not active:
                systemctl("start", SERVICE)
                log("runner started")
        time.sleep(INTERVAL)


if __name__ == "__main__":
    main()
