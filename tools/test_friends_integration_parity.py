#!/usr/bin/env python3
import json
from pathlib import Path
import subprocess
import sys

import lab
import test_friends
from integration_env import suite_lock


if __name__ == '__main__':
    subprocess.run(['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'vise-client',
                    '-p', 'vise-relay', '--bin', 'vise-client', '--bin', 'vise-ctl',
                    '--bin', 'vise-relay'], cwd=lab.ROOT, check=True)
    metadata = json.loads(subprocess.check_output(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps'], cwd=lab.ROOT, text=True))
    bins = Path(metadata['target_directory']) / 'fast'
    sys.argv = [sys.argv[0], '--client', str(bins / 'vise-client'),
                '--relay', str(bins / 'vise-relay'), '--baseline',
                *(['--offscreen'] if sys.platform == 'darwin' else [])]
    with suite_lock():
        test_friends.main()
