[← Назад](../../README.ru.md)

# 🤖 Автоматизация

`vise-ctl` управляет игрой из терминала или скрипта.

```sh
vise-ctl launch --world Test --wait
vise-ctl state
vise-ctl help
```

## ИИ-агенты (MCP)

`vise-ctl mcp` это MCP-сервер по stdio. Добавьте его в MCP-клиент:

```json
{
  "mcpServers": {
    "vise": { "command": "vise-ctl", "args": ["mcp"] }
  }
}
```

Он находит игру рядом с собой или версию, установленную лаунчером.
