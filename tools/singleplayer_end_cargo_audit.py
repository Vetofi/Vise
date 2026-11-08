#!/usr/bin/env python3
import argparse
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time

from agent_control_e2e import Cli
import lab


GATEWAY = (104, 50, 0)
DIAMONDS = 7


def data_get(cli, selector, path):
    since = cli('events')['last']
    cli('command', command=f'data get entity {selector} {path}')
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        reply = cli('events', since=since)
        for event in reply['events']:
            text = event.get('text', '')
            if event.get('event') == 'chat' and ('entity data' in text or 'No entity was found' in text):
                return text
        time.sleep(0.1)
    raise AssertionError(f'no answer for /data get entity {selector} {path}')


def numbers(text):
    return [float(v) for v in re.findall(r'-?\d+(?:\.\d+)?', text.split(':', 1)[-1])]


def run(bin_dir, resources, in_frame):
    with tempfile.TemporaryDirectory(prefix='hz-end-gateway-') as scratch:
        work = Path(scratch)
        env = lab.game_env({k: v for k, v in os.environ.items() if k not in (
            'VISE_HOME', 'VISE_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME')})
        home = Path(env['VISE_HOME'])
        socket = work / 'game.sock'
        cli = Cli(bin_dir / 'vise-ctl', socket, home, env)
        args = [str(bin_dir / 'vise-client'), '--resources', str(resources),
                '--world', 'cargo', '--creative', '--seed', '4242', '--offscreen',
                '--render-distance', '4', '--control', '--control-socket', str(socket)]
        if in_frame:
            args.append('--in-frame')
        with (work / 'game.log').open('w') as log:
            process = subprocess.Popen(args, cwd=home, env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 120
                while not socket.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError('game did not open its control socket')
                    time.sleep(0.1)
                cli('wait_ready', seconds=240)
                cli('command', command='execute in minecraft:the_end run tp @s 100 50 0')
                for _ in range(60):
                    cli('wait', ticks=20)
                    if cli('state').get('dimension') == 'minecraft:the_end':
                        break
                else:
                    raise AssertionError('did not enter the End')
                cli('wait_ready', seconds=240)
                x, y, z = GATEWAY
                cli('command', command=(
                    f'execute in minecraft:the_end run setblock {x} {y} {z} minecraft:end_gateway'))
                cli('command', command=(
                    f'execute in minecraft:the_end run summon minecraft:cow {x}.5 {y}.5 {z}.5 {{NoAI:1b,Tags:["vise_gateway_cow"]}}'))
                nbt = '{Tags:["vise_gateway_item"],Item:{id:"minecraft:diamond",count:' + str(DIAMONDS) + '}}'
                cli('command', command=(
                    'execute in minecraft:the_end run summon minecraft:item '
                    + f'{x}.5 {y}.5 {z}.5 ' + nbt))
                cli('wait', ticks=120)
                length = math.hypot(x, z)
                ex, ez = 1024 * x / length, 1024 * z / length
                cli('command', command=f'execute in minecraft:the_end run tp @s {ex:.1f} 80 {ez:.1f}')
                for _ in range(60):
                    cli('wait', ticks=20)
                    cow = data_get(cli, '@e[type=cow,tag=vise_gateway_cow,limit=1]', 'Pos')
                    if 'entity data' in cow:
                        break
                else:
                    raise AssertionError('the cow never arrived')
                item = data_get(cli, '@e[type=item,tag=vise_gateway_item,limit=1]', 'Pos')
                count = numbers(data_get(cli, '@e[type=item,tag=vise_gateway_item,limit=1]', 'Item.count'))
                cow_at, item_at = numbers(cow), numbers(item)
                print(f'cow at {cow_at}, item at {item_at}, count {count}')
                assert len(cow_at) == 3, f'the cow has no position: {cow_at}'
                assert len(item_at) == 3, f'the stack has no position: {item_at}'
                for name, at in (('cow', cow_at), ('item', item_at)):
                    assert abs(at[0] - x) > 64 or abs(at[2] - z) > 64, \
                        f'{name} stayed in the gateway at {at}'
                assert count and int(count[0]) == DIAMONDS, f'the stack lost diamonds: {count}'
                cli('quit')
                assert process.wait(timeout=120) == 0
            except Exception:
                log.flush()
                text = (work / 'game.log').read_text(errors='replace')
                Path('/tmp/end-gateway-game.log').write_text(text)
                for log in (home / 'logs').glob('*'):
                    if log.is_file():
                        Path('/tmp/end-gateway-' + log.name).write_text(
                            log.read_text(errors='replace'))
                print(text[-4000:])
                raise
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=30)
        print(f'PASS end gateway: {"in-frame" if in_frame else "server-thread"}, '
              f'the cow and the stack left the gateway with their data')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case', choices=['gateway'], default='gateway')
    parser.add_argument('--bin-dir', type=Path, default=lab.cargo_target() / 'fast')
    parser.add_argument('--resources', type=Path, default=lab.RESOURCES)
    parser.add_argument('--mode', choices=['server', 'in-frame', 'both'], default='in-frame')
    args = parser.parse_args()
    modes = {'server': (False,), 'in-frame': (True,), 'both': (False, True)}
    for in_frame in modes[args.mode]:
        run(args.bin_dir.resolve(), args.resources.resolve(), in_frame)


if __name__ == '__main__':
    main()
