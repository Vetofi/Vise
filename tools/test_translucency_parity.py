#!/usr/bin/env python3
import argparse
import json
import os
import random
import subprocess

try:
    from . import lab, test_rust_parity as base, test_model_baking_parity as baking
except ImportError:
    import lab, test_rust_parity as base, test_model_baking_parity as baking


def cases():
    rng = random.Random(563)
    yield {'origin': [0, 0, 0], 'quads': [
        [[-1, 0, 0]] * 4, [[1, 0, 0]] * 4,
        [[0, 0, 0], [100, 100, 100], [2, 2, 2], [200, 200, 200]]],
        'cameras': [[0, 0, 0], [0.1, 0.1, 0.1], [1, 0, 0], [-1, 0, 0]]}
    for count in [0, 1, 2, 64, *[rng.randrange(3, 100) for _ in range(200)]]:
        origin = [rng.choice([-30000000, -64, -16, 0, 16, 30000000]) for _ in range(3)]
        origin[1] = rng.choice([-64, -16, 0, 16, 304])
        quads = [[[rng.uniform(-1, 17) for _ in range(3)] for _ in range(4)] for _ in range(count)]
        cameras = [[o + d for o in origin] for d in [-32.5, -32.1, -31.9, -16, -0.1, 0, 0.9, 1.1, 15.9, 16, 32, 48]]
        cameras += [[origin[i] + rng.uniform(-64, 64) for i in range(3)] for _ in range(8)]
        yield {'origin': origin, 'quads': quads, 'cameras': cameras}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile', default='fast', help='Cargo build profile')
    args = parser.parse_args()
    lock, jars, _ = base.verify_artifacts()
    build = lab.LOCAL / 'translucency-parity'
    classes = build / 'classes'
    classes.mkdir(parents=True, exist_ok=True)
    inputs = build / 'inputs.jsonl'
    inputs.write_text(''.join(json.dumps(c) + '\n' for c in cases()), encoding='utf-8')
    dependencies = os.pathsep.join(map(str, [jars['client'], *baking.client_libraries()]))

    def run(command, name):
        with (build / (name + '.log')).open('w', encoding='utf-8') as log:
            result = subprocess.run(list(map(str, command)), cwd=lab.ROOT, stdout=log, stderr=subprocess.STDOUT, timeout=600)
        if result.returncode:
            raise RuntimeError((build / (name + '.log')).read_text()[-4000:])

    run(['javac', '--release', lock['minecraft']['java_major'], '-proc:none', '-cp', dependencies, '-d', classes,
         lab.ROOT / 'mods/vise/src/test/java/dev/viselab/parity/VanillaTranslucencyOracle.java'], 'compile-java')
    vanilla, rust = build / 'vanilla.jsonl', build / 'rust.jsonl'
    run(['java', '-cp', str(classes) + os.pathsep + dependencies, 'dev.viselab.parity.VanillaTranslucencyOracle', inputs, vanilla], 'vanilla')
    run(['cargo', 'run', '--quiet', '--locked', '--profile', args.profile, '-p', 'vise-client', '--example', 'client_probe', '--', 'translucency_fixture', inputs, rust], 'rust')
    expected = [json.loads(line) for line in vanilla.read_text().splitlines()]
    actual = [json.loads(line) for line in rust.read_text().splitlines()]
    assert len(expected) == len(actual), 'case count differs'
    for i, (a, b) in enumerate(zip(expected, actual)):
        assert a == b, f'sorting mismatch in case {i}; inspect {build}'
    frames = sum(len(c['frames']) for c in expected)
    print(f'translucency parity ok: {len(expected)} cases, {frames} camera views; centroids, indices and cache decisions match exactly')


if __name__ == '__main__':
    main()
