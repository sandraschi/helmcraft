//! `helmcraft-app`: executive cockpit window on the shared craft-biz-ui shell.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use craft_biz::bus::Bus;
use craft_biz_ui::app::{self, AppMenus, Frame, NavEntry, SuiteApp};
use craft_biz_ui::help::{AppInfo, Doc};
use craft_biz_ui::menu::{Item, Menu};
use craft_biz_ui::table::{Cell, Col, Table, TableEvent};
use craft_biz_ui::theme::{self, semibold, Tone};
use craft_biz_ui::widgets::{self, Kpi};
use eframe::egui::{self, RichText, ScrollArea, TextEdit};
use helmcraft_core::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const APP_NAME: &str = "HelmCraft";
const V_OVERVIEW: usize = 0;
const V_TREASURY: usize = 1;
const V_PEOPLE: usize = 2;
const V_COMPLIANCE: usize = 3;
const V_RISK: usize = 4;
const V_BRIEFING: usize = 5;
const V_SNAPSHOTS: usize = 6;
const V_SOURCES: usize = 7;
const V_SETTINGS: usize = 8;
const VIEWS: usize = 9;

fn default_db() -> PathBuf {
    if let Ok(p) = std::env::var("HELMCRAFT_DB") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("helmcraft").join("helmcraft.db")
}

fn open(db: &Path) -> Result<HelmApp, String> {
    let store = HelmStore::open(db).map_err(|e| e.to_string())?;
    let bus = Bus::new(Bus::default_root());
    HelmApp::from_store(store, db.to_path_buf(), bus)
}

fn money(cents: i64) -> String {
    format!("EUR {:.2}", cents as f64 / 100.0)
}

fn money_short(cents: i64) -> String {
    let eur = cents as f64 / 100.0;
    if eur.abs() >= 1_000_000.0 {
        format!("EUR {:.2}M", eur / 1_000_000.0)
    } else if eur.abs() >= 1_000.0 {
        format!("EUR {:.1}k", eur / 1_000.0)
    } else {
        format!("EUR {eur:.2}")
    }
}

fn export_default(suffix: &str, ext: &str) -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir());
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M");
    base.join("helmcraft")
        .join(format!("{suffix}-{stamp}.{ext}"))
}

fn route_action(item: &str) -> usize {
    if item.starts_with("[Compliance]") {
        V_COMPLIANCE
    } else if item.starts_with("[Risk]") {
        V_RISK
    } else if item.starts_with("[People]") {
        V_PEOPLE
    } else if item.starts_with("[Treasury]") {
        if item.to_lowercase().contains("set the treasury") {
            V_SETTINGS
        } else {
            V_TREASURY
        }
    } else {
        V_OVERVIEW
    }
}

fn csv_escape(value: &str) -> String {
    if value.contains([';', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

struct HelmApp {
    store: HelmStore,
    db_path: PathBuf,
    bus: Bus,
    snapshot: ExecutiveSnapshot,
    briefing: BoardBriefing,
    forecast: CashflowForecast,
    sources: Vec<SourceHealth>,
    snapshots: Vec<SnapshotRecord>,
    snapshot_total: i64,
    employees: Vec<EmployeeRow>,
    violations: Vec<ViolationRow>,
    at_risk: Vec<AtRiskAccount>,
    blocked: Vec<BlockedIssue>,
    invoices: Vec<InvoiceRow>,
    renewals: Vec<RenewalRow>,
    selected_snapshot: Option<i64>,
    cash_input: String,
    last_export: Option<String>,
}

impl HelmApp {
    fn from_store(store: HelmStore, db_path: PathBuf, bus: Bus) -> Result<HelmApp, String> {
        let mut app = HelmApp {
            store,
            db_path,
            bus,
            snapshot: empty_snapshot(),
            briefing: empty_briefing(),
            forecast: empty_forecast(),
            sources: Vec::new(),
            snapshots: Vec::new(),
            snapshot_total: 0,
            employees: Vec::new(),
            violations: Vec::new(),
            at_risk: Vec::new(),
            blocked: Vec::new(),
            invoices: Vec::new(),
            renewals: Vec::new(),
            selected_snapshot: None,
            cash_input: String::new(),
            last_export: None,
        };
        app.reload()?;
        Ok(app)
    }

    fn cash_from_input(&self) -> Result<f64, String> {
        let raw = self.cash_input.trim().replace(',', ".");
        let value: f64 = raw
            .parse()
            .map_err(|_| "Enter the cash balance as a number, e.g. 1420500.00".to_string())?;
        if !value.is_finite() || value < 0.0 {
            return Err("Cash must be zero or a positive number".into());
        }
        Ok(value)
    }

    fn export_briefing(&self, path: Option<&str>) -> Result<Value, String> {
        let target = path
            .filter(|p| !p.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| export_default("briefing", "md"));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let markdown = self.briefing.to_markdown();
        std::fs::write(&target, markdown.as_bytes()).map_err(|e| e.to_string())?;
        Ok(json!({
            "path": target.display().to_string(),
            "bytes": markdown.len(),
        }))
    }

    fn export_snapshots_csv(&self, path: Option<&str>) -> Result<Value, String> {
        let target = path
            .filter(|p| !p.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| export_default("snapshots", "csv"));
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = String::from(
            "id;saved_at;cash_cents;runway_months;mrr_cents;arr_cents;receivables_cents;overdue_cents;headcount;azg_violations;at_risk;blocked\n",
        );
        for rec in &self.snapshots {
            let s = &rec.snapshot;
            out.push_str(&format!(
                "{};{};{};{:.1};{};{};{};{};{};{};{};{}\n",
                rec.id,
                csv_escape(&rec.saved_at),
                s.treasury.cash_cents,
                s.treasury.runway_months,
                s.treasury.mrr_cents,
                s.treasury.arr_cents,
                s.treasury.receivables_cents,
                s.treasury.overdue_cents,
                s.people.headcount_fte,
                s.people.azg_violations,
                s.ops_risk.at_risk_accounts,
                s.ops_risk.blocked_issues,
            ));
        }
        std::fs::write(&target, out.as_bytes()).map_err(|e| e.to_string())?;
        Ok(json!({
            "path": target.display().to_string(),
            "rows": self.snapshots.len(),
            "bytes": out.len(),
        }))
    }
}

fn empty_snapshot() -> ExecutiveSnapshot {
    ExecutiveSnapshot {
        timestamp: chrono::Utc::now(),
        treasury: TreasuryMetrics {
            cash_cents: 0,
            cash_configured: false,
            burn_monthly_cents: 0,
            burn_payroll_cents: 0,
            burn_expenses_cents: 0,
            runway_months: 0.0,
            mrr_cents: 0,
            arr_cents: 0,
            receivables_cents: 0,
            overdue_cents: 0,
            net_bank_flow_cents: 0,
        },
        people: PeopleMetrics {
            headcount_fte: 0,
            open_requisitions: 0,
            active_candidates: 0,
            pending_offers: 0,
            azg_violations: 0,
            unassigned_assets: 0,
        },
        ops_risk: OpsRiskMetrics {
            active_issues: 0,
            blocked_issues: 0,
            open_tickets: 0,
            sla_breaches: 0,
            at_risk_accounts: 0,
            expiring_contracts_90d: 0,
            csat_nps_score: 0.0,
            csat_nps_available: false,
        },
        action_items: Vec::new(),
        data_notes: Vec::new(),
        sources_read: 0,
        sources_total: 11,
    }
}

fn empty_briefing() -> BoardBriefing {
    BoardBriefing {
        generated_at: chrono::Utc::now(),
        executive_summary: "Telemetry not loaded yet.".into(),
        snapshot: empty_snapshot(),
        top_risks: Vec::new(),
        key_recommendations: Vec::new(),
    }
}

fn empty_forecast() -> CashflowForecast {
    CashflowForecast {
        start_cash_cents: 0,
        cash_configured: false,
        monthly_burn_cents: 0,
        monthly_mrr_cents: 0,
        zero_cash_month: None,
        months: Vec::new(),
        notes: Vec::new(),
    }
}

fn tools_json() -> Vec<Value> {
    vec![
        json!({
            "name": "helm_status",
            "description": "Live executive KPIs from the local craft databases",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "helm_refresh",
            "description": "Re-read every source database and rebuild the cockpit",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "helm_board_briefing",
            "description": "Synthesize the board briefing, risk radar and recommendations",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "helm_cashflow_forecast",
            "description": "Project cash month by month from MRR, burn and open receivables",
            "inputSchema": {"type": "object", "properties": {
                "months": {"type": "integer", "description": "Projection length in months (1-60)", "default": 12}
            }}
        }),
        json!({
            "name": "helm_snapshot_save",
            "description": "Save the current executive snapshot to the history",
            "inputSchema": {"type": "object", "properties": {}}
        }),
        json!({
            "name": "helm_snapshot_list",
            "description": "List saved executive snapshots, newest first",
            "inputSchema": {"type": "object", "properties": {
                "limit": {"type": "integer", "description": "Maximum records (default 20)", "default": 20}
            }}
        }),
        json!({
            "name": "helm_snapshot_delete",
            "description": "Delete one saved snapshot by id",
            "inputSchema": {"type": "object", "properties": {
                "id": {"type": "integer", "description": "Snapshot id"}
            }, "required": ["id"]}
        }),
        json!({
            "name": "helm_export_briefing",
            "description": "Write the board briefing as Markdown",
            "inputSchema": {"type": "object", "properties": {
                "path": {"type": "string", "description": "Destination .md file (default: LOCALAPPDATA/helmcraft)"}
            }}
        }),
        json!({
            "name": "helm_export_snapshots_csv",
            "description": "Write the saved snapshot history as CSV",
            "inputSchema": {"type": "object", "properties": {
                "path": {"type": "string", "description": "Destination .csv file (default: LOCALAPPDATA/helmcraft)"}
            }}
        }),
        json!({
            "name": "helm_set_cash",
            "description": "Set the treasury cash balance used for runway",
            "inputSchema": {"type": "object", "properties": {
                "cash_eur": {"type": "number", "description": "Cash balance in euros, e.g. 1420500.00"}
            }, "required": ["cash_eur"]}
        }),
        json!({
            "name": "helm_sources",
            "description": "Audit every local craft database read by the cockpit",
            "inputSchema": {"type": "object", "properties": {}}
        }),
    ]
}

#[cfg(test)]
const COVERAGE: &[(&str, &str)] = &[
    ("helm_status", "Overview page and Ctrl+K palette"),
    ("helm_refresh", "Overview header, Cockpit menu, F5"),
    ("helm_board_briefing", "Briefing page; Cockpit > Briefing"),
    ("helm_cashflow_forecast", "Treasury page and Cockpit menu"),
    (
        "helm_snapshot_save",
        "Snapshots page, Briefing page, Cockpit menu",
    ),
    ("helm_snapshot_list", "Snapshots page"),
    ("helm_snapshot_delete", "Snapshots page detail"),
    (
        "helm_export_briefing",
        "File > Export Board Briefing, Briefing page",
    ),
    (
        "helm_export_snapshots_csv",
        "Snapshots header, Cockpit menu",
    ),
    (
        "helm_set_cash",
        "Settings page, Treasury page, Cockpit menu",
    ),
    ("helm_sources", "Data Sources page and Cockpit menu"),
];

impl SuiteApp for HelmApp {
    fn name(&self) -> &'static str {
        APP_NAME
    }

    fn id(&self) -> &'static str {
        "helmcraft"
    }

    fn tone(&self) -> Tone {
        Tone::Accent
    }

    fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    fn tools(&self) -> Vec<Value> {
        tools_json()
    }

    fn call(&mut self, tool: &str, args: &Value) -> Result<Value, String> {
        match tool {
            "helm_status" | "helm_refresh" => {
                serde_json::to_value(&self.snapshot).map_err(|e| e.to_string())
            }
            "helm_board_briefing" => {
                serde_json::to_value(&self.briefing).map_err(|e| e.to_string())
            }
            "helm_cashflow_forecast" => {
                let months = args
                    .get("months")
                    .and_then(Value::as_u64)
                    .unwrap_or(12)
                    .min(60) as u32;
                let f = cashflow_forecast(&self.store, months);
                serde_json::to_value(f).map_err(|e| e.to_string())
            }
            "helm_snapshot_save" => {
                let snap = self.store.aggregate_live_fleet();
                let id = self.store.save_snapshot(&snap).map_err(|e| e.to_string())?;
                self.snapshot = snap;
                Ok(json!({"saved": true, "id": id}))
            }
            "helm_snapshot_list" => {
                let limit = args
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .min(200) as usize;
                let rows = self
                    .store
                    .list_snapshots(limit)
                    .map_err(|e| e.to_string())?;
                serde_json::to_value(rows).map_err(|e| e.to_string())
            }
            "helm_snapshot_delete" => {
                let id = args
                    .get("id")
                    .and_then(Value::as_i64)
                    .ok_or("id is required")?;
                let deleted = self.store.delete_snapshot(id).map_err(|e| e.to_string())?;
                if self.selected_snapshot == Some(id) {
                    self.selected_snapshot = None;
                }
                Ok(json!({"deleted": deleted, "id": id}))
            }
            "helm_export_briefing" => {
                let path = args.get("path").and_then(Value::as_str);
                let out = self.export_briefing(path)?;
                self.last_export = out["path"].as_str().map(str::to_string);
                Ok(out)
            }
            "helm_export_snapshots_csv" => {
                let path = args.get("path").and_then(Value::as_str);
                let out = self.export_snapshots_csv(path)?;
                self.last_export = out["path"].as_str().map(str::to_string);
                Ok(out)
            }
            "helm_set_cash" => {
                let eur = args
                    .get("cash_eur")
                    .and_then(Value::as_f64)
                    .ok_or("cash_eur is required")?;
                if !eur.is_finite() || eur < 0.0 {
                    return Err("cash_eur must be zero or positive".into());
                }
                let cents = (eur * 100.0).round() as i64;
                self.store
                    .set_cash_cents(cents)
                    .map_err(|e| e.to_string())?;
                self.cash_input = format!("{eur:.2}");
                Ok(json!({"cash_cents": cents, "cash_eur": eur}))
            }
            "helm_sources" => {
                let health = self.store.source_health();
                serde_json::to_value(health).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown tool: {other}")),
        }
    }

    fn reload(&mut self) -> Result<(), String> {
        self.snapshot = self.store.aggregate_live_fleet();
        self.briefing = generate_board_briefing(&self.store);
        self.forecast = cashflow_forecast(&self.store, 12);
        self.sources = self.store.source_health();
        self.snapshots = self.store.list_snapshots(100).map_err(|e| e.to_string())?;
        self.snapshot_total = self.store.snapshot_count().map_err(|e| e.to_string())?;
        self.employees = self.store.employees().map_err(|e| e.to_string())?;
        self.violations = self.store.violations().map_err(|e| e.to_string())?;
        self.at_risk = self.store.at_risk_accounts().map_err(|e| e.to_string())?;
        self.blocked = self.store.blocked_issues().map_err(|e| e.to_string())?;
        self.invoices = self.store.open_invoices().map_err(|e| e.to_string())?;
        self.renewals = self.store.upcoming_renewals().map_err(|e| e.to_string())?;
        self.cash_input = self
            .store
            .configured_cash_cents()
            .map(|c| format!("{:.2}", c as f64 / 100.0))
            .unwrap_or_default();
        if let Some(id) = self.selected_snapshot {
            if !self.snapshots.iter().any(|s| s.id == id) {
                self.selected_snapshot = None;
            }
        }
        Ok(())
    }

    fn nav(&self, _f: &Frame) -> Vec<NavEntry> {
        self.nav_entries()
    }

    fn view(&mut self, ui: &mut egui::Ui, view: usize, f: &mut Frame) {
        match view {
            V_TREASURY => self.view_treasury(ui, f),
            V_PEOPLE => self.view_people(ui, f),
            V_COMPLIANCE => self.view_compliance(ui, f),
            V_RISK => self.view_risk(ui, f),
            V_BRIEFING => self.view_briefing(ui, f),
            V_SNAPSHOTS => self.view_snapshots(ui, f),
            V_SOURCES => self.view_sources(ui, f),
            V_SETTINGS => self.view_settings(ui, f),
            _ => self.view_overview(ui, f),
        }
    }

    fn top_actions(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        if ui
            .button("Save snapshot")
            .on_hover_text("Write the current KPIs to snapshot history")
            .clicked()
        {
            f.run("Snapshot saved", "helm_snapshot_save", json!({}));
        }
        if ui
            .button("Briefing")
            .on_hover_text("Open the board briefing")
            .clicked()
        {
            f.goto(V_BRIEFING);
        }
    }

    fn bus(&self) -> &Bus {
        &self.bus
    }

    fn wiring(
        &self,
    ) -> (
        Vec<(&'static str, &'static str)>,
        Vec<(&'static str, &'static str)>,
    ) {
        (Vec::new(), Vec::new())
    }

    fn status_info(&self) -> String {
        let ok = self
            .sources
            .iter()
            .filter(|s| s.status == "ok" || s.status == "empty")
            .count();
        format!(
            "{} / {} sources | {} snapshots | {}",
            ok,
            self.sources.len(),
            self.snapshot_total,
            self.db_path.display()
        )
    }

    fn smoke_tool(&self) -> &'static str {
        "helm_status"
    }

    fn menus(&self, _f: &Frame) -> AppMenus {
        let export_path = export_default("briefing", "md").display().to_string();
        let csv_path = export_default("snapshots", "csv").display().to_string();
        AppMenus {
            file_new: vec![Item::run(
                "Save Executive Snapshot",
                "helm_snapshot_save",
                json!({}),
            )],
            save_as: None,
            export: vec![
                Item::form(
                    "Export Board Briefing...",
                    "helm_export_briefing",
                    &[("path", export_path.as_str())],
                ),
                Item::form(
                    "Export Snapshot History CSV...",
                    "helm_export_snapshots_csv",
                    &[("path", csv_path.as_str())],
                ),
            ],
            view: Vec::new(),
            middle: vec![Menu::new(
                "Cockpit",
                vec![
                    Item::run("Refresh Telemetry", "helm_refresh", json!({})),
                    Item::sep(),
                    Item::run("Save Snapshot", "helm_snapshot_save", json!({})),
                    Item::run(
                        "Cashflow Forecast (12 months)",
                        "helm_cashflow_forecast",
                        json!({"months": 12}),
                    ),
                    Item::run("Audit Data Sources", "helm_sources", json!({})),
                    Item::sep(),
                    Item::form("Set Treasury Cash...", "helm_set_cash", &[]),
                    Item::form(
                        "Export Board Briefing...",
                        "helm_export_briefing",
                        &[("path", export_path.as_str())],
                    ),
                ],
            )],
            window: Vec::new(),
        }
    }

    fn info(&self) -> AppInfo {
        AppInfo {
            name: APP_NAME.into(),
            version: self.version().into(),
            description: "Sovereign CEO/CFO cockpit over the local craft databases".into(),
            license: "MIT OR Apache-2.0".into(),
            repository: "https://github.com/sandraschi/helmcraft".into(),
            commit: option_env!("GIT_COMMIT").unwrap_or("unknown").into(),
            built: option_env!("BUILD_DATE").unwrap_or("").into(),
            safety: "Pure Rust; every craft database is opened read-only.".into(),
            third_party: Vec::new(),
        }
    }

    fn docs(&self) -> Vec<Doc> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        vec![
            Doc::embedded(
                "README",
                root.join("README.md"),
                include_str!("../../../README.md"),
            ),
            Doc::embedded(
                "PRD (tiers)",
                root.join("PRD.md"),
                include_str!("../../../PRD.md"),
            ),
            Doc::embedded(
                "Changelog",
                root.join("CHANGELOG.md"),
                include_str!("../../../CHANGELOG.md"),
            ),
        ]
    }

    fn extra_shortcuts(&self, _de: bool) -> Vec<craft_biz_ui::help::ShortcutRow> {
        let mut rows = vec![(
            "HelmCraft".to_string(),
            "Alt+1..9".to_string(),
            "Switch cockpit page".to_string(),
        )];
        rows.push((
            "HelmCraft".to_string(),
            "Ctrl+K".to_string(),
            "Command palette (all tools)".to_string(),
        ));
        rows
    }

    fn copy_text(&self, view: usize) -> Option<String> {
        if view == V_BRIEFING {
            Some(self.briefing.to_markdown())
        } else {
            None
        }
    }

    fn close(&mut self) {
        self.selected_snapshot = None;
    }

    fn select(&mut self, key: &str) {
        if let Ok(id) = key.parse::<i64>() {
            self.selected_snapshot = Some(id);
        }
    }

    fn preferences_view(&self) -> Option<usize> {
        Some(V_SETTINGS)
    }
}

impl HelmApp {
    fn nav_entries(&self) -> Vec<NavEntry> {
        let entries = vec![
            NavEntry::new("\u{25C6}", "Overview").group("Cockpit"),
            NavEntry::new("\u{20AC}", "Treasury").badge(
                self.snapshot.treasury.overdue_cents.max(0) as usize,
                Tone::Warn,
            ),
            NavEntry::new("\u{25CF}", "People"),
            NavEntry::new("\u{00A7}", "Compliance")
                .badge(self.snapshot.people.azg_violations, Tone::Danger),
            NavEntry::new("\u{25B2}", "Risk & Delivery").badge(
                self.snapshot.ops_risk.at_risk_accounts + self.snapshot.ops_risk.blocked_issues,
                Tone::Warn,
            ),
            NavEntry::new("\u{2261}", "Briefing").group("Board"),
            NavEntry::new("\u{25A3}", "Snapshots")
                .badge(self.snapshot_total.max(0) as usize, Tone::Neutral),
            NavEntry::new("\u{25A6}", "Data Sources").group("System"),
            NavEntry::new("\u{2699}", "Settings"),
        ];
        debug_assert_eq!(entries.len(), VIEWS);
        entries
    }

    fn view_overview(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        let snap = self.snapshot.clone();
        let t = &snap.treasury;
        widgets::page_header(
            ui,
            "Executive overview",
            "Live read-only telemetry from the local craft databases.",
            |ui| {
                if widgets::primary(ui, "Refresh telemetry").clicked() {
                    f.run("Telemetry refreshed", "helm_refresh", json!({}));
                }
                if ui.button("Save snapshot").clicked() {
                    f.run("Snapshot saved", "helm_snapshot_save", json!({}));
                }
            },
        );

        let cash_value = if t.cash_configured {
            money_short(t.cash_cents)
        } else {
            "Not set".to_string()
        };
        widgets::kpi_row(
            ui,
            &[
                Kpi::new("Treasury cash", cash_value).note(
                    if t.cash_configured {
                        format!("Runway {:.1} months", t.runway_months)
                    } else {
                        "Set it in Settings".to_string()
                    },
                    if t.cash_configured {
                        Tone::Ok
                    } else {
                        Tone::Warn
                    },
                ),
                Kpi::new("Monthly burn", money_short(t.burn_monthly_cents))
                    .note("Payroll + expenses", Tone::Neutral),
                Kpi::new(
                    "MRR / ARR",
                    format!(
                        "{} / {}",
                        money_short(t.mrr_cents),
                        money_short(t.arr_cents)
                    ),
                )
                .note("GainCraft active ARR", Tone::Info),
                Kpi::new("AZG violations", snap.people.azg_violations.to_string()).note(
                    if snap.people.azg_violations > 0 {
                        "Uncleared - needs action"
                    } else {
                        "No open violations"
                    },
                    if snap.people.azg_violations > 0 {
                        Tone::Danger
                    } else {
                        Tone::Ok
                    },
                ),
            ],
        );
        ui.add_space(8.0);
        widgets::kpi_row(
            ui,
            &[
                Kpi::new("Headcount", format!("{} FTE", snap.people.headcount_fte)).note(
                    format!("{} open requisitions", snap.people.open_requisitions),
                    Tone::Neutral,
                ),
                Kpi::new("Overdue", money_short(t.overdue_cents)).note(
                    format!("{} open invoices", self.invoices.len()),
                    if t.overdue_cents > 0 {
                        Tone::Danger
                    } else {
                        Tone::Ok
                    },
                ),
                Kpi::new(
                    "At-risk accounts",
                    snap.ops_risk.at_risk_accounts.to_string(),
                )
                .note(
                    "Red churn band",
                    if snap.ops_risk.at_risk_accounts > 0 {
                        Tone::Warn
                    } else {
                        Tone::Ok
                    },
                ),
                Kpi::new("Blocked issues", snap.ops_risk.blocked_issues.to_string()).note(
                    format!("{} active issues", snap.ops_risk.active_issues),
                    if snap.ops_risk.blocked_issues > 0 {
                        Tone::Warn
                    } else {
                        Tone::Ok
                    },
                ),
            ],
        );

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Treasury details ->").clicked() {
                f.goto(V_TREASURY);
            }
            if ui.button("People ->").clicked() {
                f.goto(V_PEOPLE);
            }
            if ui.button("Compliance ->").clicked() {
                f.goto(V_COMPLIANCE);
            }
            if ui.button("Risk & Delivery ->").clicked() {
                f.goto(V_RISK);
            }
            if ui.button("Board briefing ->").clicked() {
                f.goto(V_BRIEFING);
            }
        });

        ui.add_space(10.0);
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                widgets::section(ui, "Executive action items", Some(snap.action_items.len()));
                widgets::card(ui, |ui| {
                    if snap.action_items.is_empty() {
                        widgets::empty_state(
                            ui,
                            "All systems nominal",
                            "No escalations detected across connected sources.",
                        );
                    } else {
                        for item in &snap.action_items {
                            let route = route_action(item);
                            if ui
                                .add(
                                    egui::Button::new(RichText::new(item).size(13.5))
                                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                                )
                                .on_hover_text("Open the page that owns this metric")
                                .clicked()
                            {
                                f.goto(route);
                            }
                        }
                    }
                });
            });
            cols[1].vertical(|ui| {
                let ok = self
                    .sources
                    .iter()
                    .filter(|s| s.status == "ok" || s.status == "empty")
                    .count();
                widgets::section(ui, "Telemetry", Some(self.sources.len()));
                widgets::card(ui, |ui| {
                    ui.label(format!(
                        "{} of {} craft databases connected read-only.",
                        ok,
                        self.sources.len()
                    ));
                    for note in &snap.data_notes {
                        ui.label(
                            RichText::new(format!("- {note}"))
                                .color(theme::tone_colors(Tone::Warn, ui.visuals().dark_mode).0),
                        );
                    }
                    if ui.button("Audit data sources ->").clicked() {
                        f.goto(V_SOURCES);
                    }
                });
                widgets::section(ui, "Board briefing preview", None);
                widgets::card(ui, |ui| {
                    ui.label(RichText::new(&self.briefing.executive_summary).size(13.5));
                    if ui.button("Open briefing ->").clicked() {
                        f.goto(V_BRIEFING);
                    }
                });
            });
        });
    }

    fn view_treasury(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Treasury & runway",
            "Cash, burn, receivables and the forward projection.",
            |ui| {
                if ui.button("Save snapshot").clicked() {
                    f.run("Snapshot saved", "helm_snapshot_save", json!({}));
                }
                if ui.button("Forecast to result").clicked() {
                    f.run_show("helm_cashflow_forecast", json!({"months": 12}));
                }
            },
        );
        let t = self.snapshot.treasury.clone();
        widgets::kpi_row(
            ui,
            &[
                Kpi::new(
                    "Treasury cash",
                    if t.cash_configured {
                        money(t.cash_cents)
                    } else {
                        "Not configured".into()
                    },
                )
                .note(
                    if t.cash_configured {
                        format!("Runway {:.1} months", t.runway_months)
                    } else {
                        "Runway unavailable".into()
                    },
                    if t.cash_configured {
                        Tone::Ok
                    } else {
                        Tone::Warn
                    },
                ),
                Kpi::new("Monthly burn", money(t.burn_monthly_cents))
                    .note("Payroll + expenses", Tone::Neutral),
                Kpi::new("MRR", money(t.mrr_cents)).note("Normalized monthly", Tone::Info),
                Kpi::new("Receivables", money(t.receivables_cents)).note(
                    format!("{} overdue", money(t.overdue_cents)),
                    if t.overdue_cents > 0 {
                        Tone::Danger
                    } else {
                        Tone::Ok
                    },
                ),
            ],
        );

        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new("Treasury cash balance").family(semibold()));
            ui.label(
                RichText::new(
                    "A real BuchCraft book supplies this from its bank account (default 2800). A demo ledger is ignored. Otherwise set it here. HELMCRAFT_CASH_CENTS is the same fallback.",
                )
                .color(theme::current(ui.ctx()).text_weak),
            );
            ui.horizontal(|ui| {
                ui.label("Cash (EUR):");
                ui.add(TextEdit::singleline(&mut self.cash_input).desired_width(160.0));
                if widgets::primary(ui, "Save cash").clicked() {
                    match self.cash_from_input() {
                        Ok(v) => f.run(
                            format!("Cash set to EUR {v:.2}"),
                            "helm_set_cash",
                            json!({"cash_eur": v}),
                        ),
                        Err(e) => f.toast(e, Tone::Danger),
                    }
                }
            });
            if let Some(path) = &self.last_export {
                ui.label(
                    RichText::new(format!("Last export: {path}"))
                        .size(12.0)
                        .color(theme::current(ui.ctx()).text_weak),
                );
            }
        });

        ui.add_space(8.0);
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                widgets::section(ui, "Burn breakdown", None);
                widgets::card(ui, |ui| {
                    let total = t.burn_monthly_cents.max(1);
                    ui.label(format!("Payroll: {}", money(t.burn_payroll_cents)));
                    widgets::meter(
                        ui,
                        t.burn_payroll_cents as f32 / total as f32,
                        260.0,
                        Tone::Accent,
                    );
                    ui.label(format!("Expenses: {}", money(t.burn_expenses_cents)));
                    widgets::meter(
                        ui,
                        t.burn_expenses_cents as f32 / total as f32,
                        260.0,
                        Tone::Warn,
                    );
                    ui.separator();
                    ui.label(format!(
                        "Imported bank net movement: {}",
                        money(t.net_bank_flow_cents)
                    ));
                });
            });
            cols[1].vertical(|ui| {
                widgets::section(ui, "12-month cash projection", None);
                widgets::card(ui, |ui| {
                    if self.forecast.months.is_empty() {
                        widgets::empty_state(ui, "No projection", "Refresh telemetry.");
                    } else {
                        let values: Vec<f32> = self
                            .forecast
                            .months
                            .iter()
                            .map(|m| m.cash_cents as f32)
                            .collect();
                        widgets::sparkline(
                            ui,
                            &values,
                            egui::vec2(ui.available_width().min(360.0), 54.0),
                            if self.forecast.zero_cash_month.is_some() {
                                Tone::Danger
                            } else {
                                Tone::Ok
                            },
                        );
                        match self.forecast.zero_cash_month {
                            Some(m) => ui.label(
                                RichText::new(format!("Cash reaches zero in month {m}.")).color(
                                    theme::tone_colors(Tone::Danger, ui.visuals().dark_mode).0,
                                ),
                            ),
                            None => ui.label(
                                RichText::new("Cash stays positive for the whole horizon.")
                                    .color(theme::tone_colors(Tone::Ok, ui.visuals().dark_mode).0),
                            ),
                        };
                        for note in &self.forecast.notes {
                            ui.label(
                                RichText::new(format!("- {note}"))
                                    .size(12.0)
                                    .color(theme::current(ui.ctx()).text_weak),
                            );
                        }
                    }
                });
            });
        });

        ui.add_space(10.0);
        widgets::section(ui, "Open invoices", Some(self.invoices.len()));
        let cols = [
            Col::new("Customer", 3.0),
            Col::new("Due", 1.2),
            Col::num("Gross", 1.6),
            Col::new("Status", 1.0),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .invoices
            .iter()
            .enumerate()
            .map(|(i, inv)| {
                (
                    format!("inv-{i}"),
                    vec![
                        Cell::Strong(inv.customer.clone()),
                        Cell::Mono(inv.due_date.clone()),
                        Cell::Money(money(inv.gross_cents)),
                        Cell::Pill(
                            inv.status.clone(),
                            if inv.status == "overdue" {
                                Tone::Danger
                            } else {
                                Tone::Warn
                            },
                        ),
                    ],
                )
            })
            .collect();
        if rows.is_empty() {
            widgets::empty_state(
                ui,
                "No open invoices",
                "BillCraft has no issued or overdue invoices.",
            );
        } else {
            let _ = Table::new("helm_invoices", &cols, &rows).show(ui);
        }

        ui.add_space(10.0);
        widgets::section(ui, "Forecast months", Some(self.forecast.months.len()));
        let cols = [
            Col::num("Month", 0.8),
            Col::num("Inflow", 1.5),
            Col::num("Outflow", 1.5),
            Col::num("Net", 1.5),
            Col::num("Cash", 1.5),
            Col::new("State", 1.0),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .forecast
            .months
            .iter()
            .map(|m| {
                (
                    format!("m{}", m.month),
                    vec![
                        Cell::Text(m.month.to_string()),
                        Cell::Money(money(m.inflow_cents)),
                        Cell::Money(money(m.outflow_cents)),
                        Cell::Money(money(m.net_cents)),
                        Cell::Money(money(m.cash_cents)),
                        Cell::Pill(
                            if m.insolvent { "negative" } else { "positive" }.into(),
                            if m.insolvent { Tone::Danger } else { Tone::Ok },
                        ),
                    ],
                )
            })
            .collect();
        if !rows.is_empty() {
            let _ = Table::new("helm_forecast", &cols, &rows)
                .filter_box(false)
                .show(ui);
        }
    }

    fn view_people(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "People & organization",
            "Headcount, recruiting funnel and asset assignment.",
            |ui| {
                if ui.button("Refresh").clicked() {
                    f.run("Telemetry refreshed", "helm_refresh", json!({}));
                }
            },
        );
        let p = self.snapshot.people.clone();
        widgets::kpi_row(
            ui,
            &[
                Kpi::new("Active headcount", format!("{} FTE", p.headcount_fte))
                    .note("TeamCraft employees", Tone::Info),
                Kpi::new("Open requisitions", p.open_requisitions.to_string())
                    .note("HireCraft", Tone::Accent),
                Kpi::new("Candidates", p.active_candidates.to_string())
                    .note("Applied / screening / interview", Tone::Neutral),
                Kpi::new("Pending offers", p.pending_offers.to_string()).note(
                    "Stage = offer",
                    if p.pending_offers > 0 {
                        Tone::Warn
                    } else {
                        Tone::Ok
                    },
                ),
            ],
        );
        ui.add_space(10.0);
        widgets::section(ui, "Employees", Some(self.employees.len()));
        let cols = [
            Col::new("Code", 0.8),
            Col::new("Name", 3.0),
            Col::num("Weekly hours", 1.2),
            Col::num("Monthly gross", 1.5),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .employees
            .iter()
            .enumerate()
            .map(|(i, e)| {
                (
                    format!("emp-{i}"),
                    vec![
                        Cell::Mono(e.code.clone()),
                        Cell::Strong(e.name.clone()),
                        Cell::Text(format!("{:.1}", e.weekly_hours)),
                        Cell::Money(money(e.monthly_gross_cents)),
                    ],
                )
            })
            .collect();
        if rows.is_empty() {
            widgets::empty_state(ui, "No employees", "TeamCraft has no active employees.");
        } else {
            let _ = Table::new("helm_employees", &cols, &rows).show(ui);
        }
        ui.add_space(10.0);
        widgets::card(ui, |ui| {
            ui.label(RichText::new("Assets").family(semibold()));
            ui.label(format!(
                "{} device(s) are not assigned to a person (AssetCraft).",
                p.unassigned_assets
            ));
        });
    }

    fn view_compliance(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Compliance radar (AZG)",
            "Uncleared Austrian Arbeitszeitgesetz violations from TeamCraft.",
            |ui| {
                if ui.button("Refresh").clicked() {
                    f.run("Telemetry refreshed", "helm_refresh", json!({}));
                }
            },
        );
        let total = self.snapshot.people.azg_violations;
        let severe = self
            .violations
            .iter()
            .filter(|v| v.severity == "violation")
            .count();
        widgets::kpi_row(
            ui,
            &[
                Kpi::new("Uncleared violations", total.to_string()).note(
                    if total > 0 {
                        "Action required"
                    } else {
                        "Compliant"
                    },
                    if total > 0 { Tone::Danger } else { Tone::Ok },
                ),
                Kpi::new("Severity = violation", severe.to_string())
                    .note("Warnings are reported too", Tone::Warn),
                Kpi::new(
                    "Employees at risk",
                    self.violations
                        .iter()
                        .map(|v| v.employee.as_str())
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        .to_string(),
                )
                .note("Distinct employees", Tone::Neutral),
            ],
        );
        ui.add_space(10.0);
        widgets::card(ui, |ui| {
            ui.label(
                RichText::new(
                    "AZG: daily rest must be at least 11 hours; daily working time above 10 hours and missing breaks are violations. Clearing a violation in TeamCraft removes it here.",
                )
                .color(theme::current(ui.ctx()).text_weak),
            );
        });
        ui.add_space(10.0);
        widgets::section(ui, "Open violations", Some(self.violations.len()));
        let cols = [
            Col::new("Employee", 2.4),
            Col::new("Date", 1.2),
            Col::new("Kind", 2.6),
            Col::new("Severity", 1.0),
            Col::num("Minutes", 0.8),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .violations
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    format!("vio-{i}"),
                    vec![
                        Cell::Strong(v.employee.clone()),
                        Cell::Mono(v.date.clone()),
                        Cell::Text(v.kind.replace('_', " ")),
                        Cell::Pill(
                            v.severity.clone(),
                            if v.severity == "violation" {
                                Tone::Danger
                            } else {
                                Tone::Warn
                            },
                        ),
                        Cell::Text(v.minutes.to_string()),
                    ],
                )
            })
            .collect();
        if rows.is_empty() {
            widgets::empty_state(
                ui,
                "No open violations",
                "TeamCraft reports full AZG compliance.",
            );
        } else {
            let _ = Table::new("helm_violations", &cols, &rows).show(ui);
        }
    }

    fn view_risk(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Risk & delivery",
            "Churn risk, blocked work, renewals and support load.",
            |ui| {
                if ui.button("Refresh").clicked() {
                    f.run("Telemetry refreshed", "helm_refresh", json!({}));
                }
            },
        );
        let r = self.snapshot.ops_risk.clone();
        widgets::kpi_row(
            ui,
            &[
                Kpi::new("At-risk accounts", r.at_risk_accounts.to_string()).note(
                    "Red churn band",
                    if r.at_risk_accounts > 0 {
                        Tone::Danger
                    } else {
                        Tone::Ok
                    },
                ),
                Kpi::new("Blocked issues", r.blocked_issues.to_string()).note(
                    "Open and blocked",
                    if r.blocked_issues > 0 {
                        Tone::Warn
                    } else {
                        Tone::Ok
                    },
                ),
                Kpi::new("Open tickets", r.open_tickets.to_string())
                    .note(format!("{} SLA breach(es)", r.sla_breaches), Tone::Neutral),
                Kpi::new("Renewals (90d)", r.expiring_contracts_90d.to_string()).note(
                    "Contracts / terms",
                    if r.expiring_contracts_90d > 0 {
                        Tone::Warn
                    } else {
                        Tone::Ok
                    },
                ),
            ],
        );
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                widgets::section(ui, "At-risk accounts", Some(self.at_risk.len()));
                let table_cols = [
                    Col::new("Account", 3.0),
                    Col::num("ARR", 1.4),
                    Col::num("Score", 0.8),
                    Col::new("Band", 0.8),
                ];
                let rows: Vec<(String, Vec<Cell>)> = self
                    .at_risk
                    .iter()
                    .map(|a| {
                        (
                            a.key.clone(),
                            vec![
                                Cell::Strong(a.name.clone()),
                                Cell::Money(money(a.arr_cents)),
                                Cell::Text(a.score.to_string()),
                                Cell::Pill(
                                    a.band.clone(),
                                    if a.band == "red" {
                                        Tone::Danger
                                    } else {
                                        Tone::Warn
                                    },
                                ),
                            ],
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    widgets::empty_state(ui, "No red accounts", "GainCraft health is green.");
                } else {
                    let _ = Table::new("helm_at_risk", &table_cols, &rows)
                        .filter_box(false)
                        .show(ui);
                }
            });
            cols[1].vertical(|ui| {
                widgets::section(ui, "Blocked issues", Some(self.blocked.len()));
                let table_cols = [
                    Col::new("#", 0.5),
                    Col::new("Issue", 3.2),
                    Col::new("Status", 1.0),
                    Col::new("Priority", 0.9),
                ];
                let rows: Vec<(String, Vec<Cell>)> = self
                    .blocked
                    .iter()
                    .map(|b| {
                        (
                            b.number.to_string(),
                            vec![
                                Cell::Mono(b.number.to_string()),
                                Cell::Strong(b.title.clone()),
                                Cell::Pill(b.status.replace('_', " "), Tone::Warn),
                                Cell::Pill(b.priority.clone(), Tone::Danger),
                            ],
                        )
                    })
                    .collect();
                if rows.is_empty() {
                    widgets::empty_state(ui, "Nothing blocked", "TaskCraft has no blocked work.");
                } else {
                    let _ = Table::new("helm_blocked", &table_cols, &rows)
                        .filter_box(false)
                        .show(ui);
                }
            });
        });
        ui.add_space(10.0);
        widgets::section(ui, "Renewals within 90 days", Some(self.renewals.len()));
        let cols = [
            Col::new("Account", 3.0),
            Col::num("ARR", 1.5),
            Col::new("Term end", 1.2),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .renewals
            .iter()
            .map(|x| {
                (
                    x.key.clone(),
                    vec![
                        Cell::Strong(x.name.clone()),
                        Cell::Money(money(x.arr_cents)),
                        Cell::Mono(x.term_end.clone()),
                    ],
                )
            })
            .collect();
        if rows.is_empty() {
            widgets::empty_state(ui, "No renewals due", "No terms end in the next 90 days.");
        } else {
            let _ = Table::new("helm_renewals", &cols, &rows).show(ui);
        }
    }

    fn view_briefing(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Board briefing",
            "Synthesized from the current snapshot; export it as Markdown.",
            |ui| {
                if widgets::primary(ui, "Regenerate").clicked() {
                    f.run("Briefing regenerated", "helm_board_briefing", json!({}));
                }
                if ui.button("Save snapshot").clicked() {
                    f.run("Snapshot saved", "helm_snapshot_save", json!({}));
                }
                if ui.button("Export Markdown...").clicked() {
                    let path = export_default("briefing", "md").display().to_string();
                    f.form("helm_export_briefing", &[("path", path.as_str())]);
                }
            },
        );
        widgets::card(ui, |ui| {
            ui.label(RichText::new("Executive summary").family(semibold()));
            ui.label(RichText::new(&self.briefing.executive_summary).size(14.5));
            ui.label(
                RichText::new(format!(
                    "Generated {} | {} of {} sources",
                    self.briefing.generated_at.format("%Y-%m-%d %H:%M UTC"),
                    self.briefing.snapshot.sources_read,
                    self.briefing.snapshot.sources_total
                ))
                .size(12.0)
                .color(theme::current(ui.ctx()).text_weak),
            );
        });
        ui.add_space(8.0);
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                widgets::section(ui, "Top risks", Some(self.briefing.top_risks.len()));
                widgets::card(ui, |ui| {
                    if self.briefing.top_risks.is_empty() {
                        ui.label("No risks detected.");
                    } else {
                        for risk in &self.briefing.top_risks {
                            ui.label(format!("- {risk}"));
                        }
                    }
                });
            });
            cols[1].vertical(|ui| {
                widgets::section(
                    ui,
                    "Recommendations",
                    Some(self.briefing.key_recommendations.len()),
                );
                widgets::card(ui, |ui| {
                    for (i, rec) in self.briefing.key_recommendations.iter().enumerate() {
                        ui.label(format!("{}. {rec}", i + 1));
                    }
                });
            });
        });
        ui.add_space(8.0);
        egui::CollapsingHeader::new("Markdown preview")
            .default_open(false)
            .show(ui, |ui| {
                ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                    let mut text = self.briefing.to_markdown();
                    ui.add(
                        TextEdit::multiline(&mut text)
                            .font(egui::TextStyle::Monospace)
                            .desired_width(f32::INFINITY)
                            .interactive(false),
                    );
                });
            });
    }

    fn view_snapshots(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Snapshot history",
            "Saved executive snapshots, with current-value comparison.",
            |ui| {
                if widgets::primary(ui, "Save snapshot").clicked() {
                    f.run("Snapshot saved", "helm_snapshot_save", json!({}));
                }
                if ui.button("Export CSV...").clicked() {
                    let path = export_default("snapshots", "csv").display().to_string();
                    f.form("helm_export_snapshots_csv", &[("path", path.as_str())]);
                }
            },
        );
        let cols = [
            Col::num("Id", 0.6),
            Col::new("Saved", 2.2),
            Col::num("Cash", 1.5),
            Col::num("Runway", 1.0),
            Col::num("ARR", 1.4),
            Col::num("Headcount", 1.0),
            Col::num("Overdue", 1.3),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .snapshots
            .iter()
            .map(|rec| {
                let s = &rec.snapshot;
                (
                    rec.id.to_string(),
                    vec![
                        Cell::Mono(rec.id.to_string()),
                        Cell::Mono(rec.saved_at.clone()),
                        Cell::Money(if s.treasury.cash_configured {
                            money(s.treasury.cash_cents)
                        } else {
                            "not set".into()
                        }),
                        Cell::Text(format!("{:.1}", s.treasury.runway_months)),
                        Cell::Money(money(s.treasury.arr_cents)),
                        Cell::Text(s.people.headcount_fte.to_string()),
                        Cell::Money(money(s.treasury.overdue_cents)),
                    ],
                )
            })
            .collect();
        let selected = self.selected_snapshot.map(|id| id.to_string());
        if rows.is_empty() {
            widgets::empty_state(
                ui,
                "No snapshots yet",
                "Save one from this page or File > Save Executive Snapshot.",
            );
        } else {
            let events = Table::new("helm_snapshots", &cols, &rows)
                .selected(selected.as_deref())
                .show(ui);
            for event in events {
                match event {
                    TableEvent::Select(id) | TableEvent::Open(id) => {
                        self.selected_snapshot = id.parse::<i64>().ok();
                    }
                }
            }
        }

        if let Some(id) = self.selected_snapshot {
            if let Some(rec) = self.snapshots.iter().find(|r| r.id == id).cloned() {
                ui.add_space(10.0);
                let old = &rec.snapshot;
                let now = &self.snapshot;
                widgets::section(ui, &format!("Snapshot #{id} vs current"), None);
                widgets::card(ui, |ui| {
                    ui.label(
                        RichText::new(format!("Saved {}", rec.saved_at))
                            .color(theme::current(ui.ctx()).text_weak),
                    );
                    let mut delta = |label: &str, a: i64, b: i64| {
                        ui.label(format!("{label}: {a} -> {b} ({:+})", b - a));
                    };
                    delta(
                        "Cash (cents)",
                        old.treasury.cash_cents,
                        now.treasury.cash_cents,
                    );
                    delta(
                        "Headcount",
                        old.people.headcount_fte as i64,
                        now.people.headcount_fte as i64,
                    );
                    delta(
                        "AZG violations",
                        old.people.azg_violations as i64,
                        now.people.azg_violations as i64,
                    );
                    delta(
                        "Overdue (cents)",
                        old.treasury.overdue_cents,
                        now.treasury.overdue_cents,
                    );
                    ui.label(format!(
                        "Runway: {:.1} -> {:.1} months",
                        old.treasury.runway_months, now.treasury.runway_months
                    ));
                    ui.horizontal(|ui| {
                        if widgets::danger(ui, "Delete snapshot").clicked() {
                            f.run_confirm(
                                format!("Delete snapshot #{id}? This cannot be undone."),
                                "Snapshot deleted",
                                "helm_snapshot_delete",
                                json!({"id": id}),
                            );
                        }
                    });
                });
            }
        }
    }

    fn view_sources(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Data sources",
            "Every craft database is opened read-only; missing sources are reported, not faked.",
            |ui| {
                if widgets::primary(ui, "Re-audit").clicked() {
                    f.run("Sources audited", "helm_sources", json!({}));
                }
            },
        );
        let ok = self
            .sources
            .iter()
            .filter(|s| s.status == "ok" || s.status == "empty")
            .count();
        widgets::kpi_row(
            ui,
            &[
                Kpi::new("Connected", format!("{ok} / {}", self.sources.len())).note(
                    "Read-only",
                    if ok == self.sources.len() {
                        Tone::Ok
                    } else {
                        Tone::Warn
                    },
                ),
                Kpi::new(
                    "With rows",
                    self.sources
                        .iter()
                        .filter(|s| s.rows > 0)
                        .count()
                        .to_string(),
                )
                .note("Non-empty key tables", Tone::Info),
                Kpi::new(
                    "Missing",
                    self.sources
                        .iter()
                        .filter(|s| s.status == "missing")
                        .count()
                        .to_string(),
                )
                .note("Source database absent", Tone::Neutral),
            ],
        );
        ui.add_space(10.0);
        let cols = [
            Col::new("App", 1.3),
            Col::new("Table", 1.6),
            Col::new("Status", 1.0),
            Col::num("Rows", 0.8),
            Col::new("Path", 3.4),
        ];
        let rows: Vec<(String, Vec<Cell>)> = self
            .sources
            .iter()
            .map(|s| {
                (
                    s.app.clone(),
                    vec![
                        Cell::Strong(s.app.clone()),
                        Cell::Mono(s.table.clone()),
                        Cell::Pill(
                            s.status.clone(),
                            match s.status.as_str() {
                                "ok" => Tone::Ok,
                                "empty" => Tone::Info,
                                "missing" => Tone::Warn,
                                _ => Tone::Danger,
                            },
                        ),
                        Cell::Text(s.rows.to_string()),
                        Cell::Mono(s.path.clone()),
                    ],
                )
            })
            .collect();
        let _ = Table::new("helm_sources", &cols, &rows).show(ui);
        ui.add_space(8.0);
        widgets::card(ui, |ui| {
            ui.label(
                RichText::new(format!("Data root: {}", data_root().display()))
                    .color(theme::current(ui.ctx()).text_weak),
            );
            ui.label(
                RichText::new(
                    "Override with HELMCRAFT_DATA_ROOT; the cockpit never writes to these files.",
                )
                .size(12.0)
                .color(theme::current(ui.ctx()).text_weak),
            );
        });
    }

    fn view_settings(&mut self, ui: &mut egui::Ui, f: &mut Frame) {
        widgets::page_header(
            ui,
            "Settings",
            "Treasury input, storage locations and appearance controls.",
            |ui| {
                if ui.button("Refresh").clicked() {
                    f.run("Telemetry refreshed", "helm_refresh", json!({}));
                }
            },
        );
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                widgets::section(ui, "Treasury", None);
                widgets::card(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "A real BuchCraft book supplies this from its bank account (default 2800). A demo ledger is ignored. Otherwise set it here. HELMCRAFT_CASH_CENTS is the same fallback.",
                        )
                        .color(theme::current(ui.ctx()).text_weak),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Cash (EUR):");
                        ui.add(TextEdit::singleline(&mut self.cash_input).desired_width(160.0));
                        if widgets::primary(ui, "Save").clicked() {
                            match self.cash_from_input() {
                                Ok(v) => f.run(
                                    format!("Cash set to EUR {v:.2}"),
                                    "helm_set_cash",
                                    json!({"cash_eur": v}),
                                ),
                                Err(e) => f.toast(e, Tone::Danger),
                            }
                        }
                    });
                    ui.label(format!(
                        "Stored: {}",
                        self.store
                            .configured_cash_cents()
                            .map(money)
                            .unwrap_or_else(|| "not configured".into())
                    ));
                });
            });
            cols[1].vertical(|ui| {
                widgets::section(ui, "Storage", None);
                widgets::card(ui, |ui| {
                    ui.label(format!("Cockpit DB: {}", self.db_path.display()));
                    ui.label(format!("Data root: {}", data_root().display()));
                    ui.label(format!("Snapshots: {}", self.snapshot_total));
                    if let Some(path) = &self.last_export {
                        ui.label(format!("Last export: {path}"));
                    }
                });
            });
        });
        ui.add_space(10.0);
        ui.columns(2, |cols| {
            cols[0].vertical(|ui| {
                widgets::section(ui, "Appearance", None);
                widgets::card(ui, |ui| {
                    ui.label("Theme: the Dark / Light button in the status bar switches and persists it.");
                    ui.label("Zoom: Ctrl + mouse wheel, Ctrl + Plus/Minus; the percentage in the status bar resets to 100%.");
                    ui.label("Language: the Deutsch / English button in the status bar.");
                    ui.label("Command palette: Ctrl+K lists every tool with a generated form.");
                });
            });
            cols[1].vertical(|ui| {
                widgets::section(ui, "Keyboard", None);
                widgets::card(ui, |ui| {
                    ui.label("F5 - reload telemetry");
                    ui.label("Ctrl+K - command palette");
                    ui.label("Ctrl+/ - keyboard shortcut sheet");
                    ui.label("Alt+1..9 - switch cockpit page (View menu)");
                });
            });
        });
    }
}

fn main() -> std::process::ExitCode {
    app::main_with(default_db(), open, "helm_refresh")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use craft_biz_ui::app::{Args, Shell};

    fn test_app() -> HelmApp {
        let store = HelmStore::open_in_memory().unwrap();
        let bus = Bus::new(std::env::temp_dir().join("helmcraft-test-bus"));
        HelmApp::from_store(store, PathBuf::from(":memory:"), bus).unwrap()
    }

    #[test]
    fn every_tool_has_a_place_in_the_window() {
        craft_biz_ui::coverage::check(&tools_json(), COVERAGE).unwrap();
    }

    #[test]
    fn nav_lists_every_view() {
        let app = test_app();
        assert_eq!(app.nav_entries().len(), VIEWS);
    }

    #[test]
    fn cash_and_snapshot_tools_work_in_memory() {
        let mut app = test_app();
        app.call("helm_set_cash", &json!({"cash_eur": 1420500.0}))
            .unwrap();
        assert_eq!(app.store.configured_cash_cents(), Some(142_050_000));
        app.call("helm_snapshot_save", &json!({})).unwrap();
        let saved = app.call("helm_snapshot_list", &json!({})).unwrap();
        assert_eq!(saved.as_array().map(Vec::len), Some(1));
    }

    #[test]
    fn smoke_checks_menu_parity_and_glyphs() {
        let app = test_app();
        let mut shell = Shell::new(app, None);
        let out = shell.smoke().unwrap();
        assert!(out.get("menu_check").is_some());
    }

    #[test]
    fn english_shell_leaves_no_error() {
        let mut shell = Shell::new(test_app(), None).with_args(&Args {
            lang: Some("en".into()),
            ..Args::default()
        });
        assert!(shell.menu_check().is_ok());
    }

    #[test]
    fn every_view_state_is_available() {
        let app = test_app();
        assert_eq!(app.nav_entries().len(), VIEWS);
        assert_eq!(V_OVERVIEW, 0);
        assert_eq!(V_SETTINGS, VIEWS - 1);
    }
}
