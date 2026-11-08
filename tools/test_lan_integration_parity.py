#!/usr/bin/env python3
import lan_e2e
from integration_env import suite_lock


if __name__ == '__main__':
    with suite_lock():
        lan_e2e.main()
