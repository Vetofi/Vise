#!/usr/bin/env python3
import json
import subprocess

import lab
import lan_e2e as lan


def main():
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'vise-client',
                    '--bin', 'vise-client', '--bin', 'vise-ctl'], cwd=lab.ROOT, check=True)
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps'], cwd=lab.ROOT, text=True))
    ctl = lab.ROOT / metadata['target_directory'] / 'fast' / 'vise-ctl'
    lan.game_env = lambda _screen, home: lab.game_env(
        VISE_HOME=home, VISE_TELEMETRY_URL='off')
    reports = {}
    for threaded in (True, False):
        lan.SERVER_THREAD = threaded
        mode = 'server-thread' if threaded else 'in-frame'
        out = lab.LOCAL / 'remote-fireworks-parity' / mode
        out.mkdir(parents=True, exist_ok=True)
        reports[mode] = lan.run(ctl, out, lab.RESOURCES.resolve(), ['fireworks'])
    lab.write_json(lab.LOCAL / 'remote-fireworks-parity' / 'report.json', reports)
    assert all(r['pass'] for mode in reports.values() for r in mode.values()), reports


if __name__ == '__main__':
    main()
