[← Back](../../README.md)

# 🤖 Automation

`vise-ctl` plays the game from a terminal or script.

```sh
vise-ctl launch --world Test --wait
vise-ctl state
vise-ctl help
```

## AI agents (MCP)

`vise-ctl mcp` is an MCP server over stdio. Add it to your MCP client:

```json
{
  "mcpServers": {
    "vise": { "command": "vise-ctl", "args": ["mcp"] }
  }
}
```

It finds the game beside it, or the version the launcher installed.
