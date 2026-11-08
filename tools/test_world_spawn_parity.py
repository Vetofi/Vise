#!/usr/bin/env python3
import argparse
import hashlib
import json
import os
import random
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

try:
    from . import anvil_dump, lab, oracle_harness as oh, parity_shard, test_rust_parity as base
    from .test_dedicated_server_parity import Server, free_port
except ImportError:
    import anvil_dump
    import lab
    import oracle_harness as oh
    import parity_shard
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, free_port

NAME = 'world-spawn-parity'
PARITY_SHARDS = 4
REPORT = 1891623539983467248
SEEDS = [REPORT, 12345, -7046029254386353131, 987654321987, 4242, -1, 0x5DEECE66D, 20260607, 777, 0]
RESOURCES = lab.RESOURCES


def seeds(count):
    rng = random.Random(20261014)
    return SEEDS + [rng.randrange(-2**63, 2**63) for _ in range(count)]


def target_dir(profile='release'):
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    return Path(metadata['target_directory']) / profile


def origins(build, classes, dependencies, all_seeds):
    vanilla_out = build / 'origins-vanilla.txt'
    oh.run_logged(['java', '-cp', str(classes) + os.pathsep + dependencies, 'dev.viselab.parity.VanillaWorldSpawnOracle',
                   vanilla_out, *all_seeds], build, 'origins-vanilla')
    rust = subprocess.run([target_dir() / 'examples/world_probe', 'spawn_origin', RESOURCES, *map(str, all_seeds)],
                          capture_output=True, text=True, check=True, cwd=lab.ROOT).stdout
    vanilla = vanilla_out.read_text()
    parse = lambda text: {int(l.split()[0]): tuple(map(int, l.split()[1:])) for l in text.splitlines() if l}
    return parse(vanilla), parse(rust)


def properties(seed):
    return {'online-mode': 'false', 'level-seed': str(seed), 'view-distance': '4', 'simulation-distance': '4',
            'pause-when-empty-seconds': '-1'}


def vanilla_cache(lock, command, classpath):
    root = os.environ.get('PARITY_SHIM_CACHE')
    if not root or os.environ.get('PARITY_JAVA_CACHE') == '0':
        return None
    java = subprocess.run(['java', '-version'], capture_output=True, text=True, check=True).stderr
    server = {'jar': lock['minecraft']['server_sha1'], 'java': java,
              'command': [part for part in command if part != classpath]}

    def record(seed):
        identity = json.dumps({**server, 'properties': properties(seed)}, sort_keys=True)
        return Path(root) / f'world-spawn-{hashlib.sha256(identity.encode()).hexdigest()}.json'
    return record


def world_spawn(command, directory, seed, log_name):
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    props = {**properties(seed), 'server-port': str(free_port())}
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()), encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    server = Server(command, directory, log_name)
    try:
        server.wait_for(r'Done \(', timeout=900)
    finally:
        server.stop()
    data = anvil_dump.read_dat(directory / 'world/level.dat')['Data']
    return tuple(data['spawn']['pos'][1])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--random', type=int, default=50, help='random seeds besides the fixed ones')
    parser.add_argument('--jobs', type=int, default=max(1, min(4, int(os.environ.get('FLEET_LOAD_JOBS', '4')) // 4)),
                        help='worlds created at once (each side)')
    parser.add_argument('--origins-only', action='store_true', help='compare the spawn search origins only')
    parser.add_argument('--seed', type=int, action='append', help='only these seeds (repeatable)')
    args = parser.parse_args()
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME / parity_shard.label()
    build.mkdir(parents=True, exist_ok=True)
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    oh.run_logged(['javac', '--release', lock['minecraft']['java_major'], '-proc:none', '-nowarn', '-cp', dependencies,
                   '-d', classes, lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/VanillaWorldSpawnOracle.java'],
                  build, 'compile-java')
    oh.run_logged(['cargo', 'build', '--release', '-p', 'vise-world', '--example', 'world_probe'], build, 'compile-probe')
    all_seeds = parity_shard.shard(args.seed or seeds(args.random))
    started = time.time()

    vanilla, rust = origins(build, classes, dependencies, all_seeds)
    origin_diffs = [s for s in all_seeds if vanilla.get(s) != rust.get(s)]
    for s in origin_diffs:
        print(f'origin {s}: vanilla column {vanilla.get(s)} / vise {rust.get(s)}')
    print(f'Origins: {len(all_seeds) - len(origin_diffs)}/{len(all_seeds)} seeds match '
          f'(column x z, chunk x z) in {time.time() - started:.0f}s', flush=True)
    if args.origins_only:
        sys.exit(1 if origin_diffs else 0)

    oh.run_logged(['cargo', 'build', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], build,
                  'compile-rust', timeout=3600)
    binary = target_dir('fast') / ('vise-client.exe' if os.name == 'nt' else 'vise-client')
    vanilla_server = ['java', '-Xmx2g', '-Dmax.bg.threads=1', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    vise_server = [binary, '--resources', RESOURCES, '--dedicated', '--nogui']
    os.environ.update(lab.game_env())
    cached = vanilla_cache(lock, vanilla_server, dependencies)
    reused = []

    def vanilla_spawn(seed):
        record = cached and cached(seed)
        if record and record.exists():
            reused.append(seed)
            return tuple(json.loads(record.read_text()))
        spawn = world_spawn(vanilla_server, build / f'vanilla-{seed}', seed, f'vanilla-{seed}')
        if record:
            temporary = record.with_suffix(f'.{os.getpid()}.tmp')
            temporary.write_text(json.dumps(spawn))
            temporary.replace(record)
        return spawn

    def one(seed):
        t0 = time.time()
        a = vanilla_spawn(seed)
        b = world_spawn(vise_server, build / f'vise-{seed}', seed, f'vise-{seed}')
        return seed, a, b, time.time() - t0

    spawn_diffs = []
    with ThreadPoolExecutor(args.jobs) as pool:
        for seed, a, b, took in pool.map(one, all_seeds):
            ok = a == b
            if not ok:
                spawn_diffs.append(seed)
            print(f'{"ok  " if ok else "DIFF"} {seed}: vanilla {a[0]} {a[1]} {a[2]} / vise {b[0]} {b[1]} {b[2]} '
                  f'(origin chunk {vanilla[seed][2]} {vanilla[seed][3]}; {took:.0f}s)', flush=True)
            if ok:
                for path in build.glob(f'*-{seed}*'):
                    shutil.rmtree(path, ignore_errors=True)
    print(f'World spawns: {len(all_seeds) - len(spawn_diffs)}/{len(all_seeds)} seeds match in {time.time() - started:.0f}s'
          + (f' (vanilla reused for {len(reused)})' if reused else ''))
    if origin_diffs or spawn_diffs:
        sys.exit(1)


if __name__ == '__main__':
    main()
