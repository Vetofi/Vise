[← Назад](../../README.ru.md)

# 🧩 Моды

## Создание

```sh
cargo vise-mod new my-mod
cargo vise-mod build
cargo vise-mod run
cargo vise-mod check
```

## Что есть

| | |
|---|---|
| **API** | В духе Fabric: события с фазами, реестры, точки входа |
| **Формат** | Файлы `.vmod`, модпаки, индекс реестра |
| **Шейдеры** | Свои паки на WGSL |

Пример мода: [`mods/ruby.vmod`](../../mods/ruby.vmod)

## Перенос Java-модпака

Крейт [`vise-port-pack`](../../crates/vise-port-pack) импортирует Java-модпак, инвентаризирует и классифицирует моды и переносит данные.
