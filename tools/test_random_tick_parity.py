#!/usr/bin/env python3
import random
try:
    from . import block_update_harness as h
except ImportError:
    import block_update_harness as h


def cases(facts, count, seed, only):
    rng = random.Random(seed)
    pool = h.state_pool(facts, only)
    for n in range(count):
        state = pool[n % len(pool)]
        case = {**h.environment(rng), 'pos': [0, 0, 0], 'cells': facts.cells(rng, state),
                'mode': 'random' if n % 4 else 'tick'}
        yield case


def main():
    args = h.argument_parser(__doc__).parse_args()
    facts = h.load_facts()
    differing = h.execute('random-tick-parity', list(cases(facts, args.count, args.seed, args.only)), args, facts)
    h.finish(differing)


if __name__ == '__main__':
    main()
