#!/usr/bin/env python3
import sys

try:
    from . import test_locator_bar_parity as locator
except ImportError:
    import test_locator_bar_parity as locator

NAME = 'lunge-motion-parity'
locator.PROPERTIES = dict(locator.PROPERTIES, gamemode='survival')
SCRIPT = [
    ('setup', ['cmd Alice give @s minecraft:netherite_spear[enchantments={lunge:3}]', 'arc Alice -1']),
    ('ground', ['stab Alice']),
    ('same-tick', ['arc Alice 0']),
    ('rising', ['arc Alice 3']),
    ('falling', ['arc Alice 8']),
    ('sprinting', ['sprint Alice', 'arc Alice 3']),
]


def shown(lines):
    return [line for line in lines if len(parts := line.split(' ', 3)) >= 3 and parts[1] != 'step=setup'
            and parts[2] == 'motion']


def check(vanilla, vanilla_all):
    steps = {line.split()[1] for line in vanilla}
    missing = {f'step={step}' for step, _ in SCRIPT[1:]} - steps
    return f'vanilla sent no motion in {sorted(missing)}' if missing else None


def main():
    lines = locator.one_player(NAME, SCRIPT, shown, check, "A spear's Lunge standing and at three points of a jump "
                               'on a dedicated server: the motion the player is sent.', step_seconds=2.0)
    print(f'lunge motion: {len(lines)} lines identical')


if __name__ == '__main__':
    try:
        main()
    except AssertionError as error:
        sys.exit(f'{NAME}: {error}')
