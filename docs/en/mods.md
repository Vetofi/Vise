[← Back](../../README.md)

# 🧩 Mods

## Create

```sh
cargo vise-mod new my-mod
cargo vise-mod build
cargo vise-mod run
cargo vise-mod check
```

## What you get

| | |
|---|---|
| **API** | Fabric-style: events with phases, registries, entrypoints |
| **Format** | `.vmod` files, modpacks, registry index |
| **Shaders** | Own packs in WGSL |

Example mod: [`mods/ruby.vmod`](../../mods/ruby.vmod)

## Port a Java modpack

The [`vise-port-pack`](../../crates/vise-port-pack) crate imports a Java modpack, inventories and classifies its mods, and carries data over.
