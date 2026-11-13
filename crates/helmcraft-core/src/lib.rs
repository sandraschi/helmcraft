#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use craft_biz::bus::Bus;
pub use helmcraft_store::*;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HelmCoreError {
    #[error("Store error: {0}")]
    Store(#[from] HelmStoreError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BoardBriefing {
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub executive_summary: String,
    pub snapshot: ExecutiveSnapshot,
    pub top_risks: Vec<String>,
    pub key_recommendations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastMonth {
    pub month: u32,
    pub inflow_cents: i64,
    pub outflow_cents: i64,
    pub net_cents: i64,
    pub cash_cents: i64,
    pub insolvent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CashflowForecast {
    pub start_cash_cents: i64,
    pub cash_configured: bool,
    pub monthly_burn_cents: i64,
    pub monthly_mrr_cents: i64,
    /// Month index (1-based) when cash first goes below zero, if it does.
    pub zero_cash_month: Option<u32>,
    pub months: Vec<ForecastMonth>,
    pub notes: Vec<String>,
}

fn eur(cents: i64) -> String {
    format!("EUR {:.2}", cents as f64 / 100.0)
}

impl BoardBriefing {
    /// Markdown the board pack can consume directly.
    pub fn to_markdown(&self) -> String {
        let s = &self.snapshot;
        let t = &s.treasury;
        let mut out = String::new();
        out.push_str("# HelmCraft Board Briefing\n\n");
        out.push_str(&format!(
            "Generated: {}\n\n",
            self.generated_at.to_rfc3339()
        ));
        out.push_str("## Executive summary\n\n");
        out.push_str(&self.executive_summary);
        out.push_str("\n\n## Key metrics\n\n");
        out.push_str(&format!(
            "- Treasury cash: {}\n- Monthly burn: {} (payroll {}, expenses {})\n- Runway: {}\n- MRR / ARR: {} / {}\n- Receivables: {} (overdue {})\n- Headcount: {} FTE\n- AZG violations (uncleared): {}\n- Churn-risk accounts: {}\n- Blocked issues: {}\n",
            if t.cash_configured {
                eur(t.cash_cents)
            } else {
                "not configured".to_string()
            },
            eur(t.burn_monthly_cents),
            eur(t.burn_payroll_cents),
            eur(t.burn_expenses_cents),
            if t.cash_configured {
                format!("{:.1} months", t.runway_months)
            } else {
                "unavailable (cash not configured)".to_string()
            },
            eur(t.mrr_cents),
            eur(t.arr_cents),
            eur(t.receivables_cents),
            eur(t.overdue_cents),
            s.people.headcount_fte,
            s.people.azg_violations,
            s.ops_risk.at_risk_accounts,
            s.ops_risk.blocked_issues,
        ));
        out.push_str("\n## Risks\n\n");
        if self.top_risks.is_empty() {
            out.push_str("- None detected across connected craft telemetry.\n");
        } else {
            for risk in &self.top_risks {
                out.push_str(&format!("- {risk}\n"));
            }
        }
        out.push_str("\n## Recommendations\n\n");
        for rec in &self.key_recommendations {
            out.push_str(&format!("- {rec}\n"));
        }
        out.push_str("\n## Telemetry provenance\n\n");
        out.push_str(&format!(
            "{} of {} craft databases read.\n",
            s.sources_read, s.sources_total
        ));
        for note in &s.data_notes {
            out.push_str(&format!("- {note}\n"));
        }
        out.push_str("\n## Action items\n\n");
        if s.action_items.is_empty() {
            out.push_str("- No escalations.\n");
        } else {
            for item in &s.action_items {
                out.push_str(&format!("- {item}\n"));
            }
        }
        out
    }
}

pub fn generate_board_briefing(store: &HelmStore) -> BoardBriefing {
    let snap = store.aggregate_live_fleet();
    let t = &snap.treasury;

    let summary = if t.cash_configured {
        format!(
            "Treasury holds {} with {:.1} months of runway at {}/month burn. MRR is {} ({} ARR), with {} FTE and {} uncleared AZG violation(s).",
            eur(t.cash_cents),
            t.runway_months,
            eur(t.burn_monthly_cents),
            eur(t.mrr_cents),
            eur(t.arr_cents),
            snap.people.headcount_fte,
            snap.people.azg_violations,
        )
    } else {
        format!(
            "Treasury cash is not configured, so runway cannot be stated. Live payroll and reimbursed expenses give {}/month burn; MRR is {} ({} ARR), {} FTE, and {} uncleared AZG violation(s). Set the cash balance in HelmCraft Settings for a runway figure.",
            eur(t.burn_monthly_cents),
            eur(t.mrr_cents),
            eur(t.arr_cents),
            snap.people.headcount_fte,
            snap.people.azg_violations,
        )
    };

    let mut top_risks = Vec::new();
    if t.overdue_cents > 0 {
        top_risks.push(format!(
            "Overdue receivables: {} needs dunning escalation.",
            eur(t.overdue_cents)
        ));
    }
    if snap.people.azg_violations > 0 {
        top_risks.push(format!(
            "Compliance: {} uncleared Austrian AZG working-time violation(s).",
            snap.people.azg_violations
        ));
    }
    if snap.ops_risk.at_risk_accounts > 0 {
        top_risks.push(format!(
            "Retention: {} account(s) sit in the red churn band.",
            snap.ops_risk.at_risk_accounts
        ));
    }
    if snap.ops_risk.blocked_issues > 0 {
        top_risks.push(format!(
            "Delivery: {} open issue(s) are blocked by another issue.",
            snap.ops_risk.blocked_issues
        ));
    }
    if snap.ops_risk.expiring_contracts_90d > 0 {
        top_risks.push(format!(
            "Contracts: {} agreement(s) renew or lapse within 90 days.",
            snap.ops_risk.expiring_contracts_90d
        ));
    }
    if snap.ops_risk.sla_breaches > 0 {
        top_risks.push(format!(
            "Support: {} ticket(s) breached first-response SLA.",
            snap.ops_risk.sla_breaches
        ));
    }
    if t.cash_configured && t.runway_months > 0.0 && t.runway_months < 6.0 {
        top_risks.push(format!(
            "Runway: {:.1} months is below the six-month floor.",
            t.runway_months
        ));
    }

    let mut key_recommendations = Vec::new();
    if !t.cash_configured {
        key_recommendations
            .push("Configure the treasury cash balance so runway is measurable.".to_string());
    }
    if t.overdue_cents > 0 {
        key_recommendations.push(
            "Run the dunning batch in BillCraft and record collections against open invoices."
                .to_string(),
        );
    }
    if snap.people.azg_violations > 0 {
        key_recommendations.push(
            "Clear the open TeamCraft AZG violations or record the compensating rest period."
                .to_string(),
        );
    }
    if snap.ops_risk.at_risk_accounts > 0 {
        key_recommendations.push(
            "Open the red-band accounts in GainCraft and assign a retention call-to-action."
                .to_string(),
        );
    }
    if snap.ops_risk.blocked_issues > 0 {
        key_recommendations.push(
            "Resolve the blocking issues in TaskCraft before the next delivery review.".to_string(),
        );
    }
    if snap.ops_risk.expiring_contracts_90d > 0 {
        key_recommendations.push(
            "Start renewal conversations for agreements inside the 90-day window.".to_string(),
        );
    }
    if key_recommendations.is_empty() {
        key_recommendations
            .push("No escalations detected; keep the weekly telemetry review cadence.".to_string());
    }

    BoardBriefing {
        generated_at: chrono::Utc::now(),
        executive_summary: summary,
        snapshot: snap,
        top_risks,
        key_recommendations,
    }
}

pub fn cashflow_forecast(store: &HelmStore, months: u32) -> CashflowForecast {
    let months = months.clamp(1, 60);
    let snap = store.aggregate_live_fleet();
    let t = &snap.treasury;
    let mut cash = t.cash_cents;
    let mut out = Vec::new();
    let mut zero_cash_month = None;
    for month in 1..=months {
        let inflow = t.mrr_cents + if month == 1 { t.receivables_cents } else { 0 };
        let outflow = t.burn_monthly_cents;
        let net = inflow - outflow;
        cash += net;
        if cash < 0 && zero_cash_month.is_none() {
            zero_cash_month = Some(month);
        }
        out.push(ForecastMonth {
            month,
            inflow_cents: inflow,
            outflow_cents: outflow,
            net_cents: net,
            cash_cents: cash,
            insolvent: cash < 0,
        });
    }
    let mut notes = vec![
        "Scenario only: MRR is held flat and monthly burn is assumed constant.".to_string(),
        "All open receivables are assumed collected in month 1.".to_string(),
    ];
    if !t.cash_configured {
        notes.push("Treasury cash is not configured; projection starts from zero.".to_string());
    }
    CashflowForecast {
        start_cash_cents: t.cash_cents,
        cash_configured: t.cash_configured,
        monthly_burn_cents: t.burn_monthly_cents,
        monthly_mrr_cents: t.mrr_cents,
        zero_cash_month,
        months: out,
        notes,
    }
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
        generate_board_briefing(&self.store)
    }

    pub fn cashflow_forecast(&self, months: u32) -> CashflowForecast {
        cashflow_forecast(&self.store, months)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn briefing_is_generated_from_store() {
        let store = HelmStore::open_in_memory().unwrap();
        let b = generate_board_briefing(&store);
        assert!(!b.executive_summary.is_empty());
        assert!(!b.key_recommendations.is_empty());
        let md = b.to_markdown();
        assert!(md.contains("# HelmCraft Board Briefing"));
        assert!(md.contains("## Telemetry provenance"));
    }

    #[test]
    fn forecast_respects_month_count() {
        let store = HelmStore::open_in_memory().unwrap();
        let f = cashflow_forecast(&store, 12);
        assert_eq!(f.months.len(), 12);
        assert_eq!(f.months[0].month, 1);
        assert_eq!(f.months[11].month, 12);
    }

    #[test]
    fn service_delegates_to_free_functions() {
        let store = HelmStore::open_in_memory().unwrap();
        let service = HelmService::new(store, None);
        assert_eq!(service.cashflow_forecast(3).months.len(), 3);
        assert!(service.store().source_health().len() >= 10);
    }
}
