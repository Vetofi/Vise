import os


def shard(items):
    spec = os.environ.get("PARITY_SHARD")
    if not spec:
        return items
    index, count = map(int, spec.split("/"))
    return items[index - 1::count]


def label():
    return os.environ.get("PARITY_SHARD", "all").replace("/", "-")
