#!/usr/bin/env python3

import argparse
import datetime
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import sys
import threading
import time
import urllib.request
import zipfile

try:
    from . import lab, runtime, test_rust_parity as base
except ImportError:
    import lab
    import runtime
    import test_rust_parity as base

ROOT = lab.ROOT
EXE = '.exe' if os.name == 'nt' else ''
BIN = Path(os.environ.get('VISE_BENCH_BIN', ROOT / 'target/release'))

PROPERTIES = {
    'level-seed': '12345',
    'view-distance': '10',
    'simulation-distance': '10',
    'online-mode': 'false',
    'white-list': 'false',
    'gamemode': 'creative',
    'force-gamemode': 'true',
    'spawn-protection': '0',
    'max-players': '200',
    'difficulty': 'easy',
    'sync-chunk-writes': 'true',
}


def cpu_seconds(text):
    days, _, rest = text.rpartition('-')
    parts = [float(p) for p in rest.split(':')]
    seconds = 0.0
    for p in parts:
        seconds = seconds * 60 + p
    return seconds + (float(days) * 86400 if days else 0)


def tree_usage(root):
    out = subprocess.run(['ps', '-A', '-o', 'pid=,ppid=,rss=,time='], capture_output=True, text=True).stdout
    rows = {}
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 4:
            rows[int(parts[0])] = (int(parts[1]), int(parts[2]) * 1024, cpu_seconds(parts[3]))
    tree, frontier = {root}, [root]
    while frontier:
        parent = frontier.pop()
        for pid, (ppid, _, _) in rows.items():
            if ppid == parent and pid not in tree:
                tree.add(pid)
                frontier.append(pid)
    rss = sum(rows[p][1] for p in tree if p in rows)
    cpu = sum(rows[p][2] for p in tree if p in rows)
    return rss, cpu


class Server:

    def __init__(self, command, directory, log_path):
        self.lines = []
        self.lock = threading.Lock()
        self.log = log_path.open('w', encoding='utf-8')
        self.process = subprocess.Popen([str(c) for c in command], cwd=directory, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                        encoding='utf-8', errors='replace',
                                        env=lab.game_env())
        threading.Thread(target=self._read, daemon=True).start()

    def _read(self):
        for line in self.process.stdout:
            with self.lock:
                self.lines.append(line.rstrip('\n'))
            self.log.write(line)
            self.log.flush()

    def count(self):
        with self.lock:
            return len(self.lines)

    def since(self, n):
        with self.lock:
            return self.lines[n:]

    def wait_for(self, pattern, timeout):
        regex = re.compile(pattern)
        deadline = time.time() + timeout
        while time.time() < deadline:
            if any(regex.search(l) for l in self.since(0)):
                return
            if self.process.poll() is not None:
                raise RuntimeError('the server exited:\n' + '\n'.join(self.since(0)[-20:]))
            time.sleep(0.1)
        raise RuntimeError(f'no {pattern!r} within {timeout} s')

    def console(self, line):
        self.process.stdin.write(line + '\n')
        self.process.stdin.flush()

    def stop(self):
        if self.process.poll() is None:
            try:
                self.console('stop')
                self.process.wait(timeout=180)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                self.process.kill()
                self.process.wait()
        self.log.close()


DONE = r'INFO\]: Done \('
TICK_AVERAGE = re.compile(r'Average time per tick: ([0-9.]+) ?ms')
TICK_PERCENTILES = re.compile(r'P50: ([0-9.]+) ?ms P95: ([0-9.]+) ?ms P99: ([0-9.]+) ?ms')


PAPER_API = 'https://fill.papermc.io/v3/projects/paper/versions/{version}/builds/latest'
JARS = lab.LOCAL / 'server-bench/jars'


def paper_jar():
    lock, _ = lab.config()
    version = lock['minecraft']['version']
    request = urllib.request.Request(PAPER_API.format(version=version),
                                     headers={'User-Agent': 'vise-server-bench (github.com/vise-rust/vise)'})
    with urllib.request.urlopen(request, timeout=30) as response:
        build = json.load(response)
    download = build['downloads']['server:default']
    jar = runtime.fetch(download['url'], JARS / download['name'], download['checksums']['sha256'], 'sha256')
    return jar, build['id']


def fabric_mods():
    lock = json.loads((ROOT / 'tools/benchmark-deps/mods.lock.json').read_text())
    def fetch(name):
        project = lock['projects'][name]
        return runtime.fetch(project['url'], JARS / project['filename'], project['hashes']['sha512'], 'sha512')
    jars = [fetch('lithium')]
    with zipfile.ZipFile(jars[0]) as archive:
        depends = json.loads(archive.read('fabric.mod.json')).get('depends', {})
    if any(d == 'fabric-api' or d.startswith('fabric-') and d != 'fabricloader' for d in depends):
        jars.append(fetch('fabric-api'))
    return jars, {'fabric_loader': lock['fabric_loader'], 'lithium': lock['projects']['lithium']['version']}


def tick_query(server):
    start = server.count()
    server.console('tick query')
    deadline = time.time() + 5
    average = percentiles = None
    while time.time() < deadline and (average is None or percentiles is None):
        for line in server.since(start):
            if (m := TICK_AVERAGE.search(line)) and average is None:
                average = float(m.group(1))
            if (m := TICK_PERCENTILES.search(line)) and percentiles is None:
                percentiles = tuple(float(g) for g in m.groups())
        time.sleep(0.05)
    if average is None or percentiles is None:
        return None
    return {'average': average, 'p50': percentiles[0], 'p95': percentiles[1], 'p99': percentiles[2]}


def server_command(kind, directory, args):
    java = ['java', f'-Xms{args.heap}', f'-Xmx{args.heap}', '-Duser.timezone=UTC']
    if kind == 'vanilla':
        _, jars, libraries = base.verify_artifacts()
        classpath = os.pathsep.join(map(str, [jars['server'], *libraries]))
        return [*java, '-cp', classpath, 'net.minecraft.server.Main', 'nogui']
    if kind == 'paper':
        lock, jars, _ = base.verify_artifacts()
        jar, _ = paper_jar()
        shared = lab.LOCAL / 'server-bench/paper-shared'
        cache = shared / 'cache'
        cache.mkdir(parents=True, exist_ok=True)
        mojang = cache / f'mojang_{lock["minecraft"]["version"]}.jar'
        if not mojang.exists():
            shutil.copyfile(jars['server'].parent / 'server.jar', mojang)
        for name in ('cache', 'libraries', 'versions'):
            (shared / name).mkdir(exist_ok=True)
            (directory / name).symlink_to(shared / name, target_is_directory=True)
        (directory / 'bukkit.yml').write_text('settings:\n  connection-throttle: -1\n')
        return [*java, '-jar', jar, '--nogui']
    if kind == 'fabric':
        _, jars, libraries = base.verify_artifacts()
        mods = directory / 'mods'
        mods.mkdir()
        for jar in fabric_mods()[0]:
            shutil.copyfile(jar, mods / jar.name)
        classpath = os.pathsep.join(map(str, [jars['server'], *libraries, *runtime.fabric_libraries()]))
        return [*java, '-Dfabric.log.disableAnsi=true', '-cp', classpath,
                'net.fabricmc.loader.impl.launch.knot.KnotServer', 'nogui']
    server = BIN / f'vise-server{EXE}'
    subprocess.run([server, '--dir', directory, 'import', directory / 'server.properties'], check=True,
                   capture_output=True)
    toml = directory / 'server.toml'
    toml.write_text(re.sub(r'(?m)^bind = "127\.0\.0\.1:8130"', 'bind = ""', toml.read_text()))
    if args.anticheat:
        toml.write_text(toml.read_text() + '\n[anticheat]\nenabled = true\n')
    return [server, '--dir', directory, 'run']


def flag_count(directory):
    log = directory / 'logs/anticheat.log'
    return len(log.read_text().splitlines()) if log.exists() else 0


def bots_command(args):
    if args.bots_via:
        return [*args.bots_via.split(), args.bots_bin or 'vise-bots']
    return [args.bots_bin or BIN / f'vise-bots{EXE}']


def run_once(kind, bots, behaviour, args, out):
    name = f'{kind}-{behaviour}-{bots}'
    directory = out / name
    shutil.rmtree(directory, ignore_errors=True)
    directory.mkdir(parents=True)
    port = lab.free_port()
    props = dict(PROPERTIES, **{'server-port': str(port), 'level-seed': str(args.seed)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()))
    (directory / 'eula.txt').write_text('eula=true\n')
    command = server_command(kind, directory, args)
    if args.nice:
        command = ['nice', '-n', str(args.nice), *command]
    started = time.time()
    server = Server(command, directory, out / f'{name}.log')
    result = {'server': kind, 'bots': bots, 'behaviour': behaviour}
    try:
        server.wait_for(DONE, 600)
        result['start_seconds'] = round(time.time() - started, 2)
        server.console('gamerule respawn_radius 0')
        for rule in filter(None, args.gamerule.split(',')):
            server.console('gamerule ' + rule.replace('=', ' '))
        time.sleep(5)
        report = out / f'{name}-bots.json'
        total = bots * args.join_every + args.warmup + args.seconds
        bot_process = subprocess.Popen(
            [*bots_command(args), '--server', f'{args.connect}:{port}', '--bots', str(bots),
             '--behaviour', behaviour, '--join-every', str(args.join_every), '--seconds', str(int(total)),
             '--seed', str(args.seed), '--report', '-',
             *(['--explore-speed', str(args.explore_speed)] if args.explore_speed else [])],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        bot_lines = []
        threading.Thread(target=lambda: bot_lines.extend(bot_process.stdout), daemon=True).start()
        time.sleep(bots * args.join_every + args.warmup)
        measure_start = time.time()
        rss, cpu, ticks = [], [], []
        _, last_cpu = tree_usage(server.process.pid)
        last_time = time.time()
        next_query = time.time()
        while time.time() - measure_start < args.seconds:
            time.sleep(1)
            now = time.time()
            r, c = tree_usage(server.process.pid)
            rss.append(r)
            cpu.append((c - last_cpu) / (now - last_time) * 100)
            last_cpu, last_time = c, now
            if now >= next_query:
                if (t := tick_query(server)) is not None:
                    ticks.append(t)
                next_query = now + 5
        bot_process.wait(timeout=total + 120)
        time.sleep(0.5)
        measured, rest = [], []
        for line in list(bot_lines):
            try:
                sample = json.loads(line)
            except ValueError:
                sample = None
            if isinstance(sample, dict) and 't' in sample:
                measured.append(sample)
            else:
                rest.append(line)
        window = [l for l in measured if l['t'] >= measured[-1]['t'] - args.seconds] if measured else []
        summary_text = ''.join(rest)
        summary = json.loads(summary_text) if summary_text.strip() else {}
        report.write_text(json.dumps(summary, indent=2) + '\n')
        mean = lambda xs: round(statistics.fmean(xs), 2) if xs else None
        per_bot = summary.get('per_bot', [])
        by_behaviour = {}
        for b in per_bot:
            by_behaviour.setdefault(b['behaviour'], []).append(b)
        result.update({
            'mspt': {k: mean([t[k] for t in ticks]) for k in ('average', 'p50', 'p95', 'p99')},
            'mspt_worst_p99': max((t['p99'] for t in ticks), default=None),
            'tps': round(min(20.0, 1000 / max(statistics.fmean([t['average'] for t in ticks]), 1e-9)), 2) if ticks else None,
            'rss_mb': {'mean': mean([r / 1e6 for r in rss]), 'max': round(max(rss) / 1e6, 1) if rss else None},
            'cpu_percent': mean(cpu),
            'chunks_per_second': mean([l['chunks_per_second'] for l in window]),
            **({k + '_per_second': round((window[-1][k] - window[0][k]) / max(len(window) - 1, 1), 2)
                for k in ('unique', 'forgotten', 'waited_ticks')} if len(window) > 1 else {}),
            'chunks_total': summary.get('chunks'),
            'chunks_per_bot': {k: mean([b['chunks'] for b in v]) for k, v in by_behaviour.items()},
            'unique_chunks_per_bot': {k: mean([b['unique_chunks'] for b in v]) for k, v in by_behaviour.items()},
            'resent': summary.get('resent'),
            'pulled_back': summary.get('pulled_back'),
            'moved_too_quickly': sum('moved too quickly!' in l for l in server.since(0)),
            'anticheat_flags': flag_count(directory),
            'moved_wrongly': sum('moved wrongly!' in l for l in server.since(0)),
            'bots_joined': summary.get('joined'),
            'bots_failed': [b for b in summary.get('per_bot', []) if b.get('error')],
            'tick_queries': len(ticks),
        })
    except Exception as e:
        result['error'] = str(e)
    finally:
        server.stop()
    return result


EQUAL_WORK = 0.10


def work(runs):
    groups = {}
    for r in runs:
        if 'error' not in r:
            groups.setdefault((r['behaviour'], r['bots']), []).append(r)
    rows = []
    for (behaviour, bots), group in groups.items():
        if len(group) < 2:
            continue
        row = {'behaviour': behaviour, 'bots': bots, 'servers': {}, 'equal': True}
        for key in ('chunks_per_second', 'unique_per_second'):
            top = max(r.get(key) or 0 for r in group)
            for r in group:
                value = r.get(key) or 0
                share = round(value / top, 3) if top else 1.0
                row['servers'].setdefault(r['server'], {})[key] = {'value': value, 'share': share}
                if top - value > max(EQUAL_WORK * top, 1.0):
                    row['equal'] = False
        for r in group:
            row['servers'][r['server']]['waited_ticks_per_second'] = r.get('waited_ticks_per_second')
        rows.append(row)
    return rows


def versions(kinds):
    out = {}
    if 'paper' in kinds:
        out['paper_build'] = paper_jar()[1]
    if 'fabric' in kinds:
        out.update(fabric_mods()[1])
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--servers', default='vise,vanilla', help='vise, vanilla, paper, fabric (Lithium)')
    parser.add_argument('--bots', default='1,10,25,50')
    parser.add_argument('--behaviour', default='walk,explore', help='one pass per behaviour (comma-separated)')
    parser.add_argument('--join-every', type=float, default=1.0)
    parser.add_argument('--warmup', type=int, default=60)
    parser.add_argument('--seconds', type=int, default=300)
    parser.add_argument('--seed', type=int, default=12345)
    parser.add_argument('--explore-speed', type=float, default=0,
                        help='blocks a second the explore bots fly (default: the bots\' 22, as fast as chunks '
                             'come); a speed both servers keep up with makes the explore pass equal work')
    parser.add_argument('--gamerule', default='',
                        help="more game rules for both servers, 'name=value,...' (locator_bar=false: Vise has "
                             "no locator bar yet)")
    parser.add_argument('--heap', default='4g', help='the Java heap, -Xms and -Xmx (vanilla, Paper, Fabric)')
    parser.add_argument('--out', type=Path, default=None, help='the report (JSON)')
    parser.add_argument('--nice', type=int, default=0, help='run the servers under nice -n N')
    parser.add_argument('--anticheat', action='store_true',
                        help="turn Vise's anticheat on (server.toml [anticheat]); its flags are counted")
    parser.add_argument('--bots-via', default='', help="a command prefix to run the bots elsewhere ('ssh host')")
    parser.add_argument('--bots-bin', default='', help='vise-bots (on the --bots-via machine)')
    parser.add_argument('--connect', default='127.0.0.1', help='the address the bots reach the server at')
    args = parser.parse_args()
    out = lab.LOCAL / 'server-bench'
    out.mkdir(parents=True, exist_ok=True)
    runs = []
    for behaviour in args.behaviour.split(','):
        for bots in [int(b) for b in args.bots.split(',')]:
            for kind in args.servers.split(','):
                print(f'{kind}, {bots} {behaviour} bots...', flush=True)
                result = run_once(kind, bots, behaviour, args, out)
                runs.append(result)
                print(json.dumps(result), flush=True)
    report = {
        'kind': 'vise-server-benchmark',
        'date': datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds'),
        'machine': {'system': platform.system(), 'machine': platform.machine(), 'cpus': os.cpu_count()},
        'commit': subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=ROOT, capture_output=True, text=True).stdout.strip(),
        'java': subprocess.run(['java', '--version'], capture_output=True, text=True).stdout.splitlines()[:1],
        'versions': versions(args.servers.split(',')),
        'settings': {**vars(args), 'out': str(args.out), 'properties': PROPERTIES},
        'runs': runs,
        'work': work(runs),
    }
    path = args.out or out / 'report.json'
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(report, indent=2) + '\n')
    print(f'\n{"server":8} {"pass":8} {"bots":>4} {"start s":>8} {"MSPT":>6} {"P95":>6} {"P99":>6} {"RSS MB":>7} '
          f'{"CPU %":>6} {"chunks/s":>8} {"/bot":>6} {"resent":>6} {"pulled":>6} {"joined":>6}')
    for r in runs:
        if 'error' in r:
            print(f'{r["server"]:8} {r["behaviour"]:8} {r["bots"]:>4}  failed: {r["error"].splitlines()[0]}')
            continue
        per_bot = r['chunks_per_bot'].get(r['behaviour']) or round((r['chunks_total'] or 0) / r['bots'])
        print(f'{r["server"]:8} {r["behaviour"]:8} {r["bots"]:>4} {r["start_seconds"]:>8} {r["mspt"]["average"]!s:>6} '
              f'{r["mspt"]["p95"]!s:>6} {r["mspt"]["p99"]!s:>6} {r["rss_mb"]["mean"]!s:>7} {r["cpu_percent"]!s:>6} '
              f'{r["chunks_per_second"]!s:>8} {per_bot!s:>6} {r["resent"]!s:>6} {r["pulled_back"]!s:>6} '
              f'{r["bots_joined"]!s:>6}')
    if report['work']:
        print(f'\nWork, against the busiest server (chunks a second sent, new; ticks a second bots waited):')
        for row in report['work']:
            cells = '  '.join(
                f'{server} {w["chunks_per_second"]["value"]} ({w["chunks_per_second"]["share"]:.0%}), '
                f'{w["unique_per_second"]["value"]} ({w["unique_per_second"]["share"]:.0%}), '
                f'{w["waited_ticks_per_second"]}' for server, w in row['servers'].items())
            print(f'{row["behaviour"]:8} {row["bots"]:>4}  {"equal" if row["equal"] else "NOT EQUAL"}  {cells}')
    print(f'\nWrote {path}')


if __name__ == '__main__':
    sys.exit(main())
