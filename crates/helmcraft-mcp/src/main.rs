#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use clap::{Parser, Subcommand};
use craft_biz::bus::Bus;
use helmcraft_core::*;
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "helmcraft", author, version, about = "Local-first CEO/CFO Executive Cockpit & Fleet Radar. CLI + MCP.")]
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
    SaveSnapshot,
    Stats,
    Tools,
    Mcp,
}

fn resolve_db(path: Option<PathBuf>) -> PathBuf {
    if let Some(p) = path {
        return p;
    }
    if let Ok(p) = std::env::var("HELMCRAFT_DB") {
        return PathBuf::from(p);
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
            println!("Treasury Cash:       €{:.2}", snap.treasury.cash_cents as f64 / 100.0);
            println!("Monthly Burn:        €{:.2}", snap.treasury.burn_monthly_cents as f64 / 100.0);
            println!("Cash Runway:         {:.1} months", snap.treasury.runway_months);
            println!("ARR / MRR:           €{:.2} / €{:.2}", snap.treasury.arr_cents as f64 / 100.0, snap.treasury.mrr_cents as f64 / 100.0);
            println!("Receivables:         €{:.2} (Overdue: €{:.2})", snap.treasury.receivables_cents as f64 / 100.0, snap.treasury.overdue_cents as f64 / 100.0);
            println!("Headcount:           {} FTE (Open Reqs: {}, Offers: {})", snap.people.headcount_fte, snap.people.open_requisitions, snap.people.pending_offers);
            println!("Austrian Compliance: {} AZG violations", snap.people.azg_violations);
            println!("Support & Churn:     {} open tickets ({} SLA breaches), {} accounts at risk", snap.ops_risk.open_tickets, snap.ops_risk.sla_breaches, snap.ops_risk.at_risk_accounts);
            println!("Action Items:        {} items pending", snap.action_items.len());
            for (idx, item) in snap.action_items.iter().enumerate() {
                println!("  {}. {}", idx + 1, item);
            }
        }
        Commands::Briefing => {
            let briefing = service.generate_board_briefing();
            println!("# EXECUTIVE BOARD BRIEFING");
            println!("\n## Executive Summary\n{}", briefing.executive_summary);
            println!("\n## Key Fleet Risks");
            if briefing.top_risks.is_empty() {
                println!("- None detected across active craft telemetry.");
            } else {
                for r in &briefing.top_risks {
                    println!("- {}", r);
                }
            }
            println!("\n## Strategic Recommendations");
            for rec in &briefing.key_recommendations {
                println!("- {}", rec);
            }
        }
        Commands::SaveSnapshot => {
            let snap = service.store().aggregate_live_fleet();
            service.store().save_snapshot(&snap)?;
            println!("Saved executive snapshot with timestamp {}", snap.timestamp);
        }
        Commands::Stats => {
            let snap = service.store().aggregate_live_fleet();
            println!("{}", serde_json::to_string_pretty(&snap)?);
        }
        Commands::Tools => {
            let tools = json!([
                {
                    "name": "helm_status",
                    "description": "Retrieve real-time executive dashboard KPIs across all 11 craft business apps (Treasury, People, Ops Risk, Actions)",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "helm_board_briefing",
                    "description": "Generate comprehensive CEO/CFO executive board briefing and risk assessment",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "helm_cashflow_forecast",
                    "description": "Calculate runway and burn projections based on current receivables, MRR, and OpEx",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]);
            println!("{}", serde_json::to_string_pretty(&tools)?);
        }
        Commands::Mcp => {
            run_mcp_loop(&service)?;
        }
    }

    Ok(())
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

        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err_res = json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": { "code": -32700, "message": format!("Parse error: {}", e) }
                });
                writeln!(stdout, "{}", serde_json::to_string(&err_res)?)?;
                stdout.flush()?;
                continue;
            }
        };

        let id = request.get("id").cloned().unwrap_or(serde_json::Value::Null);
        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let response = match method {
            "initialize" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": { "tools": {} },
                        "serverInfo": {
                            "name": "helmcraft-mcp",
                            "version": "0.1.0"
                        }
                    }
                })
            }
            "tools/list" => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "tools": [
                            {
                                "name": "helm_status",
                                "description": "Retrieve real-time executive dashboard KPIs across all 11 craft business apps (Treasury, People, Ops Risk, Actions)",
                                "inputSchema": { "type": "object", "properties": {} }
                            },
                            {
                                "name": "helm_board_briefing",
                                "description": "Generate comprehensive CEO/CFO executive board briefing and risk assessment",
                                "inputSchema": { "type": "object", "properties": {} }
                            },
                            {
                                "name": "helm_cashflow_forecast",
                                "description": "Calculate runway and burn projections based on current receivables, MRR, and OpEx",
                                "inputSchema": { "type": "object", "properties": {} }
                            }
                        ]
                    }
                })
            }
            "tools/call" => {
                let tool_name = request.get("params")
                    .and_then(|p| p.get("name"))
                    .and_then(|n| n.as_str())
                    .unwrap_or("");

                match tool_name {
                    "helm_status" => {
                        let snap = service.store().aggregate_live_fleet();
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&snap)?
                                }]
                            }
                        })
                    }
                    "helm_board_briefing" => {
                        let briefing = service.generate_board_briefing();
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&briefing)?
                                }]
                            }
                        })
                    }
                    "helm_cashflow_forecast" => {
                        let snap = service.store().aggregate_live_fleet();
                        let forecast = json!({
                            "cash_cents": snap.treasury.cash_cents,
                            "cash_formatted": format!("€{:.2}", snap.treasury.cash_cents as f64 / 100.0),
                            "monthly_burn_formatted": format!("€{:.2}", snap.treasury.burn_monthly_cents as f64 / 100.0),
                            "runway_months": snap.treasury.runway_months,
                            "mrr_formatted": format!("€{:.2}", snap.treasury.mrr_cents as f64 / 100.0),
                            "arr_formatted": format!("€{:.2}", snap.treasury.arr_cents as f64 / 100.0),
                            "receivables_formatted": format!("€{:.2}", snap.treasury.receivables_cents as f64 / 100.0),
                            "overdue_formatted": format!("€{:.2}", snap.treasury.overdue_cents as f64 / 100.0)
                        });
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "content": [{
                                    "type": "text",
                                    "text": serde_json::to_string_pretty(&forecast)?
                                }]
                            }
                        })
                    }
                    unknown => {
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "error": {
                                "code": -32601,
                                "message": format!("Unknown tool: {}", unknown)
                            }
                        })
                    }
                }
            }
            _ => {
                json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {
                        "code": -32601,
                        "message": format!("Method not found: {}", method)
                    }
                })
            }
        };

        writeln!(stdout, "{}", serde_json::to_string(&response)?)?;
        stdout.flush()?;
    }

    Ok(())
}
