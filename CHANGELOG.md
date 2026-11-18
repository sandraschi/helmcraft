# Changelog

All notable changes to `helmcraft` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2026-11-13

### Added
- Running Projects page: per-project delivery rollups from TaskCraft (active,
  planned, done, blocked; open milestones; nearest due milestone with progress),
  plus `helm_projects` MCP tool and `projects` CLI verb. Overdue milestones and
  blocked projects feed the risk radar, action items and board briefing.

### Added
- Full Craft Menu Bar shell: File, Edit, View, Cockpit, Window and Help, with keyboard shortcuts and the five-dialog Help suite.
- Nine navigable cockpit pages: Overview, Treasury, People, Compliance, Risk & Delivery, Briefing, Snapshots, Data Sources, Settings.
- Click-through KPI and action-item navigation, command palette (Ctrl+K) over every tool, dark/light theme, Segoe UI type scale and Ctrl+wheel zoom.
- Snapshot history with current-value comparison and delete; Markdown briefing and snapshot CSV export.
- Data-source audit page (path, key table, row count, status) and an explicit treasury cash field.

### Changed
- Treasury cash is the closing balance of BuchCraft's bank account (`bank_account`, otherwise 2800). A demo ledger is noted and ignored. `bank_tx` remains movement.
- Board briefing text and recommendations are derived from the live snapshot instead of fixed copy.

### Fixed
- Open AZG violations now come from `teamcraft.violations` (three were reported while the window showed zero).
- Empty source databases no longer fall back to invented cash, burn, ARR or headcount figures.
- Refresh and briefing no longer read a throwaway in-memory store; the window keeps its configured database.

## [0.1.0] - 2026-11-10

### Added
- Pure-Rust 4-crate workspace architecture (`helmcraft-store`, `helmcraft-core`, `helmcraft-mcp`, `helmcraft-app`).
- Live multi-database telemetry reader cross-querying local SQLite databases (`billcraft.db`, `spendcraft.db`, `teamcraft.db`, `hirecraft.db`, `gaincraft.db`, `taskcraft.db`, `assetcraft.db`).
- Real-time calculations of treasury cash, monthly burn, runway months, ARR/MRR, accounts receivable, and overdue dunning amounts.
- Austrian Arbeitszeitgesetz (AZG) statutory compliance radar monitoring workweek hours and rest period violations across the fleet.
- Automated Board Briefing synthesizer generating executive summaries, risk radars, and strategic recommendations.
- Stdio FastMCP JSON-RPC server and CLI with 3 tools (`helm_status`, `helm_board_briefing`, `helm_cashflow_forecast`).
- Native egui 0.37 desktop workstation (`helmcraft-app`) with 3-column executive radar and `--smoke` verification.
- Central fleet launcher `mcp-central-docs\starts\helmcraft-start.bat`.
