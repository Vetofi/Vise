#!/usr/bin/env python3
import sys

try:
    from . import test_locator_bar_parity as locator
except ImportError:
    import test_locator_bar_parity as locator

NAME = 'cursor-death-parity'
locator.PROPERTIES = dict(locator.PROPERTIES, gamemode='survival')
SCRIPT = [
    ('setup', ['cmd Alice gamerule keep_inventory false', 'cmd Alice clear @s',
               'cmd Alice give @s minecraft:diamond 7', 'cmd Alice tp @s 10.5 -60 0.5']),
    ('pickup', ['pickup Alice 36']),
    ('death', ['cmd Alice kill @s']),
    ('respawn', ['respawn Alice']),
    ('check', ['chat on', 'cmd Alice execute if entity @e[type=minecraft:item,x=10.5,y=-60,z=0.5,distance=..4]']),
]


def shown(lines):
    return [line for line in lines if len(parts := line.split(' ', 3)) >= 3
            and parts[1] in ('step=death', 'step=respawn', 'step=check')
            and parts[2] in ('slot', 'inventory', 'respawn', 'teleport', 'chat')]


def check(vanilla, _):
    if not any(l.startswith('Alice step=check chat') for l in vanilla):
        return 'vanilla answered no check command'
    return None


def main():
    lines = locator.one_player(NAME, SCRIPT, shown, check, 'A player dying with a stack on the inventory menu cursor '
                               '(keep_inventory off) and respawning on a dedicated server: the stack is dropped '
                               'where it died, not kept.')
    print(f'cursor death: {len(lines)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
