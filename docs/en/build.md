[← Back](../../README.md)

# 🔨 Build

## Requirements

- [Rust](https://rustup.rs) 1.85+
- Minecraft: Java Edition **26.3**, played once from the official launcher
- Java 25 (only to prepare resources)

## Windows

```bat
build-vise.bat          :: release (also: fast, debug)
start-vise.bat          :: prepares resources on first run, then starts the game
```

## Linux / macOS

```sh
cargo build --release -p vise-client -p vise-prepare
```

## Prepare resources

Run once. Vise builds its resources from your own Minecraft.

```sh
./target/release/vise-prepare --prepare --server-jar path/to/server.jar
./target/release/vise-prepare sounds      # optional
```

> [!TIP]
> The `server.jar` URL is `downloads.server.url` in `versions/26.3/26.3.json` of your launcher. If the game lives elsewhere, add `--minecraft-dir PATH`.

## Run

```sh
./target/release/vise-client
```

`vise-client --help` lists all options.

## Controls

| Key | Action | Key | Action |
|---|---|---|---|
| `W A S D` | Move | `E` | Inventory |
| `Space` | Jump | `Q` | Drop |
| `Shift` | Sneak | `1-9` | Hotbar |
| `Ctrl` | Sprint | `F1` | Hide HUD |
| LMB | Break | `F3` | Debug |
| RMB | Place / use | `Esc` | Pause |
