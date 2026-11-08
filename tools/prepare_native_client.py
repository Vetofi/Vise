#!/usr/bin/env python3
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    arguments = ['--root', str(ROOT), *sys.argv[1:]]
    beside = ROOT / ('vise-prepare.exe' if os.name == 'nt' else 'vise-prepare')
    if beside.is_file():
        command = [str(beside), *arguments]
    else:
        command = ['cargo', 'run', '--quiet', '--release', '-p', 'vise-prepare', '--', *arguments]
    return subprocess.call(command, cwd=ROOT)


if __name__ == '__main__':
    sys.exit(main())
