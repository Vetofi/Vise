#!/usr/bin/env python3

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
VERSIONS = 'crates/vise-prepare/output-versions.txt'
LIB = 'crates/vise-prepare/src/lib.rs'
MANIFEST = 'vise-resources.json'
LAMBDA = re.compile(r'\$\$Lambda/0x[0-9a-fA-F]+')
CONSTANT = re.compile(r'^pub const OUTPUT_VERSION: u32 = (\d+);', re.M)


def canonical_json(value, key=''):
    if isinstance(value, dict):
        return {k: canonical_json(v, k) for k, v in value.items()}
    if isinstance(value, list):
        items = [canonical_json(v, key) for v in value]
        if key == 'tags' and all(isinstance(x, list) and all(isinstance(t, str) for t in x) for x in items):
            items = [sorted(x) for x in items]
        return items
    if isinstance(value, str):
        return LAMBDA.sub('$$Lambda', value)
    return value


def canonical(rel: str, data: bytes) -> bytes:
    if rel == MANIFEST:
        manifest = json.loads(data)
        scalars = {k: v for k, v in manifest.items() if not isinstance(v, (dict, list))}
        return json.dumps(scalars, sort_keys=True).encode()
    if rel.endswith('.json'):
        try:
            value = json.loads(data)
        except ValueError:
            return data
        return json.dumps(canonical_json(value), sort_keys=True).encode()
    return data


def skipped(rel: str) -> bool:
    parts = rel.split('/')
    return rel.endswith(('.log', '.class')) or parts[:3] == ['assets', 'minecraft', 'sounds'] \
        or rel == 'assets/minecraft/sounds.json'


def digest(resources: Path) -> tuple[str, int]:
    if not (resources / MANIFEST).is_file():
        raise SystemExit(f'{resources} is not a preparation: no {MANIFEST}')
    files = []
    for dirpath, dirs, names in os.walk(resources):
        dirs.sort()
        for name in names:
            path = Path(dirpath) / name
            rel = path.relative_to(resources).as_posix()
            if path.is_symlink() or skipped(rel):
                continue
            files.append(rel)
    total = hashlib.sha256()
    for rel in sorted(files):
        data = hashlib.sha256(canonical(rel, (resources / rel).read_bytes())).hexdigest()
        total.update(f'{rel}\0{data}\n'.encode())
    return total.hexdigest(), len(files)


def parse_versions(text: str) -> list[tuple[int, str]]:
    out = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        number, value = line.split()
        out.append((int(number), value))
    return out


def problems(versions: list[tuple[int, str]], constant: int | None) -> list[str]:
    found = []
    if not versions:
        return [f'{VERSIONS} lists no version']
    if [n for n, _ in versions] != list(range(1, len(versions) + 1)):
        found.append(f'{VERSIONS}: the versions must be 1, 2, 3... in order')
    if len({d for _, d in versions}) != len(versions):
        found.append(f'{VERSIONS}: two versions with the same digest')
    if constant != versions[-1][0]:
        found.append(f'{LIB} says OUTPUT_VERSION = {constant}, {VERSIONS} ends at {versions[-1][0]}')
    return found


def default_resources() -> Path:
    lock = json.loads((ROOT / 'research-lock.json').read_text())
    return ROOT / '.local/vise-client' / lock['minecraft']['version']


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('command', choices=['digest', 'check'])
    parser.add_argument('resources', nargs='?', type=Path)
    parser.add_argument('--base', help='the commit the change is based on (check: the file only grew since)')
    parser.add_argument('--cache', action='store_true', help='keep the digest beside the preparation')
    args = parser.parse_args()
    resources = (args.resources or default_resources()).resolve()
    tool = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()[:16]
    cache = resources.parent / f'.output-digest-{resources.name}-{tool}'
    if args.cache and cache.is_file():
        value, count = cache.read_text().split()
    else:
        value, count = digest(resources)
        if args.cache:
            cache.write_text(f'{value} {count}\n')
    if args.command == 'digest':
        print(value)
        return 0

    versions = parse_versions((ROOT / VERSIONS).read_text())
    match = CONSTANT.search((ROOT / LIB).read_text())
    found = problems(versions, int(match.group(1)) if match else None)
    if args.base:
        shown = subprocess.run(['git', 'show', f'{args.base}:{VERSIONS}'], cwd=ROOT, capture_output=True, text=True)
        if shown.returncode == 0:
            before = parse_versions(shown.stdout)
            if versions[:len(before)] != before:
                found.append(f'{VERSIONS} changed lines it already had (since {args.base[:12]}): '
                             'a new output gets a new version, appended')
    current = versions[-1] if versions else (0, '')
    if value != current[1]:
        known = [n for n, d in versions if d == value]
        if known:
            found.append(f'the preparation is OUTPUT_VERSION {known[0]}\'s output, but OUTPUT_VERSION is {current[0]}')
        else:
            found.append(
                f'the prepared output changed ({count} files, digest {value}) but OUTPUT_VERSION is still '
                f'{current[0]}. Players would keep resources prepared by the old preparer. Bump it:\n'
                f'  set `pub const OUTPUT_VERSION: u32 = {current[0] + 1};` in {LIB}\n'
                f'  append `{current[0] + 1} {value}` to {VERSIONS}\n'
                '  (if the change is only a nondeterminism this tool should ignore, teach canonical() instead)')
    for problem in found:
        print(f'::error::{problem}' if os.environ.get('GITHUB_ACTIONS') else f'error: {problem}', file=sys.stderr)
    if not found:
        print(f'prepared output: OUTPUT_VERSION {current[0]} ({count} files, {value[:16]})')
    return 1 if found else 0


if __name__ == '__main__':
    sys.exit(main())
