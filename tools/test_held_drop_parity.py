#!/usr/bin/env python3
import os
import subprocess
try:
    from . import lab, test_rust_parity as base
except ImportError:
    import lab
    import test_rust_parity as base


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / 'held-drop-parity'
    build.mkdir(parents=True, exist_ok=True)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    _, game_dir = lab.config()
    libraries += sorted((game_dir / 'libraries/client').rglob('*.jar'))
    classpath = os.pathsep.join(map(str, [jars['client'], *libraries]))
    oracle = lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/VanillaHeldDropOracle.java'
    subprocess.run(['javac', '--release', str(lock['minecraft']['java_major']), '-proc:none',
                    '-cp', classpath, '-d', str(classes), str(oracle)], check=True)
    subprocess.run(['java', '-Xmx512M', '-cp', str(classes) + os.pathsep + classpath,
                    'dev.viselab.parity.VanillaHeldDropOracle', str(build / 'vanilla.txt')], check=True)
    with (build / 'rust.txt').open('w') as out:
        subprocess.run(['cargo', 'run', '--quiet', '-p', 'vise-client', '--example',
                        'client_probe', '--', 'held_drop_fixture'], cwd=lab.ROOT, stdout=out, check=True)
    matched = base.compare(build / 'vanilla.txt', build / 'rust.txt')
    print(f'Held drop parity passed: {matched} inventory/drop/swing/packet snapshots match vanilla')


if __name__ == '__main__':
    main()
