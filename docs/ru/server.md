[← Назад](../../README.ru.md)

# 🖥️ Сервер

## Запуск

```sh
vise-server --dir my-server init       # EULA, мир, сид, порт, вайтлист
vise-server --dir my-server start      # в фоне (также: stop, restart)
vise-server --dir my-server status     # TPS, MSPT, игроки, RAM, CPU
vise-server --dir my-server cmd list   # консольная команда
vise-server --dir my-server backup     # также: restore
vise-server --dir my-server service install   # автозапуск
```

Настройки: `my-server/server.toml`. Все команды: `vise-server help`.

## Docker

```sh
docker build -f packaging/server/Dockerfile -t vise-server .
docker run -p 25565:25565 -v vise-data:/data vise-server
```

## Скрипт установки

**Linux x86_64 / macOS arm64**

```sh
curl -fsSL https://raw.githubusercontent.com/Vetofi/Vise/main/packaging/server/install.sh | sh
```

**Windows:** [`install.ps1`](../../packaging/server/install.ps1)

Релизы подписаны: установщик проверяет `SHA256SUMS` перед установкой.
