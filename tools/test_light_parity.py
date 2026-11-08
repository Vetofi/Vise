#!/usr/bin/env python3
import argparse
import shutil
import subprocess
import sys
from pathlib import Path

try:
    from . import lab
except ImportError:
    import lab

ROOT = Path(__file__).resolve().parent.parent
LOCAL = ROOT / '.local'
RESOURCES = lab.RESOURCES
SERVER = lab.GAME / 'jars' / 'server.jar'

SCENE = [
    'forceload add 0 0 63 63',
    'sleep 6',
    'fill 2 170 2 20 175 20 stone hollow',
    'fill 2 176 2 20 176 20 stone_slab[type=bottom]',
    'setblock 10 171 10 glowstone',
    'fill 4 171 4 4 171 18 oak_stairs[facing=east,half=top]',
    'fill 6 171 4 6 171 18 stone_brick_stairs[facing=north,half=bottom,shape=inner_left]',
    'fill 12 171 4 18 171 6 snow[layers=3]',
    'fill 12 172 8 18 172 8 oak_trapdoor[half=bottom,open=false]',
    'fill 12 173 12 18 173 12 oak_trapdoor[half=top,open=true,facing=north]',
    'setblock 14 171 14 lantern',
    'setblock 15 175 15 soul_lantern[hanging=true]',
    'fill 8 171 16 9 172 17 water',
    'fill 24 170 24 46 170 46 stone',
    'fill 26 171 26 44 171 26 stone_slab[type=top]',
    'fill 26 171 28 44 171 28 stone_slab[type=bottom]',
    'fill 26 172 30 44 172 30 oak_leaves[persistent=true]',
    'fill 26 171 32 30 171 40 glass',
    'fill 32 171 32 36 171 40 oak_stairs[facing=south,half=bottom]',
    'fill 38 171 32 38 175 40 stone_brick_wall',
    'fill 40 171 32 44 171 40 dirt_path',
    'fill 40 172 34 44 172 34 daylight_detector',
    'fill 24 180 50 46 180 50 stone_slab[type=double]',
    'fill 24 178 52 46 178 52 oak_stairs[facing=west,half=top]',
    'setblock 30 171 44 torch',
    'setblock 34 171 44 campfire',
    'setblock 38 171 44 sea_lantern',
    'setblock 42 171 44 lava',
    'sleep 10',
    'setblock 10 171 10 air',
    'setblock 10 176 10 air',
    'setblock 30 171 26 air',
    'fill 26 172 30 30 172 30 air',
    *[f'setblock {x * 16 + 8} 169 {z * 16 + 8} stone' for x in range(3) for z in range(4)],
    'sleep 8',
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--seed', default='4242')
    parser.add_argument('--keep', action='store_true')
    args = parser.parse_args()
    run = LOCAL / 'rust-light-parity'
    shutil.rmtree(run, ignore_errors=True)
    run.mkdir(parents=True)
    command = [sys.executable, str(ROOT / 'tools' / 'vanilla_world_check.py'), '--jar', str(SERVER),
               '--dir', str(run), '--world', 'world', '--seed', args.seed, '--wait-after', '8']
    for step in SCENE:
        command += ['--command', step]
    if subprocess.run(command).returncode:
        sys.exit('vanilla server reported problems')
    dimension = run / 'world' / 'dimensions' / 'minecraft' / 'overworld'
    if not (dimension / 'region').is_dir():
        dimension = run / 'world'
    for extra in (['--chunks', '0', '0', '2', '3'], ['--incremental', '100']):
        status = subprocess.run(['cargo', 'run', '--quiet', '--profile', 'fast', '-p', 'vise-client',
                                 '--example', 'client_probe', '--', 'light_parity', str(RESOURCES), str(dimension), *extra],
                                cwd=ROOT).returncode
        if status:
            sys.exit('light mismatch against vanilla')
    print('light parity ok')
    if not args.keep:
        shutil.rmtree(run, ignore_errors=True)


if __name__ == '__main__':
    main()
