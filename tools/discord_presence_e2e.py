#!/usr/bin/env python3
import argparse
import json
import os
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

try:
    from . import lab
except ImportError:
    import lab

ROOT = Path(__file__).resolve().parents[1]
HANDSHAKE, FRAME, CLOSE = 0, 1, 2


class FakeDiscord:

    def __init__(self):
        self.dir = Path(tempfile.mkdtemp(prefix='hzdc-', dir='/tmp'))
        self.server = None
        self.activities = []
        self.handshakes = []
        self.started = time.time()
        self.lock = threading.Lock()

    def start(self):
        self.server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.server.bind(str(self.dir / 'discord-ipc-0'))
        self.server.listen()
        threading.Thread(target=self.accept, daemon=True).start()

    def accept(self):
        while True:
            try:
                conn, _ = self.server.accept()
            except OSError:
                return
            threading.Thread(target=self.serve, args=(conn,), daemon=True).start()

    def serve(self, conn):
        def read():
            header = conn.recv(8, socket.MSG_WAITALL)
            if len(header) < 8:
                raise EOFError
            op, length = struct.unpack('<II', header)
            return op, json.loads(conn.recv(length, socket.MSG_WAITALL) or b'null')

        def send(op, payload):
            body = json.dumps(payload).encode()
            conn.sendall(struct.pack('<II', op, len(body)) + body)

        try:
            while True:
                op, msg = read()
                if op == HANDSHAKE:
                    with self.lock:
                        self.handshakes.append(msg)
                    send(FRAME, {'cmd': 'DISPATCH', 'evt': 'READY', 'data': {'v': 1}})
                elif op == FRAME and msg.get('cmd') == 'SET_ACTIVITY':
                    with self.lock:
                        self.activities.append((round(time.time() - self.started, 1), msg['args']['activity']))
                    send(FRAME, {'cmd': 'SET_ACTIVITY', 'nonce': msg['nonce'], 'data': msg['args']['activity']})
                elif op == CLOSE:
                    return
        except (EOFError, OSError, ValueError):
            pass
        finally:
            conn.close()

    def last(self):
        with self.lock:
            return self.activities[-1][1] if self.activities else 'nothing yet'

    def wait(self, what, check, timeout=60):
        deadline = time.time() + timeout
        while time.time() < deadline:
            if check(self.last()):
                print(f'  ok: {what}: {json.dumps(self.last())}')
                return
            time.sleep(0.25)
        raise AssertionError(f'{what}: the last activity was {self.last()!r}; all: {self.activities}')

    def close(self):
        if self.server:
            self.server.close()
        shutil.rmtree(self.dir, ignore_errors=True)


def shows(details, state=None):
    return lambda a: isinstance(a, dict) and a.get('details') == details and (state is None or a.get('state') == state)


class Game:

    def __init__(self, bins, name, args=(), extras=None, discord_running=True):
        self.ctl = bins / 'vise-ctl'
        self.home = Path(tempfile.mkdtemp(prefix=f'hzdp-{name}-'))
        if extras:
            (self.home / 'vise-extras.txt').write_text(extras)
        self.socket = self.home / 'c.sock'
        self.discord = FakeDiscord()
        if discord_running:
            self.discord.start()
        extra = {'VISE_HOME': self.home, 'VISE_TELEMETRY_URL': 'off', 'VISE_DISCORD_IPC_DIR': self.discord.dir}
        self.env = lab.game_env(**extra)
        self.ctl_run('launch', '--offscreen', '--game', str(bins / 'vise-client'), '--resources',
                     str(lab.RESOURCES.resolve()), *args)

    def ctl_run(self, *words, timeout=600):
        r = subprocess.run([str(self.ctl), '--socket', str(self.socket), *words], cwd=self.home, env=self.env,
                           capture_output=True, text=True, timeout=timeout)
        out = r.stdout.strip().splitlines()
        reply = json.loads(out[-1]) if out else {'error': r.stderr.strip()}
        if r.returncode or 'error' in reply:
            raise AssertionError(f'vise-ctl {" ".join(words)}: exit {r.returncode}: {reply}')
        return reply

    def quit(self):
        try:
            self.ctl_run('quit', timeout=120)
        except Exception as e:
            print(f'  quit: {e}')

    def close(self):
        self.discord.close()
        shutil.rmtree(self.home, ignore_errors=True)


def single(bins, args=()):
    game = Game(bins, 'single', args=args)
    d = game.discord
    try:
        d.wait('title screen', shows('In the menus'))
        assert d.handshakes[0]['v'] == 1 and d.handshakes[0]['client_id'], d.handshakes
        game.ctl_run('open', 'Presence', '5', 'creative')
        game.ctl_run('wait-for', 'loaded', '--timeout', '300')
        d.wait('in the world', shows('Singleplayer', 'In the Overworld'))
        start = d.last()['timestamps']['start']
        assert abs(start - time.time()) < 120 and 'party' not in d.last(), d.last()
        game.ctl_run('command', 'execute in minecraft:the_nether run tp @s 0 80 0')
        d.wait('in the Nether', shows('Singleplayer', 'In the Nether'))
        assert d.last()['timestamps']['start'] == start, 'a dimension change restarted the clock'
        text = json.dumps(d.activities)
        assert 'Presence' not in text, f'the world name reached Discord: {text}'
        game.quit()
        d.wait('cleared on quit', lambda a: a is None)
    finally:
        game.close()


def lan(bins, host_args=()):
    host = Game(bins, 'host', args=host_args)
    guest = None
    try:
        host.ctl_run('open', 'Hosted', '5', 'creative')
        host.ctl_run('wait-for', 'loaded', '--timeout', '300')
        port = lab.free_port()
        host.ctl_run('lan', 'open', str(port))
        host.discord.wait('host, alone', lambda a: shows('Hosting a world')(a) and a['party']['size'] == [1, 8])
        guest = Game(bins, 'guest')
        guest.discord.wait('guest title screen', shows('In the menus'))
        guest.ctl_run('connect', f'127.0.0.1:{port}', 'Guest')
        guest.ctl_run('wait-for', 'loaded', '--timeout', '300')
        guest.discord.wait('guest in the world', shows('Multiplayer', 'In the Overworld'))
        assert '127.0.0.1' not in json.dumps(guest.discord.activities), guest.discord.activities
        host.discord.wait('host with a guest', lambda a: shows('Hosting a world')(a) and a['party']['size'] == [2, 8])
        guest.quit()
        guest.discord.wait('guest cleared on quit', lambda a: a is None)
        host.quit()
        host.discord.wait('host cleared on quit', lambda a: a is None)
    finally:
        for g in (guest, host):
            if g:
                g.close()


def setting(bins):
    game = Game(bins, 'setting', extras='discordPresence:false\n')
    d = game.discord
    try:
        game.ctl_run('wait-for', 'screen', 'title')
        time.sleep(3)
        assert not d.handshakes and not d.activities, (d.handshakes, d.activities)
        game.ctl_run('options', 'vise')
        switch = next(w for w in game.ctl_run('ui')['widgets'] if w.get('label', '').startswith('Discord Rich'))
        assert switch['label'] == 'Discord Rich Presence: Off', switch
        game.ctl_run('click', switch['id'])
        d.wait('turned on', shows('In the menus'))
        game.ctl_run('click', switch['id'])
        d.wait('turned off', lambda a: a is None)
        assert 'discordPresence:false' in (game.home / 'vise-extras.txt').read_text()
        print('  ok: saved off')
        game.quit()
    finally:
        game.close()


def late(bins):
    game = Game(bins, 'late', discord_running=False)
    try:
        game.ctl_run('wait-for', 'screen', 'title')
        time.sleep(2)
        game.discord.start()
        started = time.time()
        game.discord.wait('found Discord', shows('In the menus'), timeout=30)
        print(f'  ok: after {time.time() - started:.1f} s')
        game.quit()
    finally:
        game.close()


CHECKS = {'single': single, 'in-frame': lambda b: single(b, ['--in-frame']), 'lan': lan,
          'lan-in-frame': lambda b: lan(b, ['--in-frame']), 'setting': setting,
          'late': late}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--check', choices=[*CHECKS, 'all'], default='all')
    parser.add_argument('--bin-dir', help='prebuilt vise-client and vise-ctl (default: build with the fast profile)')
    args = parser.parse_args()
    if sys.platform == 'win32':
        sys.exit('the fake Discord is a Unix socket: macOS or Linux only')
    if args.bin_dir:
        bins = Path(args.bin_dir).resolve()
    else:
        subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'vise-client', '--bins'],
                       cwd=ROOT, check=True)
        meta = json.loads(subprocess.run(['cargo', 'metadata', '--format-version', '1', '--no-deps'], cwd=ROOT,
                                         capture_output=True, text=True, check=True).stdout)
        bins = Path(meta['target_directory']) / 'fast'
    if not (lab.RESOURCES / 'vise-resources.json').exists():
        sys.exit(f'no prepared resources in {lab.RESOURCES}')
    results = {}
    for name in CHECKS if args.check == 'all' else [args.check]:
        print(f'[{name}]')
        started = time.time()
        try:
            CHECKS[name](bins)
            results[name] = True
            print(f'[{name}] PASS in {time.time() - started:.0f} s')
        except (AssertionError, KeyError, StopIteration, subprocess.TimeoutExpired) as e:
            results[name] = False
            print(f'[{name}] FAIL: {e}')
    print(json.dumps(results))
    sys.exit(0 if all(results.values()) else 1)


if __name__ == '__main__':
    main()
