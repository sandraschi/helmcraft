# Changelog

All notable changes to `helmcraft` will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] - 2026-11-13

### Changed
- Treasury cash is the closing balance of BuchCraft's bank account (`bank_account`, otherwise 2800). A demo ledger is noted and ignored. `bank_tx` remains movement.

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
