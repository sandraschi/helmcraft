#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use clap::{Parser, Subcommand};
use craft_biz::bus::Bus;
use helmcraft_core::*;
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "helmcraft",
    author,
    version,
    about = "Local-first CEO/CFO Executive Cockpit & Fleet Radar. CLI + MCP."
)]
struct Cli {
    #[arg(long)]
    db: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    Status,
    Briefing,
    Forecast {
        #[arg(default_value_t = 12)]
        months: u32,
    },
    SaveSnapshot,
    Snapshots {
        #[arg(default_value_t = 20)]
        limit: usize,
    },
    DeleteSnapshot {
        id: i64,
    },
    SetCash {
        cash_eur: f64,
    },
    Sources,
    ExportBriefing {
        path: PathBuf,
    },
    ExportSnapshots {
        path: PathBuf,
    },
    Stats,
    Tools,
    Mcp,
}

fn resolve_db(path: Option<PathBuf>) -> PathBuf {
    if let Some(p) = path {
        return p;
    }
    if let Ok(p) = std::env::var("HELMCRAFT_DB") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let dir = PathBuf::from(local).join("helmcraft");
        let _ = std::fs::create_dir_all(&dir);
        return dir.join("helmcraft.db");
    }
    PathBuf::from("helmcraft.db")
}

fn resolve_bus() -> Option<Bus> {
    if let Ok(dir) = std::env::var("CRAFT_BUS_DIR") {
        return Some(Bus::new(PathBuf::from(dir)));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        return Some(Bus::new(PathBuf::from(local).join("craft-biz-bus")));
    }
    None
}

fn write_file(target: &Path, body: &str) -> Result<Value, Box<dyn std::error::Error>> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(target, body.as_bytes())?;
    Ok(json!({
        "path": target.display().to_string(),
        "bytes": body.len(),
    }))
}

fn briefing_markdown(service: &HelmService) -> String {
    service.generate_board_briefing().to_markdown()
}

fn snapshots_csv(store: &HelmStore, limit: usize) -> Result<String, Box<dyn std::error::Error>> {
    let mut out = String::from(
        "id;saved_at;cash_cents;runway_months;mrr_cents;arr_cents;receivables_cents;overdue_cents;headcount;azg_violations;at_risk;blocked\n",
    );
    for rec in store.list_snapshots(limit)? {
        let s = &rec.snapshot;
        out.push_str(&format!(
            "{};{};{};{:.1};{};{};{};{};{};{};{};{}\n",
            rec.id,
            rec.saved_at,
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
    Ok(out)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let db_path = resolve_db(cli.db);
    let store = HelmStore::open(db_path)?;
    let bus = resolve_bus();
    let service = HelmService::new(store, bus);

    match cli.command.unwrap_or(Commands::Status) {
        Commands::Status => {
            let snap = service.store().aggregate_live_fleet();
            println!("=== HELMCRAFT EXECUTIVE STATUS ===");
            println!("Timestamp:           {}", snap.timestamp);
            println!(
                "Treasury Cash:       {}",
                if snap.treasury.cash_configured {
                    format!("EUR {:.2}", snap.treasury.cash_cents as f64 / 100.0)
                } else {
                    "not configured (helmcraft set-cash)".to_string()
                }
            );
            println!(
                "Monthly Burn:        EUR {:.2} (payroll {:.2}, expenses {:.2})",
                snap.treasury.burn_monthly_cents as f64 / 100.0,
                snap.treasury.burn_payroll_cents as f64 / 100.0,
                snap.treasury.burn_expenses_cents as f64 / 100.0
            );
            println!(
                "Cash Runway:         {}",
                if snap.treasury.cash_configured {
                    format!("{:.1} months", snap.treasury.runway_months)
                } else {
                    "unavailable".to_string()
                }
            );
            println!(
                "ARR / MRR:           EUR {:.2} / EUR {:.2}",
                snap.treasury.arr_cents as f64 / 100.0,
                snap.treasury.mrr_cents as f64 / 100.0
            );
            println!(
                "Receivables:         EUR {:.2} (Overdue: EUR {:.2})",
                snap.treasury.receivables_cents as f64 / 100.0,
                snap.treasury.overdue_cents as f64 / 100.0
            );
            println!(
                "Headcount:           {} FTE (Open Reqs: {}, Offers: {})",
                snap.people.headcount_fte,
                snap.people.open_requisitions,
                snap.people.pending_offers
            );
            println!(
                "Austrian Compliance: {} uncleared AZG violations",
                snap.people.azg_violations
            );
            println!(
                "Support & Churn:     {} open tickets ({} SLA breaches), {} accounts at risk",
                snap.ops_risk.open_tickets,
                snap.ops_risk.sla_breaches,
                snap.ops_risk.at_risk_accounts
            );
            println!(
                "Sources:             {} of {} craft databases",
                snap.sources_read, snap.sources_total
            );
            println!(
                "Action Items:        {} items pending",
                snap.action_items.len()
            );
            for (idx, item) in snap.action_items.iter().enumerate() {
                println!("  {}. {}", idx + 1, item);
            }
        }
        Commands::Briefing => {
            println!("{}", briefing_markdown(&service));
        }
        Commands::Forecast { months } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&service.cashflow_forecast(months))?
            );
        }
        Commands::SaveSnapshot => {
            let snap = service.store().aggregate_live_fleet();
            let id = service.store().save_snapshot(&snap)?;
            println!(
                "Saved executive snapshot #{id} with timestamp {}",
                snap.timestamp
            );
        }
        Commands::Snapshots { limit } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&service.store().list_snapshots(limit)?)?
            );
        }
        Commands::DeleteSnapshot { id } => {
            let deleted = service.store().delete_snapshot(id)?;
            println!("{}", json!({"deleted": deleted, "id": id}));
        }
        Commands::SetCash { cash_eur } => {
            if !cash_eur.is_finite() || cash_eur < 0.0 {
                return Err("cash must be zero or positive".into());
            }
            service
                .store()
                .set_cash_cents((cash_eur * 100.0).round() as i64)?;
            println!("Treasury cash set to EUR {cash_eur:.2}");
        }
        Commands::Sources => {
            println!(
                "{}",
                serde_json::to_string_pretty(&service.store().source_health())?
            );
        }
        Commands::ExportBriefing { path } => {
            println!("{}", write_file(&path, &briefing_markdown(&service))?);
        }
        Commands::ExportSnapshots { path } => {
            println!(
                "{}",
                write_file(&path, &snapshots_csv(service.store(), 10_000)?)?
            );
        }
        Commands::Stats => {
            let snap = service.store().aggregate_live_fleet();
            println!("{}", serde_json::to_string_pretty(&snap)?);
        }
        Commands::Tools => {
            println!("{}", serde_json::to_string_pretty(&tools())?);
        }
        Commands::Mcp => {
            run_mcp_loop(&service)?;
        }
    }

    Ok(())
}

fn tools() -> Value {
    json!([
        {
            "name": "helm_status",
            "description": "Retrieve real-time executive dashboard KPIs across all local craft business databases (Treasury, People, Ops Risk, Actions)",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "helm_refresh",
            "description": "Re-read every local craft database and return the refreshed snapshot",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "helm_board_briefing",
            "description": "Generate a CEO/CFO board briefing, risk radar and recommendations with provenance",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "helm_cashflow_forecast",
            "description": "Project cash month by month from MRR, burn and open receivables",
            "inputSchema": { "type": "object", "properties": {
                "months": { "type": "integer", "description": "Projection length in months (1-60)", "default": 12 }
            }}
        },
        {
            "name": "helm_snapshot_save",
            "description": "Save the current executive snapshot to the history database",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "helm_snapshot_list",
            "description": "List saved executive snapshots, newest first",
            "inputSchema": { "type": "object", "properties": {
                "limit": { "type": "integer", "description": "Maximum records (default 20)", "default": 20 }
            }}
        },
        {
            "name": "helm_snapshot_delete",
            "description": "Delete one saved snapshot by id",
            "inputSchema": { "type": "object", "properties": {
                "id": { "type": "integer", "description": "Snapshot id" }
            }, "required": ["id"] }
        },
        {
            "name": "helm_export_briefing",
            "description": "Write the board briefing as Markdown",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Destination .md file" }
            }, "required": ["path"] }
        },
        {
            "name": "helm_export_snapshots_csv",
            "description": "Write the snapshot history as CSV",
            "inputSchema": { "type": "object", "properties": {
                "path": { "type": "string", "description": "Destination .csv file" }
            }, "required": ["path"] }
        },
        {
            "name": "helm_set_cash",
            "description": "Set the treasury cash balance used for runway calculations",
            "inputSchema": { "type": "object", "properties": {
                "cash_eur": { "type": "number", "description": "Cash balance in euros" }
            }, "required": ["cash_eur"] }
        },
        {
            "name": "helm_sources",
            "description": "Audit every local craft database read by the cockpit (path, table, rows, status)",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn tool_result(id: Value, value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value)
        .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string());
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": { "content": [{ "type": "text", "text": text }] }
    })
}

fn call_tool(service: &HelmService, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "helm_status" | "helm_refresh" => {
            serde_json::to_value(service.store().aggregate_live_fleet()).map_err(|e| e.to_string())
        }
        "helm_board_briefing" => {
            serde_json::to_value(service.generate_board_briefing()).map_err(|e| e.to_string())
        }
        "helm_cashflow_forecast" => {
            let months = args
                .get("months")
                .and_then(Value::as_u64)
                .unwrap_or(12)
                .min(60) as u32;
            serde_json::to_value(service.cashflow_forecast(months)).map_err(|e| e.to_string())
        }
        "helm_snapshot_save" => {
            let snap = service.store().aggregate_live_fleet();
            let id = service
                .store()
                .save_snapshot(&snap)
                .map_err(|e| e.to_string())?;
            Ok(json!({"saved": true, "id": id, "timestamp": snap.timestamp}))
        }
        "helm_snapshot_list" => {
            let limit = args
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(20)
                .min(200) as usize;
            serde_json::to_value(
                service
                    .store()
                    .list_snapshots(limit)
                    .map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())
        }
        "helm_snapshot_delete" => {
            let id = args
                .get("id")
                .and_then(Value::as_i64)
                .ok_or("id is required")?;
            let deleted = service
                .store()
                .delete_snapshot(id)
                .map_err(|e| e.to_string())?;
            Ok(json!({"deleted": deleted, "id": id}))
        }
        "helm_export_briefing" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("path is required")?;
            write_file(Path::new(path), &briefing_markdown(service)).map_err(|e| e.to_string())
        }
        "helm_export_snapshots_csv" => {
            let path = args
                .get("path")
                .and_then(Value::as_str)
                .ok_or("path is required")?;
            let body = snapshots_csv(service.store(), 10_000).map_err(|e| e.to_string())?;
            write_file(Path::new(path), &body).map_err(|e| e.to_string())
        }
        "helm_set_cash" => {
            let eur = args
                .get("cash_eur")
                .and_then(Value::as_f64)
                .ok_or("cash_eur is required")?;
            if !eur.is_finite() || eur < 0.0 {
                return Err("cash_eur must be zero or positive".into());
            }
            service
                .store()
                .set_cash_cents((eur * 100.0).round() as i64)
                .map_err(|e| e.to_string())?;
            Ok(json!({"cash_eur": eur, "cash_cents": (eur * 100.0).round() as i64}))
        }
        "helm_sources" => {
            serde_json::to_value(service.store().source_health()).map_err(|e| e.to_string())
        }
        other => Err(format!("Unknown tool: {other}")),
    }
}

fn run_mcp_loop(service: &HelmService) -> Result<(), Box<dyn std::error::Error>> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }

        let request: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err_res = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {e}") }
                });
                writeln!(stdout, "{}", serde_json::to_string(&err_res)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "helmcraft-mcp", "version": "0.2.0" }
                }
            }),
            "notifications/initialized" => continue,
            "tools/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "tools": tools() }
            }),
            "tools/call" => {
                let params = request.get("params").cloned().unwrap_or(Value::Null);
                let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                match call_tool(service, name, &args) {
                    Ok(value) => tool_result(id, value),
                    Err(e) => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32602, "message": e }
                    }),
                }
            }
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("Method not found: {method}") }
            }),
        };

        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn tools_catalog_has_every_documented_tool() {
        let catalog = tools();
        let names: Vec<&str> = catalog
            .as_array()
            .map(|a| a.iter().filter_map(|t| t["name"].as_str()).collect())
            .unwrap_or_default();
        for expected in [
            "helm_status",
            "helm_board_briefing",
            "helm_cashflow_forecast",
            "helm_snapshot_save",
            "helm_snapshot_list",
            "helm_snapshot_delete",
            "helm_export_briefing",
            "helm_export_snapshots_csv",
            "helm_set_cash",
            "helm_sources",
        ] {
            assert!(names.contains(&expected), "missing {expected}");
        }
    }

    #[test]
    fn cash_and_snapshot_roundtrip_in_memory() {
        let store = HelmStore::open_in_memory().unwrap();
        let service = HelmService::new(store, None);
        let set = call_tool(&service, "helm_set_cash", &json!({"cash_eur": 1000.0})).unwrap();
        assert_eq!(set["cash_cents"], 100_000);
        call_tool(&service, "helm_snapshot_save", &json!({})).unwrap();
        let listed = call_tool(&service, "helm_snapshot_list", &json!({})).unwrap();
        assert_eq!(listed.as_array().map(Vec::len), Some(1));
    }
}
