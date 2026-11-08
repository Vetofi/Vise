[← Назад](../../README.ru.md)

# 🔨 Сборка

## Требования

- [Rust](https://rustup.rs) 1.85+
- Minecraft: Java Edition **26.3**, запущенный один раз из официального лаунчера
- Java 25 (только для подготовки ресурсов)

## Windows

```bat
build-vise.bat          :: релиз (также: fast, debug)
start-vise.bat          :: при первом запуске готовит ресурсы, затем запускает игру
```

## Linux / macOS

```sh
cargo build --release -p vise-client -p vise-prepare
```

## Подготовка ресурсов

Делается один раз. Vise собирает ресурсы из вашего Minecraft.

```sh
./target/release/vise-prepare --prepare --server-jar path/to/server.jar
./target/release/vise-prepare sounds      # по желанию
```

> [!TIP]
> Ссылка на `server.jar` лежит в `downloads.server.url` файла `versions/26.3/26.3.json` вашего лаунчера. Если игра в другом месте, добавьте `--minecraft-dir PATH`.

## Запуск

```sh
./target/release/vise-client
```

Все опции: `vise-client --help`.

## Управление

| Клавиша | Действие | Клавиша | Действие |
|---|---|---|---|
| `W A S D` | Движение | `E` | Инвентарь |
| `Space` | Прыжок | `Q` | Выбросить |
| `Shift` | Красться | `1-9` | Хотбар |
| `Ctrl` | Бег | `F1` | Скрыть HUD |
| ЛКМ | Ломать | `F3` | Отладка |
| ПКМ | Ставить / использовать | `Esc` | Пауза |
