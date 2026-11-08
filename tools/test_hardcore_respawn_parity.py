#!/usr/bin/env python3
import sys

try:
    from . import test_locator_bar_parity as locator
except ImportError:
    import test_locator_bar_parity as locator

NAME = 'hardcore-respawn-parity'
locator.PROPERTIES = dict(locator.PROPERTIES, hardcore='true', gamemode='survival')
SCRIPT = [
    ('death', ['cmd Alice kill @s']),
    ('respawn', ['respawn Alice']),
]


def shown(lines):
    return [line for line in lines if len(parts := line.split(' ', 3)) >= 3
            and parts[1] in ('step=death', 'step=respawn') and parts[2] in ('gamemode', 'respawn')]


def check(vanilla, _):
    if 'Alice step=respawn gamemode 3' not in vanilla:
        return 'vanilla did not make the respawned player a spectator'
    return None


def main():
    lines = locator.one_player(NAME, SCRIPT, shown, check, 'A player dying and respawning in a hardcore world on a '
                               'dedicated server: the respawn and the game mode it is sent (spectator).')
    print(f'hardcore respawn: {len(lines)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
