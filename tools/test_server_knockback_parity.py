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
    from .test_dedicated_server_parity import Server, free_port, game_time, wait_ticks
    from .test_locator_bar_parity import generate_world, ops_json
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, free_port, game_time, wait_ticks
    from test_locator_bar_parity import generate_world, ops_json

NAME = 'server-knockback-parity'
PLAYERS = ('Alice', 'Bob')
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-seed': '12345',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'gamemode': 'survival',
    'difficulty': 'normal',
    'view-distance': '4',
    'simulation-distance': '4',
    'spawn-protection': '0',
    'max-players': '5',
    'pause-when-empty-seconds': '-1',
}
KNOWN = {
    'pvp-mace': "a smash on a player runs none of MaceItem.hurtEnemy (the attacker's fall goes on)",
}

A = (0.5, -60.0, 0.5)
V = (2.5, -60.0, 0.5)
SWORD = 'minecraft:wooden_sword'
KNOCKBACK_SWORD = 'minecraft:wooden_sword[enchantments={"minecraft:knockback":2}]'


def stand(name, at, yaw, ticks=3):
    return [f'pos {name} {at[0]} {at[1]} {at[2]} {yaw} 0 1'] * ticks


def falling(height):
    return [f'pos Alice {A[0]} {A[1] + h} {A[2]} -90 0 0' for h in (height, height - 0.3)]


def mob_case(kind, weapon=SWORD, sprint=False, crit=False, air=False):
    mob = f'@e[type=minecraft:{kind},limit=1]'
    summon = [f'cmd Alice summon minecraft:{kind} {V[0]} {V[1] + (1.5 if air else 0)} {V[2]} {{Rotation:[0f,0f]}}',
              f'cmd Alice effect give {mob} minecraft:slowness 100000 127 true']
    lines = ['cmd Alice kill @e[type=!player]', 'cmd Alice gamemode creative', 'cmd Bob gamemode creative',
             f'cmd Alice item replace entity @s weapon.mainhand with {weapon}', 'cmd Alice tp Bob 2.5 -60 12.5',
             *stand('Alice', A, -90), 'ticks 24']
    lines += ['cmd Alice tick freeze', 'frozen', *summon] if air else [*summon, 'ticks 30', 'cmd Alice tick freeze', 'frozen']
    lines += [f'cmd Alice data get entity {mob} Pos']
    if sprint:
        lines.append('sprint Alice on')
    if crit:
        lines += falling(0.8)
    lines += [f'hit {kind}', f'cmd Alice data get entity {mob} Motion']
    for _ in range(20):
        lines += ['tickstep', f'cmd Alice data get entity {mob} Pos', f'cmd Alice data get entity {mob} Motion']
    lines += ['cmd Alice tick unfreeze', 'cmd Alice kill @e[type=!player]']
    if sprint:
        lines.append('sprint Alice off')
    lines += [*stand('Alice', A, -90), 'cmd Alice gamemode survival', 'cmd Bob gamemode survival',
              f'cmd Alice tp Bob {V[0]} {V[1]} {V[2]} 90 0']
    return lines


def pvp_case(weapon=SWORD, sprint=False, crit=False, shield=False, armour=False, twice=False, mace=False,
             cooldown=False):
    lines = [f'cmd Alice item replace entity @s weapon.mainhand with {weapon}', 'cmd Bob clear @s',
             'cmd Bob effect give @s minecraft:resistance 1000 4 true',
             'cmd Bob effect give @s minecraft:instant_health 1 5 true', *stand('Alice', A, -90), *stand('Bob', V, 90)]
    if armour:
        lines += [f'cmd Bob item replace entity @s armor.{slot} with minecraft:netherite_{piece}'
                  for slot, piece in (('head', 'helmet'), ('chest', 'chestplate'), ('legs', 'leggings'),
                                      ('feet', 'boots'))]
    if shield:
        lines += ['cmd Bob item replace entity @s weapon.mainhand with minecraft:shield', 'use Bob']
    lines += ['ticks 30', *stand('Bob', V, 90, ticks=1)]
    if sprint:
        lines.append('sprint Alice on')
    if crit:
        lines += falling(0.8)
    if mace:
        lines += falling(3.0) + falling(1.5)
    if cooldown:
        lines += ['ticks 4', 'punch Alice', 'attack Alice Bob']
    lines += ['sleep 0.2' if cooldown else 'ticks 4', 'attack Alice Bob', 'ticks 6']
    if twice:
        lines += [*stand('Bob', V, 90, ticks=20), 'ticks 30', 'attack Alice Bob', 'ticks 6']
    if sprint:
        lines.append('sprint Alice off')
    lines += stand('Alice', A, -90)
    if shield or armour:
        lines.append('cmd Bob clear @s')
    return lines


def mob_hits_case(kind):
    mob = f'@e[type=minecraft:{kind},limit=1]'
    hit = ['mark Bob', f'cmd Alice summon minecraft:{kind} {V[0] + 1} {V[1]} {V[2]} {{Rotation:[90f,0f]}}',
           f'cmd Alice effect give {mob} minecraft:slowness 100000 127 true', 'await Bob', f'cmd Alice kill {mob}']
    return ['cmd Bob clear @s', 'cmd Bob effect give @s minecraft:resistance 1000 4 true', 'cmd Alice gamemode creative',
            *stand('Bob', V, 90), *hit, 'ticks 40', *stand('Bob', V, 90), *hit, 'ticks 20',
            'cmd Alice kill @e[type=!player]', 'cmd Alice gamemode survival']


def blast_case(blast, row):
    at, z = (V[0], V[1], V[2] + row), V[2] + row
    summon = {'crystal': f'cmd Alice summon minecraft:end_crystal {at[0] + 2} {at[1]} {z} {{ShowBottom:0b}}',
              'tnt': f'cmd Alice summon minecraft:tnt {at[0] + 2} {at[1]} {z} {{fuse:20s}}'}[blast]
    lines = ['cmd Alice kill @e[type=!player]', 'cmd Bob gamemode creative',
             f'cmd Alice tp Bob {at[0]} {at[1]} {at[2]} -90 0', *stand('Bob', at, -90), 'ticks 20', summon, 'ticks 5']
    lines += ['attack Bob end_crystal', 'ticks 10'] if blast == 'crystal' else ['ticks 30']
    return lines + [*stand('Bob', at, -90), 'cmd Bob gamemode survival', 'cmd Alice kill @e[type=!player]',
                    f'cmd Alice tp Bob {V[0]} {V[1]} {V[2]} 90 0', *stand('Bob', V, 90)]


CASES = {
    'cow-plain': mob_case('cow'),
    'cow-sprint': mob_case('cow', sprint=True),
    'cow-knockback-2': mob_case('cow', weapon=KNOCKBACK_SWORD),
    'cow-crit': mob_case('cow', crit=True),
    'cow-air': mob_case('cow', air=True),
    'zombie-plain': mob_case('zombie'),
    'zombie-sprint': mob_case('zombie', sprint=True),
    'zombie-knockback-2': mob_case('zombie', weapon=KNOCKBACK_SWORD),
    'zombie-crit': mob_case('zombie', crit=True),
    'zombie-air': mob_case('zombie', air=True),
    'pvp-plain': pvp_case(),
    'pvp-sprint': pvp_case(sprint=True),
    'pvp-knockback-2': pvp_case(weapon=KNOCKBACK_SWORD),
    'pvp-crit': pvp_case(crit=True),
    'pvp-shield': pvp_case(shield=True),
    'pvp-netherite': pvp_case(armour=True),
    'pvp-twice': pvp_case(twice=True),
    'pvp-sprint-twice': pvp_case(sprint=True, twice=True),
    'pvp-mace': pvp_case(weapon='minecraft:mace', mace=True),
    'pvp-cooldown': pvp_case(cooldown=True),
    'zombie-hits-bob': mob_hits_case('zombie'),
    'crystal-creative': blast_case('crystal', 16),
    'tnt-creative': blast_case('tnt', 32),
}
SETUP = ['cmd Alice gamerule spawn_mobs false', 'cmd Alice gamerule spawn_monsters false',
         'cmd Alice kill @e[type=!player]', 'cmd Alice gamerule advance_time false', 'cmd Alice time set midnight',
         f'cmd Alice tp Alice {A[0]} {A[1]} {A[2]} -90 0', f'cmd Alice tp Bob {V[0]} {V[1]} {V[2]} 90 0', 'ticks 40']


def server_data(server, selector, path, timeout=120):
    deadline = time.time() + timeout
    start = len(server.lines)
    server.console(f'data get entity {selector} {path}')
    while time.time() < deadline:
        for line in server.lines[start:]:
            if 'has the following entity data' in line:
                return line.split('data: ', 1)[-1]
        time.sleep(0.02)
    raise AssertionError(f'{server.log_path.name}: no entity data in {timeout} s')


def server_dir(directory, world):
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    port = free_port()
    props = dict(PROPERTIES, **{'server-port': str(port)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()),
                                                 encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    (directory / 'ops.json').write_text(ops_json())
    shutil.copytree(world, directory / 'world')
    (directory / 'world' / 'session.lock').unlink(missing_ok=True)
    return port


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

            def sync():
                for name in PLAYERS:
                    client.stdin.write(f'sync {name}\n')
                client.stdin.flush()
                for name in PLAYERS:
                    while (reply := client.stdout.readline()) != f'synced {name}\n':
                        if not reply:
                            raise AssertionError(f'{label}: the client exited (see {build}/{label}-client.log)')
                        log.write(reply)

            def send(line):
                word = line.split()[0]
                if word == 'sleep':
                    time.sleep(float(line.split()[1]))
                elif word == 'ticks':
                    sync()
                    wait_ticks(server, int(line.split()[1]))
                    sync()
                elif word == 'frozen':
                    sync()
                    while game_time(server) != game_time(server):
                        pass
                elif word == 'hit':
                    sync()
                    mob = f'@e[type=minecraft:{line.split()[1]},limit=1]'
                    before = server_data(server, mob, 'Motion')
                    client.stdin.write(f'attack Alice {line.split()[1]}\n')
                    client.stdin.flush()
                    while server_data(server, mob, 'Motion') == before:
                        time.sleep(0.05)
                    sync()
                elif word == 'tickstep':
                    sync()
                    before = game_time(server)
                    client.stdin.write('cmd Alice tick step 1\n')
                    client.stdin.flush()
                    while game_time(server) == before:
                        time.sleep(0.05)
                    sync()
                else:
                    client.stdin.write(line + '\n')
                    client.stdin.flush()
            try:
                for name in PLAYERS:
                    send(f'join {name}')
                    server.wait_for(rf'{name} joined the game', timeout=120)
                    wait_ticks(server, 40)
                for line in SETUP:
                    send(line)
                for case, lines in CASES.items():
                    send(f'step {case}')
                    for line in lines:
                        send(line)
                    send('ticks 10')
                send('end')
                log.write(client.communicate(timeout=60)[0])
            finally:
                if client.poll() is None:
                    client.kill()
    finally:
        server.stop()
    if not out.exists():
        raise AssertionError(f'{label}: the client wrote no transcript (see {build}/{label}-client.log)')
    return out.read_text(encoding='utf-8').splitlines()


DATA = re.compile(r'^chat \S+ has the following entity data: \[(.*)\]$')


def compared(lines):
    cases = {}
    for line in lines:
        who, step, text = line.split(' ', 2)
        step = step.removeprefix('step=')
        if step not in CASES:
            continue
        kept = cases.setdefault(step, [])
        data = DATA.match(text)
        if who == 'Alice' and data:
            kept.append('data ' + ' '.join(f'{float(v.rstrip("d")):.9f}' for v in data.group(1).split(', ')))
        elif text.startswith(('motion self', 'sprinting', 'explode push')):
            kept.append(f'{who} {text}')
        elif text.startswith('error') or text.startswith('disconnected'):
            kept.append(f'{who} {text}')
    return cases


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    oracle = lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/KnockbackClient.java'
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
    client = ['java', '-Xmx1g', '-cp', str(classes) + os.pathsep + dependencies, 'dev.viselab.parity.KnockbackClient']

    world = generate_world(vanilla_server, build)
    vanilla = compared(run_side('vanilla', vanilla_server, client, build, world))
    vise = compared(run_side('vise', vise_server, client, build, world))
    differ, fixed = [], []
    for case in CASES:
        want, got = vanilla.get(case, []), vise.get(case, [])
        if case not in vanilla or any(' error ' in f' {l} ' or 'disconnected' in l for l in want):
            raise AssertionError(f'{case}: vanilla recorded nothing or failed: {want}')
        if want == got:
            if case in KNOWN:
                fixed.append(case)
            continue
        if case in KNOWN:
            continue
        import difflib
        differ.append(f'{case}:\n' + '\n'.join(difflib.unified_diff(want, got, 'vanilla', 'vise', n=1, lineterm='')))
    if fixed:
        print('matching vanilla now, take them out of KNOWN: ' + ', '.join(fixed))
    if differ:
        raise AssertionError(f'{len(differ)} cases differ (see {build}/vanilla.txt and vise.txt):\n'
                             + '\n'.join(differ)[:8000])
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        cases=len(CASES), known=sorted(KNOWN), output_sha256=lab.digest(build / 'vise.txt', 'sha256'),
        scope='Knockback on a dedicated server, as two vanilla-codec players get it: a player hitting a cow and '
              'a zombie (plain, sprinting, Knockback II, critical, in the air; the mob\'s motion and path over 20 '
              'ticks), a player hitting a player (the same, a raised shield, netherite, two hits, a mace smash; '
              'the victim\'s motion packet and the attacker\'s sprint) and a zombie hurting a player.'))
    print(f'server knockback: {len(CASES) - len(KNOWN)} of {len(CASES)} cases identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
