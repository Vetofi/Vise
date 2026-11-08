from pathlib import Path
from contextlib import contextmanager
import os
import subprocess
import tempfile

try:
    from . import lab
except ImportError:
    import lab


def game_environment(base, root):
    root = Path(root)
    env = dict(base, VISE_TELEMETRY_URL='off')
    for key, folder in [('VISE_HOME', 'home'), ('VISE_LAUNCHER_HOME', 'launcher'),
                        ('XDG_DATA_HOME', 'data'), ('XDG_CONFIG_HOME', 'config')]:
        path = root / folder
        path.mkdir(parents=True, exist_ok=True)
        env[key] = str(path)
    return lab.game_env(env)


def run_game(command, **kwargs):
    with tempfile.TemporaryDirectory(prefix='hz-fixture-') as root:
        kwargs['env'] = game_environment(kwargs.pop('env', os.environ), root)
        if any(key in kwargs['env'] for key in
               ('VISE_COMMAND_SCRIPT', 'VISE_DISPENSE_FIXTURE', 'VISE_SCULK_FIXTURE')):
            command = [*command, '--in-frame']
        return subprocess.run(command, **kwargs)


@contextmanager
def suite_lock():
    import fcntl
    with (Path(tempfile.gettempdir()) / 'vise-integration-gpu.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def require_server_reports(resources):
    resources = Path(resources)
    reports = next((p for p in (resources, resources / '../../datagen/out/reports',
                                resources / '../datagen/out/reports')
                    if (p / 'registries.json').is_file()), None)
    if reports is None or not (reports / 'commands.json').is_file():
        raise ValueError(f'incomplete prepared resources in {resources}: registries.json and '
                         'commands.json must be installed together; prepare the pinned resources again')
    return reports
