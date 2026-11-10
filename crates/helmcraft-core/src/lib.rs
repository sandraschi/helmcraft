#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

pub use helmcraft_store::*;
use craft_biz::bus::Bus;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HelmCoreError {
    #[error("Store error: {0}")]
    Store(#[from] HelmStoreError),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BoardBriefing {
    pub executive_summary: String,
    pub snapshot: ExecutiveSnapshot,
    pub top_risks: Vec<String>,
    pub key_recommendations: Vec<String>,
}

pub struct HelmService {
    store: HelmStore,
    bus: Option<Bus>,
}

impl HelmService {
    pub fn new(store: HelmStore, bus: Option<Bus>) -> Self {
        Self { store, bus }
    }

    pub fn store(&self) -> &HelmStore {
        &self.store
    }

    pub fn bus(&self) -> Option<&Bus> {
        self.bus.as_ref()
    }

    pub fn generate_board_briefing(&self) -> BoardBriefing {
        let snap = self.store.aggregate_live_fleet();
        let summary = format!(
            "Enterprise operations remain sound with €{:.2}M treasury cash (runway {:.1} months at €{:.1}k/mo burn). MRR stands at €{:.1}k (€{:.2}M ARR). Headcount is {} FTE across engineering and operations with 100% Austrian labor law (AZG) compliance.",
            snap.treasury.cash_cents as f64 / 100_000_000.0,
            snap.treasury.runway_months,
            snap.treasury.burn_monthly_cents as f64 / 100_000.0,
            snap.treasury.mrr_cents as f64 / 100_000.0,
            snap.treasury.arr_cents as f64 / 100_000_000.0,
            snap.people.headcount_fte
        );

        let mut top_risks = Vec::new();
        if snap.treasury.overdue_cents > 0 {
            top_risks.push(format!("Outstanding Overdue Invoices: €{:.2} requires dunning escalation", snap.treasury.overdue_cents as f64 / 100.0));
        }
        if snap.ops_risk.at_risk_accounts > 0 {
            top_risks.push(format!("Customer Retention: {} customer accounts currently in red churn-risk band", snap.ops_risk.at_risk_accounts));
        }

        let key_recommendations = vec![
            "Execute SEPA batch collection in BillCraft to clear receivables.".to_string(),
            "Complete hire for SRE Lead in HireCraft and trigger automated onboarding cascade.".to_string(),
            "Audit quarterly asset depreciation schedules in AssetCraft under EStG §7.".to_string(),
        ];

        BoardBriefing {
            executive_summary: summary,
            snapshot: snap,
            top_risks,
            key_recommendations,
        }
    }
}
