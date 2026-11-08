#!/usr/bin/env python3
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys

try:
    from . import lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server, free_port
    from .test_locator_bar_parity import generate_world
except ImportError:
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, free_port
    from test_locator_bar_parity import generate_world

NAME = 'server-resource-pack-parity'
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-seed': '12345',
    'level-type': 'minecraft\\:flat',
    'generate-structures': 'false',
    'view-distance': '2',
    'simulation-distance': '2',
    'max-players': '5',
    'pause-when-empty-seconds': '-1',
}
URL = 'http://127.0.0.1:9/vise-test-pack.zip'
SERVERS = {
    'required': {
        'resource-pack': URL,
        'resource-pack-id': '5b2c7a4e-1f3d-4c8b-9a6e-2d4f8b1c3e5a',
        'resource-pack-sha1': '0123456789abcdef0123456789abcdef01234567',
        'require-resource-pack': 'true',
        'resource-pack-prompt': '["Our pack: ",{"text":"blue dirt","color":"aqua","bold":true}]',
    },
    'optional': {
        'resource-pack': URL,
        'require-resource-pack': 'false',
    },
}
PLAYERS = [
    ('required', 'Alice', 0, 'declined', None),
    ('required', 'Bob', 0, 'accepted,downloaded,successfully_loaded', 'declined'),
    ('required', 'Carol', 0, 'accepted,failed_download', None),
    ('required', 'Dave', 20, 'accepted,downloaded,successfully_loaded', None),
    ('optional', 'Erin', 0, 'declined', None),
]
REJECTION = re.compile(r'Disconnecting (\S+) due to resource pack (\S+) rejection')


def server_dir(directory, world, pack):
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    port = free_port()
    props = dict(PROPERTIES, **pack, **{'server-port': str(port)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()),
                                                 encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    shutil.copytree(world, directory / 'world')
    (directory / 'world' / 'session.lock').unlink(missing_ok=True)
    return port


def run_side(label, kind, server_command, client_command, build, world):
    directory = build / f'{label}-{kind}'
    port = server_dir(directory, world, SERVERS[kind])
    out = build / f'{label}-{kind}.txt'
    out.unlink(missing_ok=True)
    server = Server(server_command, directory, f'{label}-{kind}')
    try:
        server.wait_for(r'Done \(')
        with (build / f'{label}-{kind}-client.log').open('w') as log:
            client = subprocess.Popen([*map(str, client_command), '127.0.0.1', str(port), str(out)],
                                      cwd=lab.ROOT, stdin=subprocess.PIPE, stdout=log, stderr=subprocess.STDOUT,
                                      text=True)
            try:
                for server_kind, name, delay, answers, in_play in PLAYERS:
                    if server_kind != kind:
                        continue
                    client.stdin.write(f'join {name} {delay} {answers}\n')
                    if in_play:
                        client.stdin.write(f'play {name} {in_play}\n')
                client.stdin.write('end\n')
                client.stdin.flush()
                client.wait(timeout=180)
            finally:
                if client.poll() is None:
                    client.kill()
    finally:
        server.stop()
    if not out.exists():
        raise AssertionError(f'{label}: the client wrote no transcript (see {build}/{label}-{kind}-client.log)')
    rejections = [' '.join(m.groups()) for m in map(REJECTION.search, server.lines) if m]
    return out.read_text(encoding='utf-8').splitlines() + [f'log rejected {r}' for r in rejections]


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    oracle = lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/ServerPackClient.java'
    classes = build / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir()
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none', '-nowarn', '-sourcepath',
                    lab.ROOT / 'mods/vise/src/test/java', '-cp', dependencies, '-d', classes, oracle],
                   cwd=lab.ROOT, check=True, capture_output=True)
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], cwd=lab.ROOT,
                   check=True, capture_output=True)
    binary = lab.cargo_target() / 'fast' / ('vise-client.exe' if os.name == 'nt' else 'vise-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    vise_server = [binary, '--resources', lab.LOCAL / 'vise-client/26.3', '--dedicated', '--nogui']
    client = ['java', '-Xmx1g', '-cp', str(classes) + os.pathsep + dependencies, 'dev.viselab.parity.ServerPackClient']

    world = generate_world(vanilla_server, build)
    differ = []
    for kind in SERVERS:
        vanilla = run_side('vanilla', kind, vanilla_server, client, build, world)
        vise = run_side('vise', kind, vise_server, client, build, world)
        if not any('offer ' in l for l in vanilla) or any(' error ' in f' {l} ' for l in vanilla):
            raise AssertionError(f'{kind}: vanilla offered no pack or the client failed: {vanilla}')
        if vanilla != vise:
            import difflib
            differ.append(f'{kind}:\n' + '\n'.join(difflib.unified_diff(vanilla, vise, 'vanilla', 'vise',
                                                                        n=2, lineterm='')))
    if differ:
        raise AssertionError(f'the servers differ (see {build}):\n' + '\n'.join(differ)[:8000])
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        players=len(PLAYERS),
        scope='A dedicated server\'s resource pack as vanilla-codec players see it: the offer during configuration '
              '(id, URL, hash, required, prompt), its place among the configuration packets, the keep-alives while '
              'the player decides, and each answer\'s outcome (declined, loaded, failed download, declined in '
              'play) for a required and an optional pack, with the server\'s rejection log lines.'))
    print(f'server resource pack: {len(PLAYERS)} players identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
