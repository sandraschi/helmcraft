---
updated: 2026-11-13T08:10:00+01:00
agent: opencode
status: completed
tags: [helmcraft, executive-cockpit, egui, craft-biz-ui, tiers]
---

# Handover - HelmCraft

## What Was Done
- Added the Running Projects page backed by a TaskCraft rollup
  (`running_projects`): per-project active/planned/done/blocked, open
  milestones, nearest due milestone with progress and overdue flag. Overdue
  milestones and blocked projects flow into risks, action items and briefing.
- New `helm_projects` tool (MCP/CLI/palette/menu) and an Overview strip.
- Replaced the static three-column window with the `craft-biz-ui` suite shell:
  menu bar, navigation rail, command palette, help suite, status bar, toasts.
- Nine pages: Overview, Treasury, People, Compliance, Risk & Delivery, Briefing,
  Snapshot history, Data Sources, Settings. Alt+1..9 and the View menu switch.
- Readable Segoe UI type scale (14.5 pt body), dark/light theme, Ctrl+wheel zoom,
  English default with the Deutsch/English switch in the status bar.
- Click-through KPI/action rows, sortable/filterable tables, snapshot comparison
  and delete, Markdown briefing export, snapshot CSV export.
- Store now reads every metric from the local source databases with provenance;
  missing or empty sources read zero and are reported instead of using invented
  fallbacks. Treasury cash is BuchCraft's bank-account closing balance when that
  ledger is real, otherwise the setting or `HELMCRAFT_CASH_CENTS`.
- MCP/CLI expanded from 3 to 11 tools; MCPB manifest updated.
- Tests: store/core/MCP round-trips, coverage (every tool has a UI place), menu
  parity and glyph guard, every view rendered in light and dark.

## Last Concrete Action
- `start.ps1 -Smoke` green; `cargo test --workspace` and
  `cargo clippy --workspace --all-targets --all-features -- -D warnings` green.
- Screenshots verified: english light overview, dark treasury.

## Next Step
- Give the app a first-run demo cash toggle that uses the demo BuchCraft ledger
  balance as an explicitly MOCK runway scenario (labelled DEMO), so a fresh
  machine shows a complete cockpit without pretending the figure is real.
- Add a snapshot scheduler (daily save) and a trend sparkline over history.
