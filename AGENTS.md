# HelmCraft — AGENTS.md

Local-first sovereign CEO/CFO Executive Cockpit, Fleet Telemetry Radar, and Board Briefing Engine.

## Crates

- `crates/helmcraft-store`: Live multi-database SQLite reader (across all craft apps) and historical snapshot store.
- `crates/helmcraft-core`: Executive briefing synthesizer, cashflow forecaster, board report generator.
- `crates/helmcraft-mcp`: Stdio JSON-RPC FastMCP server & CLI binary (`helmcraft`).
- `crates/helmcraft-app`: Native desktop GUI executable (`helmcraft-app`) via `egui 0.37`.

## Smoke Test

```powershell
.\start.ps1 -Smoke
```
