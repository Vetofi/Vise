#!/usr/bin/env python3
import argparse
import os
import shutil
import struct
import subprocess
import sys
from pathlib import Path

try:
    from . import lab, oracle_harness, test_rust_parity as base, test_model_baking_parity as baking
except ImportError:
    import lab
    import oracle_harness
    import test_rust_parity as base
    import test_model_baking_parity as baking

FLOOR = -60


def ring(x0, z0, size, state, y=FLOOR):
    return [f'fill {x0 - size} {y} {z0 - size} {x0 + size} {y} {z0 + size} {state}']


def scenarios():
    s = []

    def add(name, ticks, lines, opts=''):
        s.append((name, ticks, opts, lines))

    for species, seed in [('zombie', 1), ('skeleton', 2), ('creeper', 3), ('spider', 4), ('husk', 5),
                          ('cow', 6), ('pig', 7), ('chicken', 8), ('sheep', 9), ('wandering_trader', 10),
                          ('villager', 11), ('drowned', 12), ('enderman', 13), ('witch', 14)]:
        add(f'{species}_player', 200, [
            'player 6.5 -60 0.5',
            f'mob minecraft:{species} 0.5 -60 0.5 0 {seed} persistent',
        ])
        add(f'{species}_alone', 200, [
            f'mob minecraft:{species} 0.5 -60 0.5 90 {seed * 7 + 1} persistent',
        ])
    add('drowned_pool', 200, ['fill -6 -64 -6 6 -61 6 minecraft:water',
                              'player 9.5 -60 0.5',
                              'mob minecraft:drowned 0.5 -63 0.5 0 21 persistent'])
    add('drowned_pool_swimmer', 200, ['fill -6 -64 -6 6 -61 6 minecraft:water',
                                      'player 3.5 -62 0.5',
                                      'mob minecraft:drowned -3.5 -63 0.5 0 22 persistent'])
    add('zombie_night', 200, ['player 10.5 -60 0.5', 'mob minecraft:zombie 0.5 -60 0.5 0 31 persistent'], 'time=18000')
    add('skeleton_day_burn', 120, ['mob minecraft:skeleton 0.5 -60 0.5 0 32 persistent'])
    add('zombie_noai_day_burn', 120, ['mob minecraft:zombie 0.5 -60 0.5 0 33 persistent noai'])
    add('wither_player', 200, ['player 10.5 -60 0.5', 'mob minecraft:wither 0.5 -58 0.5 0 41 persistent'])
    add('villager_village', 300, [
        'block 4 -60 0 minecraft:composter',
        'block -4 -60 2 minecraft:red_bed[facing=north,part=foot]',
        'block -4 -60 1 minecraft:red_bed[facing=north,part=head]',
        'block 0 -60 -6 minecraft:bell',
        'player 12.5 -60 0.5',
        'mob minecraft:villager 0.5 -60 0.5 0 51 persistent',
    ])
    add('warden_digs', 140, ['mob minecraft:warden 0.5 -60 0.5 0 60'])
    add('warden_alone', 400, ['mob minecraft:warden 0.5 -60 0.5 0 61 digcooldown'])
    add('warden_player', 565, ['player 5.5 -60 0.5', 'mob minecraft:warden 0.5 -60 0.5 0 62 digcooldown'])
    add('warden_player_far', 600, ['player 14.5 -60 3.5', 'mob minecraft:warden 0.5 -60 0.5 90 63 digcooldown'])
    add('warden_creative', 300, ['player 4.5 -60 0.5 creative', 'mob minecraft:warden 0.5 -60 0.5 0 64 digcooldown'])
    add('warden_persistent', 400, ['player 7.5 -60 0.5', 'mob minecraft:warden 0.5 -60 0.5 0 65 persistent digcooldown'])
    add('warden_emerging', 300, ['player 6.5 -60 0.5', 'mob minecraft:warden 0.5 -60 0.5 0 66 emerge'])
    add('warden_cow', 176, ['mob minecraft:warden 0.5 -60 0.5 0 67 digcooldown', 'mob minecraft:cow 4.5 -60 0.5 0 68 persistent'])
    gossips = ('gossip=player:minor_negative:100 gossip=player:major_positive:15 '
               'gossip=player:trading:20 gossip=player:minor_positive:3')
    for name, opts, extra in [('villager_gossip_idle', 'time=100', []),
                              ('villager_gossip_bell', 'time=9500', ['block 0 -60 -6 minecraft:bell'])]:
        add(name, 400, extra + [
            'player 14.5 -60 0.5',
            f'mob minecraft:villager 0.5 -60 0.5 0 61 persistent {gossips}',
            'mob minecraft:villager 2.5 -60 0.5 180 62 persistent gossip=player:minor_positive:50',
        ], opts)
    beds = []
    for x in (-6, 3, 6):
        beds += [f'block {x} -60 6 minecraft:red_bed[facing=north,part=foot]',
                 f'block {x} -60 5 minecraft:red_bed[facing=north,part=head]']
    for name, seeds in [('villager_breed', (71, 72)), ('villager_breed_2', (73, 74))]:
        add(name, 900, beds + [
            'player 14.5 -60 0.5',
            f'mob minecraft:villager 0.5 -60 0.5 0 {seeds[0]} persistent inv=bread*3',
            f'mob minecraft:villager 2.5 -60 0.5 180 {seeds[1]} persistent inv=bread*3 food=4',
        ], 'time=100')
    add('villager_breed_hungry', 400, beds[:2] + [
        'mob minecraft:villager 0.5 -60 0.5 0 75 persistent inv=bread*2',
        'mob minecraft:villager 2.5 -60 0.5 180 76 persistent inv=bread*3',
    ], 'time=100')
    add('breeze_alone', 200, ['mob minecraft:breeze 0.5 -60 0.5 90 61 persistent'])
    add('breeze_player', 200, ['player 8.5 -60 0.5', 'mob minecraft:breeze 0.5 -60 0.5 0 62 persistent'])
    add('breeze_close', 200, ['player 2.5 -60 1.5', 'mob minecraft:breeze 0.5 -60 0.5 0 63 persistent'])
    add('breeze_far', 200, ['player 13.5 -60 6.5', 'mob minecraft:breeze 0.5 -60 0.5 0 64 persistent'])
    add('creaking_alone', 200, ['mob minecraft:creaking 0.5 -60 0.5 90 71 persistent'])
    add('creaking_watched', 200, ['player 0.5 -60 -7.5', 'mob minecraft:creaking 0.5 -60 0.5 180 72 persistent'])
    add('creaking_behind', 200, ['player 0.5 -60 8.5', 'mob minecraft:creaking 0.5 -60 0.5 0 73 persistent'])
    add('creaking_attack', 240, ['player 0.5 -60 -7.5 turn=40,180', 'mob minecraft:creaking 0.5 -60 0.5 180 74 persistent'])
    TRUNK = ['block 3 -60 3 minecraft:pale_oak_log[axis=y]', 'block 3 -58 3 minecraft:pale_oak_log[axis=y]']
    add('heart_night', 300, TRUNK + ['heart 3 -59 3 minecraft:creaking_heart[axis=y,creaking_heart_state=awake,natural=true]', 'player 0.5 -60 -6.5'], 'time=18000 spawning')
    add('heart_day', 120, TRUNK + ['heart 3 -59 3 minecraft:creaking_heart[axis=y,creaking_heart_state=awake,natural=true]', 'player 0.5 -60 -6.5'], 'spawning')
    add('heart_uprooted', 60, ['heart 3 -59 3 minecraft:creaking_heart[axis=y,creaking_heart_state=awake,natural=true]', 'player 0.5 -60 -6.5'], 'time=18000 spawning')
    add('heart_hurt', 260, TRUNK + ['heart 3 -59 3 minecraft:creaking_heart[axis=y,creaking_heart_state=awake,natural=true]', 'player 0.5 -60 -6.5',
                            'hurt 60 0', 'hurt 64 0', 'hurt 200 0'], 'time=18000 spawning')
    add('heart_far', 120, TRUNK + ['heart 3 -59 3 minecraft:creaking_heart[axis=y,creaking_heart_state=awake,natural=true]', 'player 40.5 -60 0.5'], 'time=18000 spawning')
    for species, seed in [('llama', 61), ('trader_llama', 62)]:
        add(f'{species}_player', 200, ['player 6.5 -60 0.5', f'mob minecraft:{species} 0.5 -60 0.5 0 {seed} persistent'])
        add(f'{species}_alone', 200, [f'mob minecraft:{species} 0.5 -60 0.5 90 {seed * 7 + 1} persistent'])
    add('llama_family', 200, ['mob minecraft:llama 0.5 -60 0.5 0 63 persistent',
                              'mob minecraft:llama 6.5 -60 2.5 0 64 persistent baby'])
    add('llama_wolf', 200, ['mob minecraft:llama 0.5 -60 0.5 0 65 persistent',
                            'mob minecraft:wolf 8.5 -60 0.5 0 66 persistent'])
    add('sniffer_player', 400, ['player 8.5 -60 0.5', 'mob minecraft:sniffer 0.5 -60 0.5 0 71 persistent'])
    for i, seed in enumerate([72, 73, 74]):
        add(f'sniffer_alone_{i}', 600, [f'mob minecraft:sniffer 0.5 -60 0.5 {90 * i} {seed} persistent'])
    add('sniffer_baby', 300, ['mob minecraft:sniffer 0.5 -60 0.5 0 75 persistent baby'])
    add('happy_ghast_player', 200, ['player 6.5 -60 0.5', 'mob minecraft:happy_ghast 0.5 -55 0.5 0 81 persistent'])
    add('happy_ghast_alone', 200, ['mob minecraft:happy_ghast 0.5 -55 0.5 90 82 persistent'])
    for name, yaw in [('hover', 0), ('turn', 90)]:
        add(f'happy_ghast_ridden_{name}', 1, ['player 0.5 -51.6 2.2',
            f'mob minecraft:happy_ghast 0.5 -55 0.5 {yaw} 82 persistent ridden'])
    add('ghastling_player', 200, ['player 8.5 -60 0.5', 'mob minecraft:happy_ghast 0.5 -57 0.5 0 83 persistent baby'])
    add('ghastling_alone', 200, ['mob minecraft:happy_ghast 0.5 -57 0.5 0 84 persistent baby'])
    add('ghastling_adult', 200, ['mob minecraft:happy_ghast 10.5 -55 0.5 0 85 persistent',
                                 'mob minecraft:happy_ghast 0.5 -57 0.5 0 86 persistent baby'])
    add('sniffer_love', 300, ['mob minecraft:sniffer 0.5 -60 0.5 0 91 persistent inlove',
                              'mob minecraft:sniffer 4.5 -60 0.5 0 92 persistent inlove'])
    add('sniffer_tempted', 200, ['player 7.5 -60 0.5 hand=minecraft:torchflower_seeds',
                                 'mob minecraft:sniffer 0.5 -60 0.5 0 93 persistent'])
    add('llama_love', 300, ['mob minecraft:llama 0.5 -60 0.5 0 94 persistent tame inlove strength=3',
                            'mob minecraft:llama 4.5 -60 0.5 0 95 persistent tame inlove strength=2'])
    add('llama_tempted', 200, ['player 7.5 -60 0.5 hand=minecraft:hay_block',
                               'mob minecraft:llama 0.5 -60 0.5 0 96 persistent'])
    add('ghastling_tempted', 200, ['player 8.5 -60 0.5 hand=minecraft:snowball',
                                   'mob minecraft:happy_ghast 0.5 -57 0.5 0 97 persistent baby'])
    add('happy_ghast_tempted', 200, ['player 12.5 -60 0.5 hand=minecraft:snowball',
                                     'mob minecraft:happy_ghast 0.5 -55 0.5 0 98 persistent'])
    add('ghastling_grows', 200, ['player 8.5 -60 0.5',
                                 'mob minecraft:happy_ghast 0.5 -57 0.5 0 99 persistent age=-40'])
    add('llama_caravan', 300, ['player 0.5 -60 0.5',
                               'mob minecraft:llama 3.5 -60 0.5 0 101 persistent leashed strength=3',
                               'mob minecraft:llama 9.5 -60 2.5 0 102 persistent strength=2',
                               'mob minecraft:llama 13.5 -60 -1.5 0 103 persistent strength=1'])
    add('trader_llamas', 300, ['player 14.5 -60 0.5',
                               'mob minecraft:wandering_trader 0.5 -60 0.5 0 104 persistent',
                               'mob minecraft:trader_llama 3.5 -60 1.5 0 105 persistent leashto=0 strength=2',
                               'mob minecraft:trader_llama 3.5 -60 -1.5 0 106 persistent leashto=0 strength=4'])
    add('leash_death_trader_llama', 10, [
        'player 14.5 -60 0.5',
        'mob minecraft:wandering_trader 0.5 -60 0.5 0 104 persistent',
        'mob minecraft:trader_llama 3.5 -60 1.5 0 105 persistent leashto=0 strength=2 dead',
    ])
    add('leash_death_player_llama', 10, [
        'player 0.5 -60 0.5',
        'mob minecraft:llama 3.5 -60 0.5 0 101 persistent leashed strength=3 dead',
    ])
    def chest(x, z, slots='-', kind='chest', facing='north', half='single', y=-60):
        return f'chest {x} {y} {z} minecraft:{kind}[facing={facing},type={half},waterlogged=false] {slots}'
    golem = 'mob minecraft:copper_golem {} -60 {} {} {} persistent cooldown=0'
    add('golem_copper_to_chest', 600, [chest(4, 0, 'cobblestone*20,-,iron_ingot*5', 'copper_chest'),
                                       chest(-4, 3), golem.format(0.5, 0.5, 0, 111)])
    add('golem_skips_other_items', 700, [chest(4, 0, 'cobblestone*40', 'copper_chest'),
                                         chest(-3, 0, 'dirt*3'), chest(-6, 5, 'cobblestone*10'),
                                         golem.format(0.5, 0.5, 90, 112)])
    add('golem_queue', 600, [chest(3, 0, 'oak_log*48,stick*7', 'copper_chest'), chest(-4, 0),
                             golem.format(0.5, 0.5, 0, 113), golem.format(0.5, 3.5, 180, 114)])
    add('golem_double_chest', 600, [chest(4, 0, 'apple*30', 'exposed_copper_chest'),
                                    chest(-4, 2, '-', 'chest', 'north', 'left'), chest(-5, 2, 'apple*2', 'chest', 'north', 'right'),
                                    golem.format(0.5, 0.5, 0, 115)])
    add('golem_blocked_chest', 400, [chest(3, 0, 'sand*5', 'copper_chest'), 'block 3 -59 0 minecraft:stone',
                                     chest(-5, -2, 'gravel*9', 'oxidized_copper_chest'), chest(0, 6),
                                     golem.format(0.5, 0.5, 0, 116)])
    add('golem_part_full', 500, [chest(-3, 1, 'cobblestone*60,cobblestone*64,-,cobblestone*63'),
                                 'mob minecraft:copper_golem 0.5 -60 0.5 0 117 persistent cooldown=0 hand=cobblestone*16'])
    add('golem_idle', 400, ['player 4.5 -60 2.5', 'mob minecraft:copper_golem 0.5 -60 0.5 0 118 persistent'])
    for species, food, seed in [('cow', 'wheat', 120), ('pig', 'carrot', 121), ('sheep', 'wheat', 122),
                                ('chicken', 'wheat_seeds', 123)]:
        for baby in ('', ' baby'):
            add(f'feed_{species}{baby.replace(" ", "_")}', 80, [
                'player 3.5 -60 0.5',
                f'mob minecraft:{species} 0.5 -60 0.5 0 {seed} persistent{baby}',
                f'feed 20 0 minecraft:{food}', f'feed 40 0 minecraft:{food}',
            ])
    for species, seed, babies, ticks, opts in [
            ('horse', 180, True, 80, ''), ('donkey', 181, True, 80, ''), ('mule', 182, False, 80, ''),
            ('zombie_horse', 183, False, 80, 'time=18000'), ('llama', 184, True, 80, ''),
            ('skeleton_horse', 185, False, 45, '')]:
        for baby in ('', ' baby') if babies else ('',):
            add(f'sword_{species}{baby.replace(" ", "_")}', ticks, [
                'player 3.5 -60 0.5',
                f'mob minecraft:{species} 0.5 -60 0.5 0 {seed} persistent{baby}',
                'interact 20 0 minecraft:iron_sword', 'interact 30 0 minecraft:iron_sword',
                'interact 40 0 minecraft:iron_sword',
            ], opts)
    LAVA = 'fill -5 -63 -5 5 -61 5 minecraft:lava'
    add('strider_lava', 200, [LAVA, 'mob minecraft:strider 0.5 -61 0.5 0 131 persistent'])
    add('strider_lava_player', 200, [LAVA, 'player 8.5 -60 0.5',
                                     'mob minecraft:strider -1.5 -61 0.5 90 132 persistent'])
    add('strider_shore', 300, [LAVA, 'mob minecraft:strider 9.5 -60 0.5 90 133 persistent'])
    add('strider_cold', 200, ['mob minecraft:strider 0.5 -60 0.5 0 134 persistent'])
    add('strider_tempted', 200, [LAVA, 'player 9.5 -60 0.5 hand=minecraft:warped_fungus',
                                 'mob minecraft:strider 0.5 -61 0.5 0 135 persistent'])
    add('ocelot_alone', 200, ['mob minecraft:ocelot 0.5 -60 0.5 0 181 persistent'])
    add('ocelot_player', 200, ['player 6.5 -60 0.5', 'mob minecraft:ocelot 0.5 -60 0.5 0 182 persistent'])
    add('ocelot_player_far', 200, ['player 14.5 -60 0.5', 'mob minecraft:ocelot 0.5 -60 0.5 0 183 persistent'])
    add('ocelot_creative', 200, ['player 6.5 -60 0.5 creative', 'mob minecraft:ocelot 0.5 -60 0.5 0 184 persistent'])
    add('ocelot_tempted', 200, ['player 8.5 -60 0.5 hand=minecraft:cod',
                                'mob minecraft:ocelot 0.5 -60 0.5 0 185 persistent'])
    CEILING = 'fill -6 -57 -6 6 -57 6 minecraft:stone'
    add('bat_hanging', 200, [CEILING, 'mob minecraft:bat 0.5 -57.9 0.5 0 141 persistent'])
    add('bat_woken', 200, [CEILING, 'player 1.5 -60 0.5', 'mob minecraft:bat 0.5 -57.9 0.5 0 142 persistent'])
    add('bat_flying', 300, ['mob minecraft:bat 0.5 -57 0.5 0 143 persistent'])
    CAVE = ['fill -5 -60 -5 5 -54 5 minecraft:stone', 'fill -4 -60 -4 4 -55 4 minecraft:air']
    for i, seed in enumerate([144, 145, 146]):
        add(f'bat_cave_{i}', 400, CAVE + [f'mob minecraft:bat 0.5 -58 0.5 {90 * i} {seed} persistent'])
    FLOWERS = 'fill 3 -60 2 5 -60 4 minecraft:dandelion'
    for i, seed in enumerate([151, 152]):
        add(f'bee_alone_{i}', 400, [f'mob minecraft:bee 0.5 -58 0.5 {90 * i} {seed} persistent'])
    for i, seed in enumerate([153, 154]):
        add(f'bee_flowers_{i}', 700, [FLOWERS, f'mob minecraft:bee 0.5 -58 0.5 {180 * i} {seed} persistent'])
    add('bee_tempted', 200, ['player 6.5 -60 0.5 hand=minecraft:dandelion', 'mob minecraft:bee 0.5 -58 0.5 0 155 persistent'])
    add('bee_night', 300, [FLOWERS, 'mob minecraft:bee 0.5 -58 0.5 0 156 persistent'], 'time=18000')
    add('bee_rain', 300, [FLOWERS, 'mob minecraft:bee 0.5 -58 0.5 0 157 persistent'], 'rain')
    add('bee_pool', 200, ['fill -3 -63 -3 3 -61 3 minecraft:water', 'mob minecraft:bee 0.5 -62 0.5 0 158 persistent'])
    add('bee_sting', 400, ['player 4.5 -60 0.5', 'mob minecraft:bee 0.5 -58 0.5 0 159 persistent', 'hurt 20 0'])
    HIVE = 'block 4 -60 0 minecraft:beehive[facing=west,honey_level=0]'
    add('bee_hive_night', 300, [HIVE, 'mob minecraft:bee 0.5 -58 0.5 0 161 persistent'], 'time=13000')
    add('bee_hive_rain', 300, [HIVE, 'mob minecraft:bee 0.5 -58 0.5 0 162 persistent'], 'rain')
    add('bee_hive_morning', 1000, [HIVE, 'mob minecraft:bee 0.5 -58 0.5 0 163 persistent'], 'time=22900 daycycle')
    add('bee_hive_honey', 2600, [HIVE, 'mob minecraft:bee 0.5 -58 0.5 0 164 persistent nectar'])
    TREE = ['fill 4 -60 4 4 -56 4 minecraft:oak_log', 'fill 2 -55 2 6 -55 6 minecraft:oak_leaves[persistent=true]']
    for i, seed in enumerate([181, 182]):
        add(f'parrot_alone_{i}', 400, [f'mob minecraft:parrot 0.5 -60 0.5 {90 * i} {seed} persistent'], 'difficulty=0')
    add('parrot_tree', 400, TREE + ['mob minecraft:parrot 0.5 -60 0.5 0 183 persistent'], 'difficulty=0')
    add('parrot_falling', 100, ['mob minecraft:parrot 0.5 -50 0.5 0 184 persistent'], 'difficulty=0')
    add('wolf_hurt', 200, ['player 5.5 -60 0.5', 'mob minecraft:wolf 0.5 -60 0.5 0 181 persistent', 'hurt 20 0'])
    add('wolf_shake', 200, ['fill 0 -61 0 0 -61 0 minecraft:water', 'mob minecraft:wolf 0.5 -61 0.5 0 183 persistent'])
    add('wolf_rain', 100, ['mob minecraft:wolf 0.5 -60 0.5 0 184 persistent'], 'rain')
    for species, seed in [('pig', 171), ('cow', 172), ('wolf', 173), ('cat', 174), ('horse', 175),
                          ('witch', 176), ('vindicator', 177), ('zombie', 178)]:
        add(f'{species}_deep_water', 160, ['fill -4 -64 -4 4 -61 4 minecraft:water',
                                           f'mob minecraft:{species} 0.5 -62 0.5 0 {seed} persistent'])
    add('ghast_player', 120, ['player 7.5 -56 0.5', 'mob minecraft:ghast 0.5 -56 0.5 0 273 persistent'])
    add('llama_hurt', 200, ['player 4.5 -60 0.5', 'mob minecraft:llama 0.5 -60 0.5 0 182 persistent']
        + [f'hurt {t} 0' for t in range(10, 190, 25)])
    add('pig_tempt_fire', 100, ['player 9.5 -60 0.5 hand=minecraft:carrot', 'fill 3 -61 -3 3 -61 3 minecraft:netherrack',
                                'fill 3 -60 -3 3 -60 3 minecraft:fire', 'mob minecraft:pig 0.5 -60 0.5 0 221 persistent'])
    for seed in (201, 202):
        add(f'villager_grows_up_{seed}', 120, ['player 8.5 -60 0.5',
                                               f'mob minecraft:villager 0.5 -60 0.5 0 {seed} persistent age=-30'], 'time=2000')
    add('skeleton_sun_path', 120, ['fill -5 -57 -5 5 -57 5 minecraft:stone', 'fill -5 -56 -5 5 -56 5 minecraft:air',
                                   'player 12.5 -60 0.5', 'mob minecraft:skeleton 0.5 -60 0.5 0 211 persistent'], 'time=1000')
    add('zombified_piglin_player', 120, ['player 7.5 -60 0.5', 'mob minecraft:zombified_piglin 0.5 -60 0.5 0 241 persistent'])
    add('wither_skeleton_player', 100, ['player 9.5 -60 0.5', 'mob minecraft:wither_skeleton 0.5 -60 0.5 0 222 persistent'])
    for seed in range(191, 193):
        add(f'chicken_panic_{seed}', 60, ['fill -3 -61 -6 3 -61 6 minecraft:water',
                                          'player 12.5 -60 6.5', f'mob minecraft:chicken 0.5 -60 6.5 0 {seed} persistent']
            + [f'hurt {t} 0' for t in range(5, 60, 30)])
    add('chicken_panic_dry', 60, ['player 12.5 -60 0.5', 'mob minecraft:chicken 0.5 -60 0.5 0 195 persistent', 'hurt 5 0'])
    add('pig_panic_dry', 60, ['player 12.5 -60 0.5', 'mob minecraft:pig 0.5 -60 0.5 0 196 persistent', 'hurt 5 0'])
    add('cat_player', 120, ['player 6.5 -60 0.5', 'mob minecraft:cat 0.5 -60 0.5 0 261 persistent'])
    POOL = 'fill -5 -63 -5 5 -61 5 minecraft:water'
    add('axolotl_hunting', 45, [POOL, 'player 12.5 -60 0.5', 'mob minecraft:axolotl 0.5 -62 0.5 0 12 persistent',
                                'mob minecraft:cod 2.5 -62 0.5 0 13 persistent noai'])
    add('axolotl_play_dead', 300, [POOL, 'player 12.5 -60 0.5', 'mob minecraft:axolotl 0.5 -62 0.5 0 28 persistent']
        + [f'hurt {t} 0' for t in range(4, 160, 12)])
    for species, seed in [('cod', 181), ('salmon', 182), ('tropical_fish', 183)]:
        add(f'{species}_random_swim', 120, [POOL, 'player 12.5 -60 0.5',
                                            f'mob minecraft:{species} 0.5 -62 0.5 0 {seed} persistent'])
    add('villager_job_in_box', 300, [
        'fill -4 -60 -4 4 -58 4 minecraft:stone',
        'fill -3 -60 -3 3 -58 3 minecraft:air',
        'fill -4 -60 0 -4 -59 0 minecraft:air',
        'block 0 -60 0 minecraft:composter',
        'mob minecraft:villager 10.5 -60 0.5 90 54 persistent',
    ])
    golem_callers = [f'mob minecraft:villager {x} -60 {z} {r} {seed} persistent slept=1' for x, z, r, seed in
                     [(0.5, 0.5, 0, 81), (2.5, 0.5, 180, 82), (0.5, 2.5, 90, 83), (2.5, 2.5, 270, 84), (1.5, -1.5, 0, 85)]]
    add('villager_golem_gossip', 200, ['player 14.5 -60 0.5'] + golem_callers, 'time=100')
    add('villager_golem_roof', 200, ['fill -10 -57 -12 13 -57 13 minecraft:glass', 'player 14.5 -60 0.5'] + golem_callers,
        'time=100')
    add('villager_golem_paths', 400, ['fill -10 -61 -12 13 -61 13 minecraft:dirt_path', 'player 14.5 -60 0.5'] + golem_callers,
        'time=100')
    add('villager_golem_panic', 100, ['fill -2 -60 -3 4 -60 4 minecraft:oak_fence', 'fill -1 -60 -2 3 -60 3 minecraft:air',
                                      'player 14.5 -60 0.5'] + golem_callers[:3]
        + ['mob minecraft:zombie 6.5 -60 0.5 90 86 persistent noai'], 'time=18000')
    for difficulty in range(4):
        for flags in ('', 'persistent', 'named', 'noai', 'persistent noai'):
            suffix = flags.replace(' ', '_') or 'ordinary'
            add(f'peaceful_zombie_{difficulty}_{suffix}', 1, [
                'player 6.5 -60 0.5',
                f'mob minecraft:zombie 0.5 -60 0.5 0 1 {flags}',
            ], f'difficulty={difficulty} time=18000')
    for species in ('skeleton', 'creeper', 'warden', 'wither', 'shulker', 'wolf', 'iron_golem', 'cow'):
        add(f'peaceful_type_{species}', 1, [
            'player 6.5 -60 0.5',
            f'mob minecraft:{species} 0.5 -60 0.5 0 1 persistent noai',
        ], 'difficulty=0 time=18000')

    return s


def write_scenarios(path, chosen):
    lines = []
    for i, (name, ticks, opts, body) in chosen:
        ox, oz = 512 * (i + 1), 0
        lines.append(f'scenario {name} ticks={ticks} {opts}'.rstrip())
        lines.append(f'origin {ox} {oz}')
        for line in body:
            f = line.split()
            head, f = (f[:2], f[2:]) if f[0] == 'at' else ([], f)
            if f[0] == 'player':
                f[1] = repr(float(f[1]) + ox)
                f[3] = repr(float(f[3]) + oz)
            elif f[0] == 'mob':
                f[2] = repr(float(f[2]) + ox)
                f[4] = repr(float(f[4]) + oz)
            elif f[0] in ('block', 'heart', 'chest'):
                f[1] = str(int(f[1]) + ox)
                f[3] = str(int(f[3]) + oz)
            elif f[0] == 'fill':
                for k in (1, 4):
                    f[k] = str(int(f[k]) + ox)
                for k in (3, 6):
                    f[k] = str(int(f[k]) + oz)
            lines.append(' '.join(head + f))
        lines.append('end')
    path.write_text('\n'.join(lines) + '\n')


def parse(path):
    result, cur = {}, None
    for line in path.read_text().splitlines():
        if line.startswith('== '):
            cur = result.setdefault(line[3:], {'ticks': {}, 'sounds': {}})
            continue
        head, *fields = line.split('\t')
        parts = head.split(' ', 2)
        t = int(parts[0])
        if parts[1] == 'sound':
            cur['sounds'].setdefault(t, []).append(parts[2])
            continue
        if parts[1] == 'block':
            cur.setdefault('blocks', {}).setdefault(t, []).append(parts[2])
            continue
        if parts[1] == 'trace':
            cur.setdefault('trace', {}).setdefault(t, []).append(parts[2])
            continue
        if parts[1] == 'heart':
            cur.setdefault('hearts', {}).setdefault(t, []).append(line.split(' ', 2)[2])
            continue
        if parts[1] == 'hive':
            cur.setdefault('hives', {}).setdefault(t, []).append(parts[2])
            continue
        if parts[1] == 'leads':
            cur.setdefault('leads', {}).setdefault(t, []).append(parts[2])
            continue
        if parts[1] == 'chest':
            cur.setdefault('chests', {}).setdefault(t, []).append(parts[2])
            continue
        mob = int(parts[1])
        row = {}
        if len(parts) > 2:
            row['removed'] = parts[2]
        for f in fields:
            k, _, v = f.partition(' ')
            row[k] = v
        cur['ticks'].setdefault(t, {})[mob] = row
    return result


def decode(value):
    def one(x):
        try:
            if len(x) > 8:
                return f'{struct.unpack(">d", int(x, 16).to_bytes(8, "big"))[0]!r}'
            return f'{struct.unpack(">f", int(x, 16).to_bytes(4, "big"))[0]:.9g}'
        except ValueError:
            return x
    out = []
    for word in value.split(' '):
        out.append(','.join(one(p) if all(c in '0123456789abcdef' for c in p) and len(p) >= 5 else p for p in word.split(',')))
    return ' '.join(out)


MULT, MASK = 0x5DEECE66D, (1 << 48) - 1


def draws(before, after, limit=20000):
    try:
        s, goal = int(before, 16), int(after, 16)
    except (TypeError, ValueError):
        return '?'
    for n in range(limit):
        if s == goal:
            return n
        s = (s * MULT + 11) & MASK
    return f'>{limit}'


def compare(vanilla, rust, fields=None):
    report, totals = [], {'mob_ticks': 0, 'matched': 0, 'scenarios': 0, 'clean': 0}
    for name, v in vanilla.items():
        r = rust.get(name)
        totals['scenarios'] += 1
        if r is None:
            report.append(f'{name}: missing from Vise')
            continue
        mobs = sorted({m for t in v['ticks'].values() for m in t})
        clean = True
        for mob in mobs:
            ticks = sorted(v['ticks'])
            first = None
            for t in ticks:
                a = v['ticks'][t].get(mob)
                b = r['ticks'].get(t, {}).get(mob)
                totals['mob_ticks'] += 1
                if a == b or (b is not None and fields and all(a.get(k) == b.get(k) for k in fields)):
                    if first is None:
                        totals['matched'] += 1
                    continue
                if first is None:
                    first = t
                    diffs = []
                    for k in (a or {}):
                        if b is None or a.get(k) != b.get(k):
                            show = (lambda x: x) if k == 'rng' else decode
                            diffs.append(f'{k}: vanilla {show(a.get(k, "?"))} | vise {show((b or {}).get(k, "?"))}')
                    for k in (b or {}):
                        if a is None or k not in a:
                            diffs.append(f'{k}: vanilla ? | vise {decode(b[k])}')
                    prev_v = v['ticks'].get(t - 1, {}).get(mob, {}).get('rng')
                    prev_r = r['ticks'].get(t - 1, {}).get(mob, {}).get('rng')
                    report.append(f'{name} mob {mob}: first divergence at tick {t} of {len(ticks)} '
                                  f'(random draws this tick: vanilla {draws(prev_v, (a or {}).get("rng"))}, '
                                  f'vise {draws(prev_r, (b or {}).get("rng"))})')
                    for line in [x for x in v.get('trace', {}).get(t, []) if x.split(' ', 1)[0] == str(mob)][:40]:
                        report.append('      vanilla ' + line)
                    for line in [x for x in r.get('trace', {}).get(t, []) if x.split(' ', 1)[0] == str(mob)][:40]:
                        report.append('      vise ' + line)
                    report.extend('    ' + d for d in diffs[:12])
            if first is not None:
                clean = False
        for t in sorted(set(v.get('hearts', {})) | set(r.get('hearts', {}))):
            if v.get('hearts', {}).get(t) != r.get('hearts', {}).get(t):
                report.append(f'{name}: hearts differ at tick {t}: vanilla {v.get("hearts", {}).get(t)} | vise {r.get("hearts", {}).get(t)}')
                clean = False
                break
        for kind in ('sounds', 'blocks', 'chests', 'hives', 'leads'):
            va, ra = v.get(kind, {}), r.get(kind, {})
            for t in sorted(set(va) | set(ra)):
                if va.get(t, []) != ra.get(t, []):
                    report.append(f'{name}: {kind} differ at tick {t}: vanilla {va.get(t, [])} | vise {ra.get(t, [])}')
                    clean = False
                    break
        totals['clean'] += clean
        if clean:
            report.append(f'{name}: all {len(v["ticks"])} ticks match')
    return report, totals


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--only', nargs='*')
    parser.add_argument('--java-only', action='store_true')
    parser.add_argument('--rust-only', action='store_true', help='reuse the last vanilla output')
    parser.add_argument('--report-only', action='store_true', help='print divergences without failing')
    parser.add_argument('--trace', action='store_true',
                        help="also trace Vise's mob random draws (rng-trace probe with line tables)")
    args = parser.parse_args()
    build = lab.LOCAL / 'mob-ai-parity'
    build.mkdir(parents=True, exist_ok=True)
    chosen = [(i, s) for i, s in enumerate(scenarios()) if not args.only or any(o in s[0] for o in args.only)]
    inputs = build / 'scenarios.txt'
    write_scenarios(inputs, chosen)
    lock, jars, _ = base.verify_artifacts()
    if not args.rust_only:
        classes, dependencies, _ = oracle_harness.compile_oracle(build, 'VanillaMobAiOracle', jars, baking.client_libraries(),
                                                                lock['minecraft']['java_major'])
        world = build / 'world'
        shutil.rmtree(world, ignore_errors=True)
        oracle_harness.run_logged(['java', '-Xmx8g', '-Djava.awt.headless=true', '-cp', str(classes) + os.pathsep + dependencies,
                                   'dev.viselab.parity.VanillaMobAiOracle', inputs, build / 'vanilla.txt', world],
                                  build, 'vanilla', timeout=3600)
    if not args.java_only:
        command = ['cargo', 'build', '--locked', '--profile', 'fast', '-p', 'vise-client', '--example', 'client_probe']
        target, env = lab.cargo_target(), None
        if args.trace:
            target = lab.LOCAL / 'mob-ai-trace-target'
            command += ['--features', 'rng-trace']
            env = dict(os.environ, CARGO_TARGET_DIR=str(target), CARGO_PROFILE_RELEASE_DEBUG='line-tables-only',
                       CARGO_PROFILE_RELEASE_LTO='false', CARGO_PROFILE_RELEASE_CODEGEN_UNITS='16')
        oracle_harness.run_logged(command, build, 'compile-rust', env=env)
        binary = target / 'fast/examples/client_probe'
        oracle_harness.run_logged([binary, 'mob_ai_probe', lab.RESOURCES, inputs, build / 'vise.txt'], build, 'vise')
    if args.java_only:
        return
    names = {s[0] for _, s in chosen}
    vanilla = {k: v for k, v in parse(build / 'vanilla.txt').items() if k in names}
    report, totals = compare(vanilla, parse(build / 'vise.txt'))
    (build / 'report.txt').write_text('\n'.join(report) + '\n')
    print('\n'.join(report))
    print(f"{totals['clean']}/{totals['scenarios']} scenarios match entirely; "
          f"{totals['matched']}/{totals['mob_ticks']} mob-ticks match before each mob's first divergence")
    if not args.report_only and totals['clean'] != totals['scenarios']:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
