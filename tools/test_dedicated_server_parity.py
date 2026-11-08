#!/usr/bin/env python3
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import struct
import subprocess
import sys
import threading
import tempfile
import time
import zlib

try:
    from . import lab, oracle_harness as oh, test_rust_parity as base
    from .integration_env import game_environment, run_game
    from .lab import free_port
except ImportError:
    from integration_env import game_environment, run_game
    import lab
    import oracle_harness as oh
    import test_rust_parity as base
    from lab import free_port

NAME = 'dedicated-server-parity'
SEED = 12345
PLAYER = 'Tester'
PROPERTIES = {
    'online-mode': 'false',
    'white-list': 'false',
    'level-seed': str(SEED),
    'view-distance': '4',
    'simulation-distance': '4',
    'spawn-protection': '0',
    'max-players': '5',
    'motd': '\\u00a7aVise \\u00a7rparity \\u00e9t\\u00e9',
    'pause-when-empty-seconds': '-1',
}


def icon_png():
    rows = b''.join(b'\0' + b''.join(bytes((x * 4, y * 4, (x ^ y) * 4, 255)) for x in range(64)) for y in range(64))

    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', zlib.crc32(kind + data))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 64, 64, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def write_server_dir(directory, port, world=None):
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    props = dict(PROPERTIES, **{'server-port': str(port)})
    (directory / 'server.properties').write_text(''.join(f'{k}={v}\n' for k, v in props.items()), encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    (directory / 'server-icon.png').write_bytes(icon_png())
    if world is not None:
        shutil.copytree(world, directory / 'world')
        for lock in (directory / 'world').glob('session.lock'):
            lock.unlink()


class Server:

    def __init__(self, command, directory, log_name):
        self.log_path = directory.parent / f'{log_name}.log'
        self.log = self.log_path.open('w', encoding='utf-8')
        self.lines = []
        self.scratch = tempfile.TemporaryDirectory(prefix='hz-server-')
        env = game_environment(os.environ, self.scratch.name)
        self.process = subprocess.Popen(list(map(str, command)), cwd=directory, env=env, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                                        encoding='utf-8', errors='replace')
        self.reader = threading.Thread(target=self._read, daemon=True)
        self.reader.start()

    def _read(self):
        for line in self.process.stdout:
            self.lines.append(line.rstrip('\n'))
            self.log.write(line)
            self.log.flush()

    def wait_for(self, pattern, timeout=300):
        deadline = time.time() + timeout
        regex = re.compile(pattern)
        while time.time() < deadline:
            if any(regex.search(l) for l in self.lines):
                return
            if self.process.poll() is not None:
                time.sleep(0.5)
                raise AssertionError(f'{self.log_path.name}: server exited ({self.process.returncode}):\n'
                                     + '\n'.join(self.lines[-30:]))
            time.sleep(0.2)
        raise AssertionError(f'{self.log_path.name}: no line matching {pattern!r} in {timeout} s:\n' + '\n'.join(self.lines[-30:]))

    def console(self, command):
        self.process.stdin.write(command + '\n')
        self.process.stdin.flush()

    def stop(self, timeout=120):
        if self.process.poll() is None:
            try:
                self.console('stop')
                self.process.wait(timeout=timeout)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                self.process.kill()
                self.process.wait()
        self.reader.join(timeout=10)
        self.log.close()
        self.scratch.cleanup()


def settle(server, ticks=600, timeout=600):
    deadline = time.time() + timeout
    offsets = range(-80, 81, 16)
    for poll in range(1_000_000):
        start = len(server.lines)
        for dx in offsets:
            for dz in offsets:
                server.console(f'execute unless loaded ~{dx} ~ ~{dz} run say unloaded')
        server.console(f'say poll {poll}')
        server.wait_for(rf'\[Server\] poll {poll}$', timeout=max(1, deadline - time.time()))
        if not any(l.endswith('[Server] unloaded') for l in server.lines[start:]):
            break
        time.sleep(2)

    until = game_time(server, deadline - time.time()) + ticks
    while game_time(server, deadline - time.time()) < until:
        time.sleep(2)


def game_time(server, timeout=300):
    deadline = time.time() + timeout
    start = len(server.lines)
    server.console('time query gametime')
    while time.time() < deadline:
        for line in server.lines[start:]:
            if m := re.search(r'The (?:game )?time is (\d+)', line):
                return int(m.group(1))
        time.sleep(0.05)
    raise AssertionError(f'{server.log_path.name}: no game time in {timeout:.0f} s')


def wait_ticks(server, count, timeout=300):
    deadline = time.time() + timeout
    until = game_time(server, timeout) + count
    while game_time(server, max(1.0, deadline - time.time())) < until:
        if time.time() > deadline:
            raise AssertionError(f'{server.log_path.name}: the game clock did not advance {count} ticks in {timeout} s')
        time.sleep(0.2)


def legacy_ping(port):
    host = 'localhost'
    data = (b'\xfe\x01\xfa' + struct.pack('>H', 11) + 'MC|PingHost'.encode('utf-16-be')
            + struct.pack('>H', 7 + 2 * len(host)) + bytes([78]) + struct.pack('>H', len(host))
            + host.encode('utf-16-be') + struct.pack('>I', port))
    with socket.create_connection(('127.0.0.1', port), timeout=10) as s:
        s.sendall(data)
        reply = b''
        while True:
            try:
                chunk = s.recv(4096)
            except TimeoutError:
                return reply.hex() + ' <no close>'
            if not chunk:
                break
            reply += chunk
    return reply.hex()


CONSOLE_LINE = re.compile(r'^\[\d\d:\d\d:\d\d\] \[([^\]]+)\]: (.*)$')
KEPT = re.compile(r'Tester|\[Server\]|\[Not Secure\]|There are|Stopping|Saving|ThreadedAnvilChunkStorage|All dimensions|Unknown or incomplete')


NOT_YET = re.compile(r'Tester was killed|Tester has made the advancement')


def console_lines(lines):
    out = []
    for line in lines:
        m = CONSOLE_LINE.match(line)
        if not m or not KEPT.search(m.group(2)) or NOT_YET.search(m.group(2)):
            continue
        text = re.sub(r'/127\.0\.0\.1:\d+', '/127.0.0.1:_', m.group(2))
        text = re.sub(r'entity id \d+', 'entity id _', text)
        out.append(f'[{m.group(1)}] {text}')
    i = 0
    while i < len(out):
        j = i
        while j < len(out) and 'Saving chunks for level' in out[j]:
            j += 1
        out[i:j] = sorted(out[i:j])
        i = j + 1 if j == i else j
    return out


NOTABLE = re.compile(r'/(WARN|ERROR)\]|Exception|panicked|lost connection')


def notable_lines(path, limit=20):
    try:
        lines = path.read_text(encoding='utf-8', errors='replace').splitlines()
    except OSError:
        return []
    start = next((i for i, l in enumerate(lines) if 'Done (' in l), 0)
    return [f'{path.name}: {l[:300]}' for l in lines[start:] if NOTABLE.search(l)][-limit:]


def server_files(directory):
    out = []
    for name in ['server.properties', 'eula.txt', 'ops.json', 'whitelist.json', 'banned-players.json', 'banned-ips.json',
                 'usercache.json']:
        path = directory / name
        if not path.exists():
            out.append(f'{name}: missing')
            continue
        text = path.read_bytes().decode('iso-8859-1')
        if name in ('server.properties', 'eula.txt'):
            text = re.sub(r'^(#[^\n]*\n)#[^\n]*\n', r'\1#<date>\n', text)
            text = re.sub(r'\n(management-server-secret|server-port)=[^\n]*', r'\n\1=<varies>', text)
        if name == 'usercache.json':
            text = re.sub(r'"expiresOn":"[^"]*"', '"expiresOn":"<a month on>"', text)
        out.append(f'{name}: {text!r}')
    return out


def run_side(label, command, directory, client):
    server = Server(command, directory, label)
    try:
        server.wait_for(r'Done \(')
        server.console('summon minecraft:villager ~1 ~ ~ {NoAI:1b,Xp:1,VillagerData:'
                       '{profession:"minecraft:librarian",level:2,type:"minecraft:plains"}}')
        server.console('tick freeze')
        port = int(PROPERTIES_PORTS[label])
        legacy = legacy_ping(port)
        failure = []

        def join():
            try:
                client(port, directory.parent / f'{label}-client.txt')
            except BaseException as error:
                failure.append(error)

        joined = threading.Thread(target=join)
        joined.start()
        deadline = time.time() + 300
        frozen = True
        while joined.is_alive() and not any('<Tester> ready to respawn' in l for l in server.lines):
            if frozen and any('<Tester> hello from the parity client' in l for l in server.lines):
                server.console('tick unfreeze')
                frozen = False
            if time.time() > deadline:
                break
            time.sleep(0.2)
        if joined.is_alive():
            server.console(f'op {PLAYER}')
        joined.join()
        if failure:
            raise failure[0]
        server.wait_for(rf'{PLAYER} left the game', timeout=60)
        server.console('list')
        server.console('say console says hi')
        server.wait_for(r'\[Server\] console says hi', timeout=30)
    finally:
        server.stop()
    transcript = (directory.parent / f'{label}-client.txt').read_text(encoding='utf-8').splitlines()
    return (['legacy-ping ' + legacy] + transcript + ['console ' + l for l in console_lines(server.lines)]
            + ['file ' + l for l in server_files(directory)])


PROPERTIES_PORTS = {}


def vise_client_joins(vanilla_server, binary, world, build, reference):
    label = 'vanilla-for-vise-client'
    PROPERTIES_PORTS[label] = port = free_port()
    directory = build / label
    write_server_dir(directory, port, world)
    client_dir = build / 'vise-client'
    shutil.rmtree(client_dir, ignore_errors=True)
    client_dir.mkdir()
    report = build / 'vise-client-report.json'
    report.unlink(missing_ok=True)
    server = Server(vanilla_server, directory, label)
    try:
        server.wait_for(r'Done \(')
        env = lab.game_env(VISE_NET_REPORT=report)
        with (build / 'vise-client-run.log').open('w') as log:
            run_game([str(c) for c in [binary, '--resources', lab.RESOURCES, '--render-distance', 2,
                                             '--server', f'127.0.0.1:{port}', '--name', PLAYER]],
                           cwd=client_dir, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=300, check=True)
        server.wait_for(r'left the game', timeout=30)
    finally:
        server.stop()
    got = json.loads(report.read_text())
    move = next(l for l in reference if l.startswith('play-move'))
    want_position = [float(v) for v in re.match(r'play-move \(([^,]+), ([^,]+), ([^)]+)\)', move).groups()]
    want_chunks = int(next(l for l in reference if l.startswith('play-counts')).split()[-1])
    expected = {
        'connected throughout': None,
        'spawn position': want_position,
        'chunks received': want_chunks,
        'own chat shown': True,
        '/list reply shown': True,
        'server log': ['logged in', 'Tester joined the game', '[Not Secure] <Tester> hello from the horizon client',
                       'Tester lost connection: Disconnected', 'Tester left the game'],
    }
    log = '\n'.join(server.lines)
    actual = {
        'connected throughout': got['error'],
        'spawn position': got['position'],
        'chunks received': got['chunks'],
        'own chat shown': any('<Tester> hello from the horizon client' in c for c in got['chat']),
        '/list reply shown': any('There are 1 of a max of 5 players online: Tester' in c for c in got['chat']),
        'server log': [k for k in expected['server log'] if k in log],
    }
    return [f'vise-client {k}: {"ok" if actual[k] == v else f"expected {v!r}, got {actual[k]!r}"}'
            for k, v in expected.items()]


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
    oh.run_logged(['javac', '--release', lock['minecraft']['java_major'], '-proc:none', '-nowarn', '-sourcepath',
                   lab.ROOT / 'mods/vise/src/test/java', '-cp', dependencies, '-d', classes, oracle],
                  build, 'compile-java')
    oh.run_logged(['cargo', 'build', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], build,
                  'compile-rust')
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('vise-client.exe' if os.name == 'nt' else 'vise-client')
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main', 'nogui']
    vise_server = [binary, '--resources', lab.RESOURCES, '--dedicated', '--nogui']

    generate = build / 'generate'
    write_server_dir(generate, free_port())
    server = Server(vanilla_server, generate, 'generate')
    try:
        server.wait_for(r'Done \(')
        server.console('gamerule respawn_radius 0')
        server.wait_for(r'respawn_radius', timeout=30)
        server.console('gamerule random_tick_speed 0')
        server.wait_for(r'random_tick_speed', timeout=30)
        server.console('forceload add ~-80 ~-80 ~80 ~80')
        server.wait_for(r'chunks in the Overworld are now force loaded|forceloaded|force loaded', timeout=60)
        settle(server)
        server.console('forceload remove all')
        server.console('save-all flush')
        server.wait_for(r'Saved the game', timeout=120)
    finally:
        server.stop()
    world = generate / 'world'
    (world / 'playerdata').mkdir(exist_ok=True)

    def client(port, out):
        try:
            oh.run_logged(['java', '-Xmx1g', '-Duser.timezone=UTC', '-cp', str(classes) + os.pathsep + dependencies,
                           'dev.viselab.parity.VanillaProtocolClient', '127.0.0.1', port, PLAYER, out], build,
                          out.stem, timeout=300)
        except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
            log = (build / f'{out.stem}.log').read_text(encoding='utf-8', errors='replace').splitlines()
            raise AssertionError(f'{out.stem}: {error}:\n' + '\n'.join(log[-30:])) from None

    results = {}
    for label, command in [('vanilla', vanilla_server), ('vise', vise_server)]:
        PROPERTIES_PORTS[label] = free_port()
        directory = build / label
        write_server_dir(directory, PROPERTIES_PORTS[label], world)
        results[label] = run_side(label, command, directory, client)
        (build / f'{label}.txt').write_text('\n'.join(results[label]) + '\n', encoding='utf-8')
    client_checks = vise_client_joins(vanilla_server, binary, world, build, results['vanilla'])
    vanilla, vise = results['vanilla'], results['vise']
    brand = 'config minecraft:custom_payload BrandPayload[brand={}]'
    errors_brand = [] if brand.format('vise') in vise else ['error: Vise\'s server did not send the brand "vise"']
    vise = [brand.format('vanilla') if l == brand.format('vise') else l for l in vise]
    errors = [l for l in vanilla + vise if l.startswith('error')] + [l for l in client_checks if not l.endswith(': ok')] + errors_brand
    for line in client_checks:
        print(line)
    differences = []
    import difflib
    for line in difflib.unified_diff(vanilla, vise, 'vanilla', 'vise', n=0, lineterm=''):
        differences.append(line[:400])
    chunk_lines = [dict(re.findall(r'(-?\d+,-?\d+)=(\w+)', next((l for l in side if l.startswith('play-chunks')), '')))
                   for side in (vanilla, vise)]
    for key in sorted(set(chunk_lines[0]) | set(chunk_lines[1])):
        a, b = chunk_lines[0].get(key), chunk_lines[1].get(key)
        if a != b:
            differences.append(f'chunk {key}: vanilla {a} vise {b}')
    if differences or errors:
        notes = [line for name in ('vanilla', 'vanilla-client', 'vise', 'vise-client')
                 for line in notable_lines(build / f'{name}.log')]
        raise AssertionError(f'{len([d for d in differences if d[:1] in "+-" and d[:3] not in ("+++", "---")])} differing lines '
                             f'(see {build}/vanilla.txt and vise.txt):\n' + '\n'.join(differences[:80] + notes))
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        lines=len(vanilla), output_sha256=lab.digest(build / 'vise.txt', 'sha256'),
        scope='Dedicated server seen by a vanilla-codec client: status, legacy ping, login, configuration, play '
              'start (recipes, advancements, command tree), chunks, chat, /list, failing commands\' replies, '
              'console list/say/stop, files written. Offline mode.'))
    print(f'dedicated server: {len(vanilla)} transcript lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
