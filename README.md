# HelmCraft 🧭

**Local-first sovereign CEO & CFO Executive Cockpit, Fleet Telemetry Radar, and Board Briefing Engine.**

The "God's Eye View" executive radar aggregating live operational, financial, and compliance state across all 11 craft business applications.

## Features

- **Live Multi-Database Telemetry**: Direct read-only SQLite cross-querying across `billcraft.db`, `spendcraft.db`, `teamcraft.db`, `hirecraft.db`, `gaincraft.db`, `taskcraft.db`, and `assetcraft.db`.
- **Key Executive KPIs**:
  - **Treasury & Runway**: Real-time cash balance, monthly burn, runway months, ARR/MRR, accounts receivable, and overdue dunning amounts.
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
