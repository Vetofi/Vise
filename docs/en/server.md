[← Back](../../README.md)

# 🖥️ Server

## Run

```sh
vise-server --dir my-server init       # EULA, world, seed, port, whitelist
vise-server --dir my-server start      # background (also: stop, restart)
vise-server --dir my-server status     # TPS, MSPT, players, RAM, CPU
vise-server --dir my-server cmd list   # run a console command
vise-server --dir my-server backup     # also: restore
vise-server --dir my-server service install   # start at boot
```

Settings: `my-server/server.toml`. All commands: `vise-server help`.

## Docker

```sh
docker build -f packaging/server/Dockerfile -t vise-server .
docker run -p 25565:25565 -v vise-data:/data vise-server
```

## Install script

**Linux x86_64 / macOS arm64**

```sh
curl -fsSL https://raw.githubusercontent.com/Vetofi/Vise/main/packaging/server/install.sh | sh
```

**Windows:** [`install.ps1`](../../packaging/server/install.ps1)

Releases are signed; the installer checks `SHA256SUMS` before installing.
