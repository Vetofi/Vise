#!/usr/bin/env python3
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab
from server_bench import DONE, Server

EXE = '.exe' if os.name == 'nt' else ''
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'gamemode': 'survival',
    'difficulty': 'easy',
    'pvp': 'true',
    'spawn-protection': '0',
    'view-distance': '3',
    'simulation-distance': '3',
}
CASES = [
    (3, 'survival', True),
    (6, 'survival', True),
    (7, 'survival', False),
    (50, 'survival', False),
    (7, 'creative', True),
    (9, 'creative', False),
]
GROUND = -60
SECONDS = 16


class Failed(Exception):
    pass


def bot(bins, port, out, name, *extra):
    report = out / f'{name}.json'
    subprocess.run([bins / f'vise-bots{EXE}', '--server', f'127.0.0.1:{port}', '--bots', '1',
                    '--behaviour', 'stand', '--seconds', str(SECONDS), '--name', name, '--report', report, *extra],
                   check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=SECONDS + 180)
    return json.loads(report.read_text())['per_bot'][0]


def run(bins, folder, out):
    folder.mkdir(parents=True)
    port = lab.free_port()
    props = dict(PROPERTIES, **{'server-port': str(port)})
    (folder / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()))
    (folder / 'eula.txt').write_text('eula=true\n')
    command = [bins / f'vise-client{EXE}', '--resources', lab.RESOURCES, '--dedicated', '--nogui']
    server = Server(command, folder, out / 'server.log')
    try:
        server.wait_for(DONE, 300)
        with ThreadPoolExecutor(2 * len(CASES)) as pool:
            jobs = []
            for i, _ in enumerate(CASES):
                jobs.append((pool.submit(bot, bins, port, out, f'V{i}'),
                             pool.submit(bot, bins, port, out, f'A{i}', '--attack', f'V{i}0',
                                         '--attack-after', '6')))
            for i, (_, mode, _) in enumerate(CASES):
                for name in (f'V{i}0', f'A{i}0'):
                    server.wait_for(rf'{name} joined the game', 60)
                z = 200 * i + 0.5
                server.console(f'gamemode survival V{i}0')
                server.console(f'gamemode {mode} A{i}0')
                server.console(f'tp V{i}0 0.5 {GROUND} {z}')
                server.console(f'tp A{i}0 1.5 {GROUND} {z}')
            time.sleep(2)
            for i, (distance, _, _) in enumerate(CASES):
                server.console(f'tp A{i}0 {0.5 + distance} {GROUND} {200 * i + 0.5}')
            results = [(v.result(), a.result()) for v, a in jobs]
    finally:
        server.stop()
    failed = []
    for (distance, mode, hits), (victim, attacker) in zip(CASES, results):
        health = victim['health']
        hit = health is not None and health < 20
        line = (f'{mode} attacker at {distance} blocks: {attacker["attacks"]} attacks, victim health {health} '
                f'({"hit" if hit else "refused"}, expected {"hit" if hits else "refused"})')
        print(line)
        if attacker['attacks'] == 0 or health is None:
            failed.append(f'{line}: the bots never fought ({attacker["error"]}, {victim["error"]})')
        elif hit != hits:
            failed.append(line)
    if failed:
        raise Failed('\n'.join(failed))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--bin-dir', default=str(lab.ROOT / 'target/fast'))
    parser.add_argument('--out', default=str(lab.LOCAL / 'server-reach-e2e'))
    args = parser.parse_args()
    bins, out = Path(args.bin_dir).resolve(), Path(args.out)
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    os.environ.update(lab.game_env())
    with tempfile.TemporaryDirectory(prefix='server-reach-e2e-') as scratch:
        try:
            run(bins, Path(scratch) / 'server', out)
        except (Failed, RuntimeError) as e:
            sys.exit(f'server_reach_e2e failed (logs in {out}):\n{e}')
    print('server_reach_e2e: every attack within vanilla\'s range hit, every one beyond it was refused')


if __name__ == '__main__':
    main()
