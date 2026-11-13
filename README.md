# HelmCraft 🧭

![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)
![Rust 1.90+](https://img.shields.io/badge/rust-1.90%2B-orange?logo=rust)
![MCP stdio](https://img.shields.io/badge/MCP-stdio-purple)
![ArtCraft harness](https://img.shields.io/badge/ArtCraft-harness-green)

**Local-first sovereign CEO & CFO Executive Cockpit, Fleet Telemetry Radar, and Board Briefing Engine.**

The "God's Eye View" executive radar aggregating live operational, financial, and compliance state across all 11 craft business applications.

## Features

- **Live Multi-Database Telemetry**: Direct read-only SQLite cross-querying across `billcraft.db`, `spendcraft.db`, `teamcraft.db`, `hirecraft.db`, `gaincraft.db`, `taskcraft.db`, and `assetcraft.db`.
- **Key Executive KPIs**:
  - **Treasury & Runway**: Cash is the closing balance of BuchCraft's bank account (default 2800) when that book is real. A demo ledger is ignored. Otherwise the figure is the app setting or `HELMCRAFT_CASH_CENTS`. Monthly burn, runway, ARR/MRR, receivables, and overdue amounts are read from the sibling databases. The sum of `bank_tx` is movement, and it is not the cash figure.
  - **People & Organization**: Active FTE count, open requisitions, active candidate pipeline, pending offers.
  - **Compliance Radar**: Austrian Arbeitszeitgesetz (AZG) statutory workweek and overtime violation tracking.
  - **Customer & Delivery Risk**: Churn risk accounts, customer support SLA breaches, engineering blocked tasks.
- **Board Briefing Generator**: Automatically synthesizes executive summaries, risk radars, and strategic board recommendations.
- **Tri-Modal Interface**:
  - **L1 CLI**: Instant executive status overview and snapshot capture.
  - **L2 MCP**: Stdio FastMCP tools (`helm_status`, `helm_board_briefing`, `helm_cashflow_forecast`).
  - **L3 GUI**: Real-time 3-column executive radar in `egui 0.37` with instant refresh.

## Architecture

- `crates/helmcraft-store`: Live multi-database query aggregator and historical snapshot persistence.
- `crates/helmcraft-core`: Executive briefing synthesizer, cashflow forecaster, board report generator.
- `crates/helmcraft-mcp`: Stdio JSON-RPC MCP server & CLI binary (`helmcraft`).
- `crates/helmcraft-app`: Native desktop GUI executable (`helmcraft-app`).

## Quick Start

```powershell
# Run smoke tests
.\start.ps1 -Smoke

# Launch native desktop app
.\start.ps1

# Run MCP server on stdio
.\start.ps1 -Mcp
```

## License

MIT OR Apache-2.0
