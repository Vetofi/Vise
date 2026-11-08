#!/usr/bin/env python3
import json
import shutil
import zipfile

try:
    from . import lab, oracle_harness as harness
except ImportError:
    import lab, oracle_harness as harness

FORMAT = {"min_format": [97, 0], "max_format": [97, 1]}


def write(root, files):
    for name, data in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data if isinstance(data, bytes) else json.dumps(data).encode())


def packs(folder):
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir(parents=True)
    write(folder / "Alpha", {
        "pack.mcmeta": {"pack": {"description": "alpha", **FORMAT}, "overlays": {"entries": [
            {"directory": "alpha_now", **FORMAT},
            {"directory": "alpha_old", "min_format": [80, 0], "max_format": [80, 0]}]}},
        "assets/minecraft/textures/block/stone.png": b"alpha stone",
        "assets/minecraft/textures/block/granite.png": b"alpha granite",
        "alpha_now/assets/minecraft/textures/block/dirt.png": b"alpha overlay dirt",
        "alpha_old/assets/minecraft/textures/block/sand.png": b"old overlay sand",
        "assets/minecraft/lang/en_us.json": {"block.minecraft.stone": "Alpha Stone", "vise.k1": "A1",
                                             "vise.k2": "A2"},
        "assets/minecraft/sounds.json": {
            "vise.test": {"subtitle": "subtitles.vise.a", "sounds": ["vise/a1"]},
            "block.stone.break": {"sounds": [{"name": "vise/a2", "weight": 2}]},
            "vise.ref": {"sounds": [{"name": "vise.test", "type": "event"}]}},
        "assets/minecraft/sounds/vise/a1.ogg": b"a1",
        "assets/minecraft/sounds/vise/a2.ogg": b"a2",
        "assets/minecraft/font/default.json": {"providers": [{"type": "space", "advances": {" ": 9}}]},
        "assets/minecraft/atlases/blocks.json": {"sources": [{"type": "single", "resource": "vise:block/a"}]},
        "assets/vise/lang/en_us.json": {"vise.ns": "Alpha namespace"},
    })
    beta = folder / "beta-src"
    write(beta, {
        "pack.mcmeta": {"pack": {"description": "beta", **FORMAT},
                        "filter": {"block": [{"namespace": "minecraft", "path": "textures/block/stone\\.png"}]}},
        "assets/minecraft/textures/block/granite.png": b"beta granite",
        "assets/minecraft/lang/en_us.json": {"vise.k1": "B1"},
        "assets/minecraft/sounds.json": {
            "vise.test": {"replace": True, "subtitle": "subtitles.vise.b",
                             "sounds": ["vise/b1", {"name": "vise/a1", "weight": 3}]},
            "block.stone.break": {"sounds": ["vise/missing"]}},
        "assets/minecraft/sounds/vise/b1.ogg": b"b1",
        "assets/minecraft/font/default.json": {"providers": [{"type": "space", "advances": {" ": 4}},
                                                             {"type": "space", "advances": {" ": 5}}]},
        "assets/minecraft/atlases/blocks.json": {"sources": [{"type": "single", "resource": "vise:block/b"}]},
    })
    write(folder / "Gamma", {
        "pack.mcmeta": {"pack": {"description": "gamma", **FORMAT}},
        "assets/minecraft/sounds.json": {"vise.test": {"sounds": ["vise/a1"]}, "bad": 3},
        "assets/minecraft/lang/en_us.json": {"vise.k2": "G2"},
    })
    with zipfile.ZipFile(folder / "Beta.zip", "w") as z:
        for path in sorted(beta.rglob("*")):
            if path.is_file():
                z.write(path, path.relative_to(beta).as_posix())
    shutil.rmtree(beta)


QUERIES = [
    "file minecraft:textures/block/stone.png",
    "file minecraft:textures/block/granite.png",
    "file minecraft:textures/block/dirt.png",
    "file minecraft:textures/block/sand.png",
    "file minecraft:textures/block/oak_planks.png",
    "lang block.minecraft.stone",
    "lang block.minecraft.dirt",
    "lang vise.k1",
    "lang vise.k2",
    "lang vise.ns",
    "lang menu.singleplayer",
    "sound vise.test",
    "sound vise.ref",
    "sound block.stone.break",
    "sound block.stone.place",
    "font minecraft:default",
    "font minecraft:uniform",
    "atlas minecraft:blocks",
    "atlas minecraft:gui",
]
SELECTIONS = [
    ["file/Alpha", "file/Beta.zip", "file/Gamma"],
    ["file/Beta.zip", "file/Alpha"],
    ["file/Alpha"],
    [],
]


def main():
    total = 0
    for index, selection in enumerate(SELECTIONS):
        def make(build, selection=selection):
            folder = build / "resourcepacks"
            packs(folder)
            vanilla = lab.RESOURCES
            path = build / "inputs.txt"
            path.write_text(f"root {vanilla} {folder} {' '.join(selection)}".rstrip() + "\n"
                            + "".join(q + "\n" for q in QUERIES))
            return path, len(QUERIES)
        harness.differential(f"resource-pack-parity-{index}", "VanillaPackResourcesOracle",
                                    "pack_resources_fixture", make,
                                    "Pack discovery, overlays, filters and the resource stacks vanilla merges.",
                                    sources=[lab.ROOT / "crates/vise-client-render/src/packs.rs",
                                             lab.ROOT / "crates/vise-client-render/src/sound_events.rs",
                                             lab.ROOT / "crates/vise-client-core/src/lang.rs"])
        total += len(QUERIES)
    print(f"{total} resource pack queries match the pinned client over {len(SELECTIONS)} selections")


if __name__ == "__main__":
    main()
