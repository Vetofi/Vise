#!/usr/bin/env python3
import json
import os
import platform
import struct
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT / 'tools'))

import lab
import oracle_harness
import test_model_baking_parity as baking
import test_rust_parity as base


def bits(value):
    return struct.pack('>d', value).hex()


def vec(values):
    return [bits(v) for v in values]


def row(position, velocity, ops, seed):
    return {'seed': str(seed), 'position': vec(position), 'velocity': vec(velocity),
            'crit': False, 'no_physics': False, 'intangible': False, 'ops': ops}


def fixtures():
    wall = {'mode': 'block', 'name': 'minecraft:stone', 'position': [12, 20, 10]}

    def ticks(n):
        return [{'mode': 'tick'} for _ in range(n)]

    cases = []
    for i, z in enumerate((10.55, 10.85, 11.05, 11.15, 11.30)):
        cases.append((f'z={z:5.2f}', row((10.5, 20.52, z), (2.5, 0.0, 0.0),
                                         [{'mode': 'level_seed', 'value': str(1000 + i)}, wall] + ticks(1), 1000 + i)))
    for i, z in enumerate((10.35, 10.55)):
        cases.append((f'corner z={z:5.2f}', row((10.5, 20.52, z), (2.5, 0.0, 1.0),
                                                [{'mode': 'level_seed', 'value': str(2000 + i)}, wall] + ticks(1), 2000 + i)))
    cases.append(('miss, 3 ticks', row((10.5, 20.52, 11.05), (2.5, 0.0, 0.0),
                                       [{'mode': 'level_seed', 'value': '3000'}, wall] + ticks(3), 3000)))
    return cases


def oracle_lines(build):
    cases, current = [], []
    for line in (build / 'client.txt').read_text().splitlines():
        if line == 'end':
            cases.append(current)
            current = []
        elif line.startswith('arrow '):
            current.append(line.split())
    return cases


def probe_lines(build):
    out = []
    for line in (build / 'rust-client.txt').read_text().splitlines():
        fields = line.split()
        if len(fields) != 5 or fields[0] != 'trident':
            raise AssertionError(f'probe line: {line!r}')
        out.append(fields)
    return out


def main():
    lock, jars, _ = base.verify_artifacts()
    libraries = baking.client_libraries()
    build = lab.LOCAL / 'trident-parity'
    build.mkdir(parents=True, exist_ok=True)
    resources = lab.LOCAL / 'vise-client' / lock['minecraft']['version']
    oracle = ROOT / 'mods/vise/src/test/java/dev/viselab/parity/VanillaArrowOracle.java'
    sources = [Path(__file__), ROOT / 'tools/oracle_harness.py', ROOT / 'tools/lab.py',
               ROOT / 'tools/test_rust_parity.py', ROOT / 'tools/test_model_baking_parity.py', oracle,
               ROOT / 'Cargo.toml', ROOT / 'Cargo.lock',
               ROOT / 'crates/vise-client/examples/probe/trident_probe.rs',
               ROOT / 'crates/vise-client/examples/probe/main.rs',
               ROOT / 'crates/vise-client/src/arrows.rs',
               ROOT / 'crates/vise-client/src/game/trident.rs']
    tested = base.source_hashes(sources)
    cases = fixtures()
    (build / 'inputs.txt').write_text(''.join(json.dumps(row, separators=(',', ':')) + '\n'
                                              for _, row in cases))
    input_hash = lab.digest(build / 'inputs.txt', 'sha256')
    classes, dependencies, _ = oracle_harness.compile_oracle(
        build, 'VanillaArrowOracle', jars, libraries, lock['minecraft']['java_major'])
    probe = oracle_harness.build_probe(build, 'trident_probe')
    oracle_harness.run_logged([*probe, resources, build, '--describe'], build, 'describe')
    oracle_harness.run_logged([*probe, resources, build, 'client'], build, 'rust')
    oracle_harness.java_run(build, classes, dependencies, 'VanillaArrowOracle',
                            ['client', build, resources], 'client')
    vanilla, rust = oracle_lines(build), probe_lines(build)
    if not (len(vanilla) == len(rust) == len(cases)):
        raise AssertionError(f'case count: {len(cases)} fixtures, {len(vanilla)} vanilla, {len(rust)} rust')
    failures = []
    for (name, _), ref, got in zip(cases, vanilla, rust):
        state = ref[-1]
        expected = (state[1], state[2], state[5], state[8])
        actual = (got[1], got[2], got[3], got[4])
        if expected != actual:
            failures.append((name, expected, actual))
    if failures:
        report = '\n'.join(f'  {name}: vanilla {exp} != rust {act}' for name, exp, act in failures)
        raise AssertionError(f'{len(failures)} of {len(cases)} trident block steps differ:\n{report}')
    base.ensure_sources_unchanged(sources, tested)
    if input_hash != lab.digest(build / 'inputs.txt', 'sha256'):
        raise AssertionError('Trident fixtures changed during verification')
    report = {'schema': 1, 'minecraft': lock['minecraft']['version'],
              'manifest_sha1': lock['minecraft']['manifest_sha1'], 'platform': platform.platform(),
              'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'java': subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
              'fixtures': len(cases), 'matched_output_lines': len(cases),
              'sources_sha256': tested, 'input_sha256': input_hash,
              'client_jar_sha256': lab.digest(jars['client'], 'sha256'), 'scope': __doc__}
    lab.write_json(build / 'report.json', report)
    print(f'{len(cases)} trident block steps match the pinned client: {build / "report.json"}', flush=True)


if __name__ == '__main__':
    main()
