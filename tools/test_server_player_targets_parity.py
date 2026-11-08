#!/usr/bin/env python3
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time

try:
    from . import lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server, game_time
    from .test_locator_bar_parity import generate_world, server_dir
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, game_time
    from test_locator_bar_parity import generate_world, server_dir

NAME = 'server-player-targets-parity'
NETHER, OVERWORLD = 'minecraft:the_nether', 'minecraft:overworld'


SCRIPT = [
    ('console-give', ['give Bob minecraft:diamond 3'], []),
    ('console-give-all', ['give @a minecraft:apple'], []),
    ('alice-give-bob', [], ['cmd Alice give Bob minecraft:stick 2']),
    ('console-give-uuid', [f'give {lab.offline_uuid("Bob")} minecraft:iron_ingot'], []),
    ('console-give-nearest', ['give @p minecraft:emerald'], []),
    ('alice-give-random', [], ['cmd Alice give @r[name=Bob] minecraft:gold_ingot']),
    ('console-gamemode', ['gamemode survival Bob'], []),
    ('alice-gamemode-bob', [], ['cmd Alice gamemode creative Bob']),
    ('alice-gamemode-all', [], ['cmd Alice gamemode adventure @a']),
    ('console-gamemode-back', ['gamemode creative @a'], []),
    ('console-tp', ['tp Bob 8.5 -60 8.5'], []),
    ('alice-tp-bob-to-alice', [], ['cmd Alice tp Bob Alice']),
    ('console-tp-rotation', ['tp Bob 4.5 -60 0.5 90 10'], []),
    ('bob-tp-self', [], ['cmd Bob tp @s 6.5 -60 0.5']),
    ('console-execute-as', ['execute as Bob run tp @s ~ ~ ~2'], []),
    ('console-execute-at', ['execute at Bob run tp Alice ~1 ~ ~'], []),
    ('alice-execute-as-all', [], ['cmd Alice execute as @a run give @s minecraft:bread']),
    ('console-effect', ['effect give Bob minecraft:speed 30 1'], []),
    ('alice-effect-all', [], ['cmd Alice effect give @a minecraft:haste infinite']),
    ('console-effect-clear', ['effect clear Bob'], []),
    ('alice-effect-clear-all', [], ['cmd Alice effect clear @a minecraft:haste']),
    ('console-nether', [f'execute in {NETHER} run tp Bob 0.5 100 0.5'], []),
    ('console-tp-home', ['tp Bob 4.5 -60 0.5'], []),
    ('alice-nether', [], [f'cmd Alice execute in {NETHER} run tp Bob 0.5 100 0.5']),
    ('console-tp-to-alice', ['tp Bob Alice'], []),
    ('alice-kill-missing', [], ['cmd Alice kill Carol']),
    ('console-kill', ['kill Bob'], []),
]
DEATH_STEP = 'step=console-kill'
STEP_TICKS = 40
PLAYERS = ('Alice', 'Bob')


def run_side(label, server_command, client_command, build, world):
    directory = build / label
    port = server_dir(directory, world)
    out = build / f'{label}.txt'
    out.unlink(missing_ok=True)
    server = Server(server_command, directory, label)
    try:
        server.wait_for(r'Done \(')
        with (build / f'{label}-client.log').open('w') as log:
            client = subprocess.Popen([*map(str, client_command), '127.0.0.1', str(port), str(out)],
                                      cwd=lab.ROOT, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                      stderr=subprocess.STDOUT, text=True)

            def tell(line):
                client.stdin.write(line + '\n')
                client.stdin.flush()

            def sync():
                for name in PLAYERS:
                    tell(f'sync {name}')
                for name in PLAYERS:
                    while (line := client.stdout.readline()) != f'synced {name}\n':
                        if not line:
                            raise AssertionError(f'{label}: the client exited (see {build}/{label}-client.log)')
                        log.write(line)

            def finish_step():
                sync()
                until = game_time(server) + STEP_TICKS
                while game_time(server) < until:
                    time.sleep(0.2)
                for name in PLAYERS:
                    tell(f'still {name}')
                sync()
            try:
                for name, x in zip(PLAYERS, (0.5, 4.5)):
                    tell(f'join {name}')
                    server.wait_for(rf'{name} joined the game', timeout=120)
                    tell(f'fly {name} on')
                    tell(f'cmd {name} tp @s {x} -60 0.5 0 0')
                finish_step()
                for flag in ('chat', 'state', 'players'):
                    tell(f'{flag} on')
                for step, console, actions in SCRIPT:
                    tell(f'step {step}')
                    for command in console:
                        server.console(command)
                    game_time(server)
                    for action in actions:
                        tell(action)
                    finish_step()
                tell('end')
                log.write(client.communicate(timeout=60)[0])
            finally:
                if client.poll() is None:
                    client.kill()
    finally:
        server.stop()
    if not out.exists():
        raise AssertionError(f'{label}: the client wrote no transcript (see {build}/{label}-client.log)')
    lines = out.read_text(encoding='utf-8').splitlines()
    errors = [l for l in lines if ' error ' in f' {l} ' or 'disconnected' in l]
    steps = {}
    for line in lines:
        who, step, rest = line.split(' ', 2)
        death_side = step == DEATH_STEP and (not rest.startswith(('chat ', 'killed '))
                                             or 'chat.type.advancement' in rest)
        if death_side:
            continue
        if step != 'step=start':
            steps.setdefault((who, step), []).append(line)
    order = {f'step={s}': i for i, (s, _, _) in enumerate(SCRIPT)}
    keys = sorted(steps, key=lambda k: (k[0], order.get(k[1], -1)))
    return [l for k in keys for l in sorted(lab.settled(steps[k]))], errors


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    oracle = lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/LocatorBarClient.java'
    classes = build / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir()
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-nowarn', '-sourcepath',
                    lab.ROOT / 'mods/vise/src/test/java', '-cp', dependencies, '-d', classes, oracle],
                   cwd=lab.ROOT, check=True, capture_output=True)
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], cwd=lab.ROOT,
                   check=True, capture_output=True)
    import json
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('vise-client.exe' if os.name == 'nt' else 'vise-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    vise_server = [binary, '--resources', lab.LOCAL / 'vise-client/26.3', '--dedicated', '--nogui']
    client = ['java', '-Xmx1g', '-cp', str(classes) + os.pathsep + dependencies, 'dev.viselab.parity.LocatorBarClient']

    world = generate_world(vanilla_server, build)
    vanilla, vanilla_errors = run_side('vanilla', vanilla_server, client, build, world)
    vise, vise_errors = run_side('vise', vise_server, client, build, world)
    (build / 'vanilla.txt').write_text('\n'.join(vanilla) + '\n', encoding='utf-8')
    (build / 'vise.txt').write_text('\n'.join(vise) + '\n', encoding='utf-8')
    errors = vanilla_errors + vise_errors
    if errors or vanilla != vise:
        import difflib
        diff = '\n'.join(difflib.unified_diff(vanilla, vise, 'vanilla', 'vise', n=1, lineterm=''))
        raise AssertionError(f'{len(errors)} errors, transcripts differ (see {build}/vanilla.txt and vise.txt):\n'
                             + '\n'.join(errors[:10]) + '\n' + diff[:8000])
    for needle in ('gamemode 0', 'minecraft:diamond x3', 'effect minecraft:speed', 'respawn ' + NETHER,
                   'death.attack.genericKill'):
        if not any(needle in l for l in vanilla):
            raise AssertionError(f'vanilla never sent {needle!r}: the script did not run (see {build})')
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        lines=len(vanilla), output_sha256=lab.digest(build / 'vise.txt', 'sha256'),
        scope='Commands from the console and an operator acting on another player of a dedicated server: names, '
              'UUIDs, @a, @p, @r and @s under execute as/at, with give, gamemode, tp (place, player, rotation, '
              'execute in, across levels), effect give and clear and kill, as the players see them.'))
    print(f'server player targets: {len(vanilla)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
