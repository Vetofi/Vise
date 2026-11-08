#!/usr/bin/env python3
import argparse
import os
import re
import subprocess
import sys
import threading
import time

try:
    from . import lab
except ImportError:
    import lab

SUSPICIOUS = re.compile(
    r'(ERROR|login failed|Exception|Couldn\'t load|Failed to|Skipping|Invalid|Unknown|corrupt|'
    r'Recreating|upgrad|DataFix|out of bounds|missing|Mismatch)', re.IGNORECASE)
BENIGN = re.compile(r'(Unpacking |OFFLINE/INSECURE|online-mode|authenticate usernames|hackers|'
                    r'Failed to load properties|Failed to load eula)')


def main():
    p = argparse.ArgumentParser()
    p.add_argument('--jar', required=True)
    p.add_argument('--dir', required=True)
    p.add_argument('--world', default='world')
    p.add_argument('--seed', default='12345')
    p.add_argument('--port', type=int, default=0, help='default: a free port')
    p.add_argument('--command', action='append', default=[])
    p.add_argument('--wait-after', type=float, default=1.0)
    p.add_argument('--timeout', type=float, default=240)
    args = p.parse_args()
    port = args.port or lab.free_port()
    os.makedirs(args.dir, exist_ok=True)
    with open(os.path.join(args.dir, 'eula.txt'), 'w') as f:
        f.write('eula=true\n')
    props = {
        'server-port': port, 'online-mode': 'false', 'level-seed': args.seed,
        'level-name': args.world, 'spawn-protection': '0', 'view-distance': '4',
        'simulation-distance': '4', 'sync-chunk-writes': 'true', 'enable-query': 'false',
        'enable-rcon': 'false', 'management-server-enabled': 'false',
        'pause-when-empty-seconds': '-1', 'white-list': 'false', 'enforce-whitelist': 'false',
    }
    with open(os.path.join(args.dir, 'server.properties'), 'w') as f:
        f.writelines(f'{k}={v}\n' for k, v in props.items())
    proc = subprocess.Popen(
        ['java', '-Xmx2G', '-jar', os.path.abspath(args.jar), '--nogui'], cwd=args.dir,
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = []
    done = threading.Event()

    def pump():
        for line in proc.stdout:
            lines.append(line.rstrip('\n'))
            if 'Done (' in line:
                done.set()
        done.set()

    threading.Thread(target=pump, daemon=True).start()
    if not done.wait(args.timeout) or proc.poll() is not None:
        proc.kill()
        print('\n'.join(lines))
        print('server did not start', file=sys.stderr)
        return 1
    for command in args.command:
        if command.startswith('sleep '):
            time.sleep(float(command.split()[1]))
            continue
        if command.startswith('login '):
            sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
            import vanilla_login
            _, name, seconds = command.split()

            def run(name=name, seconds=float(seconds)):
                try:
                    vanilla_login.join('127.0.0.1', port, name, seconds)
                except Exception as e:
                    lines.append(f'login failed: {e}')

            threading.Thread(target=run, daemon=True).start()
            continue
        proc.stdin.write(command + '\n')
        proc.stdin.flush()
        time.sleep(0.5)
    time.sleep(args.wait_after)
    proc.stdin.write('stop\n')
    proc.stdin.flush()
    try:
        proc.wait(args.timeout)
    except subprocess.TimeoutExpired:
        proc.kill()
    time.sleep(0.2)
    problems = [l for l in lines if SUSPICIOUS.search(l) and not BENIGN.search(l)]
    with open(os.path.join(args.dir, 'check.log'), 'w') as f:
        f.write('\n'.join(lines) + '\n')
    for l in problems:
        print(l)
    print(f'{len(lines)} log lines, {len(problems)} suspicious')
    return 1 if problems else 0


if __name__ == '__main__':
    sys.exit(main())
