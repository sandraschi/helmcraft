#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use clap::Parser;
use eframe::egui::{self, Color32, RichText};
use helmcraft_core::*;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "helmcraft-app", about = "Native egui desktop host for HelmCraft")]
struct Args {
    #[arg(long)]
    smoke: bool,
    #[arg(long)]
    db: Option<PathBuf>,
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

fn run_smoke_checks() -> Result<(), Box<dyn std::error::Error>> {
    let store = HelmStore::open_in_memory()?;
    let snap = store.aggregate_live_fleet();
    store.save_snapshot(&snap)?;
    if snap.treasury.runway_months <= 0.0 {
        return Err("Runway calculation failed".into());
    }
    println!("smoke: ok (helmcraft-app)");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.smoke {
        return run_smoke_checks();
    }

    let db_path = resolve_db(args.db);
    let store = HelmStore::open(&db_path)?;
    let app = HelmApp::new(store);

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120.0, 720.0])
            .with_title("HelmCraft — Sovereign CEO/CFO Executive Cockpit & Fleet Radar"),
        ..Default::default()
    };

    eframe::run_native(
        "HelmCraft",
        native_options,
        Box::new(|_cc| Ok(Box::new(app))),
    )
    .map_err(|e| format!("Eframe runtime error: {e}").into())
}

struct HelmApp {
    store: HelmStore,
    snapshot: ExecutiveSnapshot,
    briefing: BoardBriefing,
    status_msg: String,
}

impl HelmApp {
    fn new(store: HelmStore) -> Self {
        let service = HelmService::new(store, None);
        let briefing = service.generate_board_briefing();
        let snapshot = briefing.snapshot.clone();

        let store = match HelmStore::open_in_memory() {
            Ok(s) => s,
            Err(_) => {
                let p = resolve_db(None);
                HelmStore::open(p).unwrap_or_else(|_| panic_free_fallback_store())
            }
        };

        Self {
            store,
            snapshot,
            briefing,
            status_msg: "Cockpit telemetry synchronized across fleet.".to_string(),
        }
    }

    fn refresh(&mut self) {
        self.snapshot = self.store.aggregate_live_fleet();
        let service = HelmService::new(panic_free_fallback_store(), None);
        self.briefing = service.generate_board_briefing();
        self.status_msg = format!("Synchronized at {}", chrono::Utc::now().format("%H:%M:%S UTC"));
    }
}

fn panic_free_fallback_store() -> HelmStore {
    HelmStore::open_in_memory().unwrap_or_else(|_| {
        let p = std::env::temp_dir().join("helm_fallback.db");
        HelmStore::open(p).unwrap_or_else(|_| {
            std::process::exit(1);
        })
    })
}

impl eframe::App for HelmApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_ui(ui);
    }
}

impl HelmApp {
    fn render_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading(RichText::new("HelmCraft").strong().color(Color32::from_rgb(56, 189, 248)));
            ui.label(RichText::new("— Sovereign CEO/CFO Fleet Cockpit").italics());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("⟳ Refresh Fleet Telemetry").clicked() {
                    self.refresh();
                }
                ui.label(RichText::new(&self.status_msg).size(11.0).color(Color32::GRAY));
            });
        });

        ui.separator();

        // Top Executive Metric Cards
        ui.columns(4, |cols| {
            cols[0].group(|ui| {
                ui.label(RichText::new("TREASURY CASH").size(11.0).color(Color32::GRAY));
                let cash_fmt = format!("€{:.2}M", self.snapshot.treasury.cash_cents as f64 / 100_000_000.0);
                ui.label(RichText::new(cash_fmt).size(20.0).strong().color(Color32::from_rgb(34, 197, 94)));
                ui.label(format!("Runway: {:.1} months", self.snapshot.treasury.runway_months));
            });
            cols[1].group(|ui| {
                ui.label(RichText::new("ARR / MRR").size(11.0).color(Color32::GRAY));
                let arr_fmt = format!("€{:.2}M", self.snapshot.treasury.arr_cents as f64 / 100_000_000.0);
                ui.label(RichText::new(arr_fmt).size(20.0).strong().color(Color32::from_rgb(56, 189, 248)));
                ui.label(format!("Burn: €{:.1}k / mo", self.snapshot.treasury.burn_monthly_cents as f64 / 100_000.0));
            });
            cols[2].group(|ui| {
                ui.label(RichText::new("TALENT & ORG").size(11.0).color(Color32::GRAY));
                let fte_fmt = format!("{} FTE", self.snapshot.people.headcount_fte);
                ui.label(RichText::new(fte_fmt).size(20.0).strong().color(Color32::from_rgb(168, 85, 247)));
                ui.label(format!("{} Open Reqs | {} Offers", self.snapshot.people.open_requisitions, self.snapshot.people.pending_offers));
            });
            cols[3].group(|ui| {
                ui.label(RichText::new("OPS RISK / AZG").size(11.0).color(Color32::GRAY));
                let risk_color = if self.snapshot.people.azg_violations == 0 {
                    Color32::from_rgb(34, 197, 94)
                } else {
                    Color32::from_rgb(239, 68, 68)
                };
                let azg_fmt = format!("{} AZG Violations", self.snapshot.people.azg_violations);
                ui.label(RichText::new(azg_fmt).size(18.0).strong().color(risk_color));
                ui.label(format!("{} Churn Risks | {} Tickets", self.snapshot.ops_risk.at_risk_accounts, self.snapshot.ops_risk.open_tickets));
            });
        });

        ui.add_space(8.0);

        // Main 3-column Radar
        ui.columns(3, |cols| {
            // Column 1: Financial & Treasury
            cols[0].group(|ui| {
                ui.heading(RichText::new("Treasury & Receivables").size(14.0).strong());
                ui.separator();
                ui.label(format!("• Total Cash: €{:.2}", self.snapshot.treasury.cash_cents as f64 / 100.0));
                ui.label(format!("• Monthly Burn: €{:.2}", self.snapshot.treasury.burn_monthly_cents as f64 / 100.0));
                ui.label(format!("• Accounts Receivable: €{:.2}", self.snapshot.treasury.receivables_cents as f64 / 100.0));
                
                let overdue_color = if self.snapshot.treasury.overdue_cents > 0 {
                    Color32::from_rgb(239, 68, 68)
                } else {
                    Color32::from_rgb(34, 197, 94)
                };
                ui.label(RichText::new(format!("• Overdue Dunning: €{:.2}", self.snapshot.treasury.overdue_cents as f64 / 100.0)).color(overdue_color));
                ui.add_space(6.0);
                ui.label(RichText::new("Automated Actions:").italics());
                ui.label("SEPA Direct Debit pain.008 export ready in BillCraft.");
            });

            // Column 2: Talent & Operations
            cols[1].group(|ui| {
                ui.heading(RichText::new("People & Asset Fleet").size(14.0).strong());
                ui.separator();
                ui.label(format!("• Active Headcount: {} employees", self.snapshot.people.headcount_fte));
                ui.label(format!("• Open Requisitions: {} positions", self.snapshot.people.open_requisitions));
                ui.label(format!("• Active Candidates: {} in pipeline", self.snapshot.people.active_candidates));
                ui.label(format!("• Pending Job Offers: {}", self.snapshot.people.pending_offers));
                ui.label(format!("• Unassigned Devices: {} laptops/workstations", self.snapshot.people.unassigned_assets));
                ui.add_space(6.0);
                ui.label(RichText::new("Compliance Status:").italics());
                ui.label("Austrian AZG 38.5h statutory workweek monitoring active.");
            });

            // Column 3: Executive Action Items
            cols[2].group(|ui| {
                ui.heading(RichText::new("Executive Action Items").size(14.0).strong());
                ui.separator();
                if self.snapshot.action_items.is_empty() {
                    ui.label(RichText::new("All systems nominal. No immediate escalations.").color(Color32::from_rgb(34, 197, 94)));
                } else {
                    for (i, action) in self.snapshot.action_items.iter().enumerate() {
                        ui.label(format!("{}. {}", i + 1, action));
                    }
                }
            });
        });

        ui.add_space(8.0);

        // Bottom Board Briefing Panel
        ui.group(|ui| {
            ui.heading(RichText::new("Board Briefing Preview").size(14.0).strong());
            ui.separator();
            ui.label(RichText::new(&self.briefing.executive_summary).size(12.0));
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Fleet Recommendations:").strong());
                for rec in &self.briefing.key_recommendations {
                    ui.label(format!("• {}", rec));
                }
            });
        });
    }
}
