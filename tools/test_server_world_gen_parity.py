#!/usr/bin/env python3
import json
import os
from pathlib import Path
import platform
import re
import shutil
import subprocess
import sys
import time

try:
    from . import anvil_dump, lab, test_rust_parity as base
    from .test_dedicated_server_parity import Server, free_port
except ImportError:
    import anvil_dump
    import lab
    import test_rust_parity as base
    from test_dedicated_server_parity import Server, free_port

NAME = 'server-world-gen-parity'
PROPERTIES = {
    'online-mode': 'false',
    'view-distance': '2',
    'simulation-distance': '2',
    'spawn-protection': '0',
}
CASES = {
    'flat': {'level-type': 'flat', 'level-seed': '1'},
    'flat-layers': {
        'level-type': 'minecraft:flat',
        'level-seed': '-7',
        'generator-settings': json.dumps({
            'layers': [{'block': 'minecraft:bedrock', 'height': 1}, {'block': 'stone', 'height': 10},
                       {'block': 'minecraft:no_such_block', 'height': 2}, {'block': 'sand', 'height': 3}],
            'biome': 'minecraft:desert', 'lakes': True, 'features': True}, separators=(',', ':')),
    },
    'flat-biome': {
        'level-type': 'FLAT',
        'level-seed': 'flat',
        'generator-settings': '{"layers":[{"block":"minecraft:glass","height":5}],"biome":"nowhere"}',
    },
    'normal': {'level-type': 'minecraft:normal', 'level-seed': 'abc', 'generate-structures': 'false'},
    'amplified': {'level-type': 'minecraft:amplified', 'level-seed': '1'},
    'large-biomes': {'level-type': 'minecraft:large_biomes', 'level-seed': '2'},
    'single-biome': {'level-type': 'minecraft:single_biome_surface', 'level-seed': '3'},
    'unknown': {'level-type': 'Some Type', 'level-seed': '42'},
}
WORKER = re.compile(r'\[Worker-Main-\d+/(WARN|ERROR)\]: (.*)')
LOCATE = ['locate structure #minecraft:village', 'locate structure minecraft:mineshaft']
REPLY = re.compile(r'\]: (?:System chat: )?((?:Could not find|The nearest|Unknown|Incorrect).*)')


def console_reply(server, command, timeout=300):
    start = len(server.lines)
    server.console(command)
    deadline = time.time() + timeout
    while time.time() < deadline:
        for line in server.lines[start:]:
            if m := REPLY.search(line):
                return m.group(1)
        time.sleep(0.2)
    raise AssertionError(f'no reply to {command!r} in {timeout} s:\n' + '\n'.join(server.lines[-20:]))


def structure_chunks(world):
    starts = references = 0
    for region in sorted((world / 'region').glob('*.mca')):
        for chunk in anvil_dump.region_chunks(region).values():
            structures = anvil_dump.plain(chunk, True).get('structures', {})
            starts += bool(structures.get('starts'))
            references += bool(structures.get('References'))
    return f'chunks with structure starts: {starts}, with references: {references}'


def escape(value):
    return value.replace('\\', '\\\\').replace(':', '\\:').replace('=', '\\=').replace('#', '\\#')


def run_case(label, command, build, case):
    directory = build / label / case
    if directory.exists():
        shutil.rmtree(directory)
    directory.mkdir(parents=True)
    props = dict(PROPERTIES, **CASES[case], **{'server-port': str(free_port())})
    (directory / 'server.properties').write_text(''.join(f'{k}={escape(v)}\n' for k, v in props.items()),
                                                 encoding='iso-8859-1')
    (directory / 'eula.txt').write_text('eula=true\n')
    server = Server(command, directory, f'{label}-{case}')
    replies = []
    try:
        server.wait_for(r'Done \(')
        if CASES[case].get('generate-structures') == 'false':
            replies = [f'{c}: {console_reply(server, c)}' for c in LOCATE]
    finally:
        server.stop()
    out = [f'# {case}', *replies]
    if replies:
        out.append(structure_chunks(directory / 'world'))
    out += [f'{m.group(1)}: {m.group(2)}' for l in server.lines if (m := WORKER.search(l))]
    settings = anvil_dump.plain(anvil_dump.read_dat(directory / 'world/data/minecraft/world_gen_settings.dat'),
                                True, typed=True)
    out += json.dumps(settings, indent=1, sort_keys=True).splitlines()
    return out


def main():
    lock, jars, libraries = base.verify_artifacts()
    build = lab.LOCAL / NAME
    build.mkdir(parents=True, exist_ok=True)
    report = build / 'report.json'
    report.unlink(missing_ok=True)
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-p', 'vise-client', '--bin', 'vise-client'], cwd=lab.ROOT,
                   check=True, capture_output=True)
    metadata = json.loads(subprocess.check_output(['cargo', 'metadata', '--no-deps', '--format-version', '1'],
                                                  cwd=lab.ROOT, text=True))
    binary = Path(metadata['target_directory']) / 'fast' / ('vise-client.exe' if os.name == 'nt' else 'vise-client')
    dependencies = os.pathsep.join(map(str, [jars['server'], *libraries]))
    vanilla_server = ['java', '-Xmx2g', '-Duser.timezone=UTC', '-cp', dependencies, 'net.minecraft.server.Main',
                      'nogui']
    vise_server = [binary, '--resources', lab.LOCAL / 'vise-client/26.3', '--dedicated', '--nogui']
    vanilla, vise = [], []
    for case in CASES:
        vanilla += run_case('vanilla', vanilla_server, build, case)
        vise += run_case('vise', vise_server, build, case)
    (build / 'vanilla.txt').write_text('\n'.join(vanilla) + '\n', encoding='utf-8')
    (build / 'vise.txt').write_text('\n'.join(vise) + '\n', encoding='utf-8')
    if vanilla != vise:
        import difflib
        diff = '\n'.join(difflib.unified_diff(vanilla, vise, 'vanilla', 'vise', n=2, lineterm=''))
        raise AssertionError(f'world settings differ (see {build}/vanilla.txt and vise.txt):\n' + diff[:6000])
    if not any('minecraft:flat' in l for l in vanilla) or not any(l.startswith('ERROR: ') for l in vanilla):
        raise AssertionError(f'vanilla made no flat world or logged no error (see {build})')
    lab.write_json(report, dict(
        schema=1, minecraft=lock['minecraft']['version'], platform=platform.platform(),
        java=subprocess.check_output(['java', '--version'], text=True).splitlines()[0],
        cases=list(CASES), lines=len(vanilla), output_sha256=lab.digest(build / 'vise.txt', 'sha256'),
        scope='a new dedicated-server world: level-type (flat, normal, amplified, large_biomes, '
              'single_biome_surface, unknown), a flat world\'s generator-settings, generate-structures, as '
              'world_gen_settings.dat and the lines logged; without structures, locate structure\'s replies and '
              'the saved chunks\' structure starts and references.'))
    print(f'server world gen: {len(CASES)} cases, {len(vanilla)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
