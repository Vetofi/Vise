#!/usr/bin/env python3
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

try:
    from . import lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server
    from .test_locator_bar_parity import generate_world, server_dir
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server
    from test_locator_bar_parity import generate_world, server_dir

NAME = 'server-entities-parity'
STEPS = 200
PLAYER = 'Tester'

SCENE = [
    'fill -7 -60 -6 7 -57 -6 minecraft:stone',
    'fill -7 -60 9 7 -60 9 minecraft:rail',
    'summon minecraft:minecart -5.5 -60 3.5',
    'summon minecraft:chest_minecart -3.5 -60 3.5',
    'summon minecraft:furnace_minecart -1.5 -60 3.5',
    'summon minecraft:hopper_minecart 1.5 -60 3.5',
    'summon minecraft:tnt_minecart 3.5 -60 3.5',
    'summon minecraft:command_block_minecart 5.5 -60 1.5 {Command:"say hi"}',
    'summon minecraft:minecart -6.5 -60 9.5 {Motion:[0.5d,0d,0d]}',
    'summon minecraft:oak_boat -5.5 -60 6.5 {Rotation:[45f,0f]}',
    'summon minecraft:birch_chest_boat -2.5 -60 6.5',
    'summon minecraft:bamboo_raft 0.5 -60 6.5',
    'summon minecraft:oak_boat 4.5 -56 6.5',
    'summon minecraft:armor_stand -2.5 -60 -2.5 {Rotation:[90f,0f]}',
    'summon minecraft:armor_stand 6.5 -57 -2.5',
    'summon minecraft:armor_stand -4.5 -60 -2.5 {ShowArms:1b,Small:1b,NoBasePlate:1b,'
    'Pose:{Head:[10f,20f,0f],RightArm:[-30f,0f,15f]},equipment:{head:{id:"minecraft:diamond_helmet",count:1}}}',
    'summon minecraft:chicken -6.5 -60 -2.5 {NoAI:1b,Passengers:[{id:"minecraft:husk",IsBaby:1b,NoAI:1b}]}',
    'summon minecraft:slime 2.5 -60 1.5 {Size:1,NoAI:1b}',
    'summon minecraft:pig -0.5 -60 1.5 {NoAI:1b}',
    'attribute @e[type=minecraft:pig,limit=1,x=-0.5,y=-60,z=1.5,distance=..1] minecraft:scale base set 1.5',
    'summon minecraft:item_frame -3.5 -59 -4.97 {block_pos:[I;-4,-59,-5],Facing:3b,'
    'Item:{id:"minecraft:diamond",count:1},ItemRotation:3b}',
    'summon minecraft:glow_item_frame -1.5 -59 -4.97 {block_pos:[I;-2,-59,-5],Facing:3b}',
    'summon minecraft:painting 1.5 -59 -4.97 {block_pos:[I;1,-59,-5],facing:0b,variant:"minecraft:kebab"}',
    'summon minecraft:painting 4 -59 -4.97 {block_pos:[I;4,-59,-5],facing:0b,variant:"minecraft:courbet"}',
    'summon minecraft:tnt 6.5 -60 6.5 {fuse:2000s}',
    'summon minecraft:tnt 6.5 -56 4.5 {fuse:2000s}',
    'summon minecraft:falling_block 2.5 -58 -0.5 {BlockState:{Name:"minecraft:sand"},NoGravity:1b}',
    'setblock -7 -58 0 minecraft:cobweb',
    'summon minecraft:falling_block -6.5 -54 0.5 {BlockState:{Name:"minecraft:sand"}}',
    'summon minecraft:arrow 0.5 -55 -1.5 {Motion:[0.3d,0d,0d]}',
    'summon minecraft:end_crystal -6.5 -60 6.5 {ShowBottom:0b}',
]


RANDOM_MODIFIERS = re.compile(r'(id=minecraft:(?:random_spawn_bonus|leader_zombie_bonus), amount=)[^,]+')


def scene_lines(lines):
    out = []
    for line in lines:
        m = re.match(r'play-entities entity \S+ (-?\d+),-?\d+,(-?\d+)', line)
        if m and -8 <= int(m.group(1)) <= 8 and -7 <= int(m.group(2)) <= 10 or line.startswith('play-entities self '):
            out.append(RANDOM_MODIFIERS.sub(r'\1~', line))
    return out


def run_side(label, server_command, client_command, build, world):
    directory = build / label
    port = server_dir(directory, world)
    out = build / f'{label}-client.txt'
    out.unlink(missing_ok=True)
    server = Server(server_command, directory, label)
    try:
        server.wait_for(r'Done \(')
        with (build / f'{label}-client.log').open('w') as log:
            client = subprocess.Popen([*map(str, client_command), '127.0.0.1', str(port), PLAYER, str(out), 'entities'],
                                      cwd=lab.ROOT, stdout=log, stderr=subprocess.STDOUT, text=True)
            try:
                server.wait_for(rf'<{PLAYER}> summon the entities', timeout=180)
                server.console('tick freeze')
                for command in SCENE:
                    server.console(command)
                time.sleep(3)
                server.console(f'tick step {STEPS}')
                client.wait(timeout=120)
            finally:
                if client.poll() is None:
                    client.kill()
    finally:
        server.stop()
    if not out.exists():
        raise AssertionError(f'{label}: the client wrote no transcript (see {build}/{label}-client.log)')
    lines = out.read_text(encoding='utf-8').splitlines()
    (build / f'{label}.txt').write_text('\n'.join(lines) + '\n', encoding='utf-8')
    return lines


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    oracle = lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/VanillaProtocolClient.java'
    classes = build / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir()
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-nowarn', '-sourcepath',
                    lab.ROOT / 'mods/vise/src/test/java', '-cp', dependencies, '-d', classes, oracle],
                   cwd=lab.ROOT, check=True, capture_output=True)
    sides = [s for s in os.environ.get('SERVER_ENTITIES_SIDES', 'vanilla,vise').split(',') if s]
    if 'vise' in sides:
        subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], cwd=lab.ROOT,
                       check=True, capture_output=True)
    import json
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('vise-client.exe' if os.name == 'nt' else 'vise-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    vise_server = [binary, '--resources', lab.LOCAL / 'vise-client/26.3', '--dedicated', '--nogui']
    client = ['java', '-Xmx1g', '-Duser.timezone=UTC', '-cp', str(classes) + os.pathsep + dependencies,
              'dev.viselab.parity.VanillaProtocolClient']

    world = generate_world(vanilla_server, build)
    results = {}
    for label, command in [('vanilla', vanilla_server), ('vise', vise_server)]:
        if label in sides:
            results[label] = run_side(label, command, client, build, world)
        else:
            results[label] = (build / f'{label}.txt').read_text(encoding='utf-8').splitlines()
    vanilla, vise = scene_lines(results['vanilla']), scene_lines(results['vise'])
    errors = [l for l in results['vanilla'] + results['vise'] if l.startswith('error')]
    if errors or vanilla != vise:
        import difflib
        diff = '\n'.join(difflib.unified_diff(vanilla, vise, 'vanilla', 'vise', n=0, lineterm=''))
        raise AssertionError(f'{len(errors)} errors, transcripts differ (see {build}/vanilla.txt and vise.txt):\n'
                             + '\n'.join(errors[:10]) + '\n' + diff[:8000])
    if len(vanilla) < len(SCENE) - 2:
        raise AssertionError(f'only {len(vanilla)} entities tracked: the scene did not run (see {build})')
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        entities=len(vanilla), output_sha256=lab.digest(build / 'vise.txt', 'sha256'),
        scope='Minecarts (one rolling), boats (one falling), armor stands, a chicken jockey, a slime, a pig with a '
              'scale, item frames, paintings, primed TNT, falling blocks (one held by a cobweb), an arrow and an '
              'end crystal '
              'tracked to a vanilla-codec client by a dedicated server: spawn packets, entity data, equipment, '
              'attributes, passengers and the positions the syncs leave them at.'))
    print(f'server entities: {len(vanilla)} entities identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
