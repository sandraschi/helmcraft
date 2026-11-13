#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum HelmStoreError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreasuryMetrics {
    pub cash_cents: i64,
    /// True when treasury cash is the closing balance of a real BuchCraft bank
    /// account, or a setting when that balance is absent. A demo ledger does
    /// not count.
    pub cash_configured: bool,
    pub burn_monthly_cents: i64,
    pub burn_payroll_cents: i64,
    pub burn_expenses_cents: i64,
    pub runway_months: f64,
    pub mrr_cents: i64,
    pub arr_cents: i64,
    pub receivables_cents: i64,
    pub overdue_cents: i64,
    /// Net movement over the imported bank transactions (not a balance).
    pub net_bank_flow_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeopleMetrics {
    pub headcount_fte: usize,
    pub open_requisitions: usize,
    pub active_candidates: usize,
    pub pending_offers: usize,
    pub azg_violations: usize,
    pub unassigned_assets: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpsRiskMetrics {
    pub active_issues: usize,
    pub blocked_issues: usize,
    pub open_tickets: usize,
    pub sla_breaches: usize,
    pub at_risk_accounts: usize,
    pub expiring_contracts_90d: usize,
    pub csat_nps_score: f64,
    pub csat_nps_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutiveSnapshot {
    pub timestamp: DateTime<Utc>,
    pub treasury: TreasuryMetrics,
    pub people: PeopleMetrics,
    pub ops_risk: OpsRiskMetrics,
    pub action_items: Vec<String>,
    /// Human-readable provenance notes (missing sources, unset cash...).
    pub data_notes: Vec<String>,
    pub sources_read: usize,
    pub sources_total: usize,
}

/// Health of one local craft database read by the cockpit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceHealth {
    pub app: String,
    pub table: String,
    pub path: String,
    pub exists: bool,
    pub rows: i64,
    /// ok | empty | missing | error
    pub status: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmployeeRow {
    pub code: String,
    pub name: String,
    pub weekly_hours: f64,
    pub monthly_gross_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViolationRow {
    pub employee: String,
    pub date: String,
    pub kind: String,
    pub severity: String,
    pub minutes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtRiskAccount {
    pub key: String,
    pub name: String,
    pub arr_cents: i64,
    pub score: i64,
    pub band: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockedIssue {
    pub number: i64,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub assignee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRow {
    pub customer: String,
    pub due_date: String,
    pub gross_cents: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenewalRow {
    pub name: String,
    pub key: String,
    pub arr_cents: i64,
    pub term_end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRecord {
    pub id: i64,
    pub saved_at: String,
    pub snapshot: ExecutiveSnapshot,
}

/// Source database names and the table reported on the Data Sources page.
const SOURCES: [(&str, &str); 11] = [
    ("billcraft", "invoices"),
    ("spendcraft", "expense_claims"),
    ("teamcraft", "employees"),
    ("hirecraft", "candidates"),
    ("gaincraft", "accounts"),
    ("taskcraft", "issues"),
    ("assetcraft", "assets"),
    ("contractcraft", "contracts"),
    ("deskcraft", "tickets"),
    ("buchcraft", "entries"),
    ("formcraft", "responses"),
];

pub fn data_root() -> PathBuf {
    if let Ok(root) = std::env::var("HELMCRAFT_DATA_ROOT") {
        let p = root.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub fn source_path(app: &str) -> PathBuf {
    data_root().join(app).join(format!("{app}.db"))
}

fn read_only(app: &str) -> Option<Connection> {
    let path = source_path(app);
    if !path.is_file() {
        return None;
    }
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()
}

fn count(conn: &Connection, sql: &str) -> Option<i64> {
    conn.query_row(sql, [], |r| r.get::<_, i64>(0)).ok()
}

fn count_f(conn: &Connection, sql: &str) -> Option<f64> {
    conn.query_row(sql, [], |r| r.get::<_, f64>(0)).ok()
}

enum BankBooks {
    MissingFile,
    NoLines,
    Unreadable,
    Ready {
        account: u32,
        cents: i64,
        demo: bool,
    },
}

fn table_exists(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![name],
        |_| Ok(1i64),
    )
    .is_ok()
}

/// Closing balance of BuchCraft's named bank account. Default account 2800,
/// or the `bank_account` meta value. This is the sum of ledger lines, which
/// is the same figure `account_ledger` carries forward. `bank_tx` is not read.
fn bank_books(conn: &Connection) -> BankBooks {
    if !table_exists(conn, "lines") || !table_exists(conn, "entries") {
        return BankBooks::NoLines;
    }
    let account = if table_exists(conn, "meta") {
        match conn.query_row(
            "SELECT value FROM meta WHERE key = 'bank_account'",
            [],
            |r| r.get::<_, String>(0),
        ) {
            Ok(value) => match value.parse::<u32>() {
                Ok(number) if (0..=9999).contains(&number) => number,
                _ => return BankBooks::Unreadable,
            },
            Err(rusqlite::Error::QueryReturnedNoRows) => 2800,
            Err(_) => return BankBooks::Unreadable,
        }
    } else {
        2800
    };
    let cents = match conn.query_row(
        "SELECT COALESCE(SUM(l.amount_cents), 0)
         FROM lines l
         JOIN entries e ON e.id = l.entry_id
         WHERE l.account = ?1",
        params![i64::from(account)],
        |r| r.get::<_, i64>(0),
    ) {
        Ok(cents) => cents,
        Err(_) => return BankBooks::Unreadable,
    };
    let demo = table_exists(conn, "meta")
        && conn
            .query_row("SELECT value FROM meta WHERE key = 'demo'", [], |r| {
                r.get::<_, String>(0)
            })
            .ok()
            .as_deref()
            == Some("1");
    BankBooks::Ready {
        account,
        cents,
        demo,
    }
}

fn fallback_cash(configured: Option<i64>) -> (i64, bool) {
    match configured {
        Some(cents) => (cents, true),
        None => (0, false),
    }
}

pub struct HelmStore {
    conn: Connection,
}

impl HelmStore {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, HelmStoreError> {
        let conn = Connection::open(path)?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self, HelmStoreError> {
        let conn = Connection::open_in_memory()?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), HelmStoreError> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS executive_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                snapshot_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn save_snapshot(&self, snap: &ExecutiveSnapshot) -> Result<i64, HelmStoreError> {
        let json = serde_json::to_string(snap)?;
        self.conn.execute(
            "INSERT INTO executive_snapshots (snapshot_json, created_at) VALUES (?1, ?2)",
            params![json, snap.timestamp.to_rfc3339()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn list_snapshots(&self, limit: usize) -> Result<Vec<SnapshotRecord>, HelmStoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, snapshot_json, created_at
             FROM executive_snapshots ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, json, saved_at) = row?;
            let snapshot = serde_json::from_str(&json)?;
            out.push(SnapshotRecord {
                id,
                saved_at,
                snapshot,
            });
        }
        Ok(out)
    }

    pub fn snapshot_count(&self) -> Result<i64, HelmStoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM executive_snapshots", [], |r| {
                r.get::<_, i64>(0)
            })?)
    }

    pub fn delete_snapshot(&self, id: i64) -> Result<bool, HelmStoreError> {
        let n = self
            .conn
            .execute("DELETE FROM executive_snapshots WHERE id = ?1", [id])?;
        Ok(n > 0)
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), HelmStoreError> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Treasury cash: env override, then the persisted setting. Never invented.
    pub fn configured_cash_cents(&self) -> Option<i64> {
        if let Ok(raw) = std::env::var("HELMCRAFT_CASH_CENTS") {
            if let Ok(v) = raw.trim().parse::<i64>() {
                return Some(v);
            }
        }
        self.get_setting("cash_cents")
            .and_then(|s| s.parse::<i64>().ok())
    }

    pub fn set_cash_cents(&self, cents: i64) -> Result<(), HelmStoreError> {
        self.set_setting("cash_cents", &cents.to_string())
    }

    /// Every source database, read-only, with its key-table row count.
    pub fn source_health(&self) -> Vec<SourceHealth> {
        SOURCES
            .iter()
            .map(|(app, table)| {
                let path = source_path(app);
                let exists = path.is_file();
                if !exists {
                    return SourceHealth {
                        app: (*app).into(),
                        table: (*table).into(),
                        path: path.display().to_string(),
                        exists,
                        rows: 0,
                        status: "missing".into(),
                        detail: "database not found".into(),
                    };
                }
                match read_only(app) {
                    Some(c) => {
                        let sql = format!("SELECT COUNT(*) FROM {table}");
                        match count(&c, &sql) {
                            Some(rows) => SourceHealth {
                                app: (*app).into(),
                                table: (*table).into(),
                                path: path.display().to_string(),
                                exists,
                                rows,
                                status: if rows == 0 { "empty" } else { "ok" }.into(),
                                detail: if rows == 0 {
                                    format!("connected; {table} is empty")
                                } else {
                                    format!("connected; {rows} row(s) in {table}")
                                },
                            },
                            None => SourceHealth {
                                app: (*app).into(),
                                table: (*table).into(),
                                path: path.display().to_string(),
                                exists,
                                rows: 0,
                                status: "error".into(),
                                detail: format!("cannot read table {table}"),
                            },
                        }
                    }
                    None => SourceHealth {
                        app: (*app).into(),
                        table: (*table).into(),
                        path: path.display().to_string(),
                        exists,
                        rows: 0,
                        status: "error".into(),
                        detail: "database cannot be opened read-only".into(),
                    },
                }
            })
            .collect()
    }

    pub fn employees(&self) -> Result<Vec<EmployeeRow>, HelmStoreError> {
        let Some(c) = read_only("teamcraft") else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT code, name, weekly_minutes, monthly_gross_cents
             FROM employees WHERE active = 1 ORDER BY name",
        )?;
        let rows = stmt.query_map([], |r| {
            let minutes: i64 = r.get(2)?;
            Ok(EmployeeRow {
                code: r.get(0)?,
                name: r.get(1)?,
                weekly_hours: minutes as f64 / 60.0,
                monthly_gross_cents: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn violations(&self) -> Result<Vec<ViolationRow>, HelmStoreError> {
        let Some(c) = read_only("teamcraft") else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT COALESCE(e.name, v.employee_id), v.date, v.kind, v.severity, v.minutes
             FROM violations v LEFT JOIN employees e ON e.id = v.employee_id
             WHERE v.cleared_at IS NULL ORDER BY v.date DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(ViolationRow {
                employee: r.get(0)?,
                date: r.get(1)?,
                kind: r.get(2)?,
                severity: r.get(3)?,
                minutes: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn at_risk_accounts(&self) -> Result<Vec<AtRiskAccount>, HelmStoreError> {
        let Some(c) = read_only("gaincraft") else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT a.key, a.name, a.arr_cents, s.score, s.band
             FROM accounts a
             JOIN snapshots s ON s.account_id = a.id
             WHERE a.active = 1
               AND s.date = (SELECT MAX(s2.date) FROM snapshots s2 WHERE s2.account_id = a.id)
               AND (s.band = 'red' OR s.score < 40)
             ORDER BY s.score ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(AtRiskAccount {
                key: r.get(0)?,
                name: r.get(1)?,
                arr_cents: r.get(2)?,
                score: r.get(3)?,
                band: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn blocked_issues(&self) -> Result<Vec<BlockedIssue>, HelmStoreError> {
        let Some(c) = read_only("taskcraft") else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT t.number, t.title, t.status, t.priority, COALESCE(t.assignee, '')
             FROM issues t
             WHERE t.status IN ('todo', 'in_progress', 'in_review')
               AND t.id IN (SELECT target_id FROM relations WHERE kind = 'blocks')
             ORDER BY t.number",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(BlockedIssue {
                number: r.get(0)?,
                title: r.get(1)?,
                status: r.get(2)?,
                priority: r.get(3)?,
                assignee: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn open_invoices(&self) -> Result<Vec<InvoiceRow>, HelmStoreError> {
        let Some(c) = read_only("billcraft") else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT customer_name, due_date, gross_cents, status
             FROM invoices
             WHERE status IN ('issued', 'overdue')
             ORDER BY due_date ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(InvoiceRow {
                customer: r.get(0)?,
                due_date: r.get(1)?,
                gross_cents: r.get(2)?,
                status: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn upcoming_renewals(&self) -> Result<Vec<RenewalRow>, HelmStoreError> {
        let Some(c) = read_only("gaincraft") else {
            return Ok(Vec::new());
        };
        let mut stmt = c.prepare(
            "SELECT name, key, arr_cents, COALESCE(term_end, '')
             FROM accounts
             WHERE active = 1 AND term_end IS NOT NULL AND term_end <> ''
               AND term_end BETWEEN date('now') AND date('now', '+90 day')
             ORDER BY term_end ASC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(RenewalRow {
                name: r.get(0)?,
                key: r.get(1)?,
                arr_cents: r.get(2)?,
                term_end: r.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Aggregates live fleet state from the local craft databases.
    ///
    /// No value is fabricated: a missing source contributes zero and a note;
    /// treasury cash is zero with `cash_configured = false` until it is set.
    pub fn aggregate_live_fleet(&self) -> ExecutiveSnapshot {
        let mut notes = Vec::new();
        let mut read = 0usize;

        let mut receivables_cents = 0i64;
        let mut overdue_cents = 0i64;
        let mut mrr_bill_cents = 0i64;
        if let Some(c) = read_only("billcraft") {
            read += 1;
            receivables_cents = count(
                &c,
                "SELECT COALESCE(SUM(gross_cents), 0) FROM invoices WHERE status = 'issued'",
            )
            .unwrap_or(0);
            overdue_cents = count(
                &c,
                "SELECT COALESCE(SUM(gross_cents), 0) FROM invoices
                 WHERE status = 'overdue' OR (status = 'issued' AND due_date < date('now'))",
            )
            .unwrap_or(0);
            mrr_bill_cents = count(
                &c,
                "SELECT COALESCE(SUM(CASE WHEN interval_months > 0
                        THEN amount_cents / interval_months
                        ELSE amount_cents END), 0)
                 FROM subscriptions WHERE status = 'active'",
            )
            .unwrap_or(0);
        } else {
            notes.push("billcraft.db not found: receivables and overdue read as 0".to_string());
        }

        let mut payroll_cents = 0i64;
        let mut headcount_fte = 0usize;
        let mut azg_violations = 0usize;
        if let Some(c) = read_only("teamcraft") {
            read += 1;
            payroll_cents = count(
                &c,
                "SELECT COALESCE(SUM(monthly_gross_cents), 0) FROM employees WHERE active = 1",
            )
            .unwrap_or(0);
            headcount_fte = count(&c, "SELECT COUNT(*) FROM employees WHERE active = 1")
                .unwrap_or(0)
                .max(0) as usize;
            azg_violations = count(
                &c,
                "SELECT COUNT(*) FROM violations WHERE cleared_at IS NULL",
            )
            .unwrap_or(0)
            .max(0) as usize;
        } else {
            notes.push("teamcraft.db not found: payroll, headcount and AZG read as 0".to_string());
        }

        let mut open_reqs = 0usize;
        let mut active_candidates = 0usize;
        let mut pending_offers = 0usize;
        if let Some(c) = read_only("hirecraft") {
            read += 1;
            open_reqs = count(
                &c,
                "SELECT COUNT(*) FROM job_requisitions WHERE status = 'open'",
            )
            .unwrap_or(0)
            .max(0) as usize;
            active_candidates = count(
                &c,
                "SELECT COUNT(*) FROM candidates
                 WHERE stage IN ('applied', 'screening', 'interview')",
            )
            .unwrap_or(0)
            .max(0) as usize;
            pending_offers = count(&c, "SELECT COUNT(*) FROM candidates WHERE stage = 'offer'")
                .unwrap_or(0)
                .max(0) as usize;
        } else {
            notes.push("hirecraft.db not found: recruiting funnel reads 0".to_string());
        }

        let mut arr_cents = 0i64;
        let mut at_risk_accounts = 0usize;
        if let Some(c) = read_only("gaincraft") {
            read += 1;
            arr_cents = count(
                &c,
                "SELECT COALESCE(SUM(arr_cents), 0) FROM accounts WHERE active = 1",
            )
            .unwrap_or(0);
            at_risk_accounts = count(
                &c,
                "SELECT COUNT(*) FROM accounts a
                 JOIN snapshots s ON s.account_id = a.id
                 WHERE a.active = 1
                   AND s.date = (SELECT MAX(s2.date) FROM snapshots s2 WHERE s2.account_id = a.id)
                   AND (s.band = 'red' OR s.score < 40)",
            )
            .unwrap_or(0)
            .max(0) as usize;
        } else {
            notes.push("gaincraft.db not found: ARR and churn risk read as 0".to_string());
        }

        let mut active_issues = 0usize;
        let mut blocked_issues = 0usize;
        if let Some(c) = read_only("taskcraft") {
            read += 1;
            active_issues = count(
                &c,
                "SELECT COUNT(*) FROM issues
                 WHERE status IN ('todo', 'in_progress', 'in_review')",
            )
            .unwrap_or(0)
            .max(0) as usize;
            blocked_issues = count(
                &c,
                "SELECT COUNT(DISTINCT t.id) FROM relations r
                 JOIN issues t ON t.id = r.target_id
                 WHERE r.kind = 'blocks'
                   AND t.status IN ('todo', 'in_progress', 'in_review')",
            )
            .unwrap_or(0)
            .max(0) as usize;
        } else {
            notes.push("taskcraft.db not found: delivery risk reads 0".to_string());
        }

        let mut open_tickets = 0usize;
        let mut sla_breaches = 0usize;
        if let Some(c) = read_only("deskcraft") {
            read += 1;
            open_tickets = count(
                &c,
                "SELECT COUNT(*) FROM tickets WHERE status NOT IN ('solved', 'closed')",
            )
            .unwrap_or(0)
            .max(0) as usize;
            sla_breaches = count(
                &c,
                "SELECT COUNT(*) FROM tickets
                 WHERE status NOT IN ('solved', 'closed')
                   AND first_response_at IS NULL
                   AND opened_at < datetime('now', '-1 day')",
            )
            .unwrap_or(0)
            .max(0) as usize;
        } else {
            notes.push("deskcraft.db not found: support tickets read 0".to_string());
        }

        let mut expenses_cents = 0i64;
        if let Some(c) = read_only("spendcraft") {
            read += 1;
            let spent = count(
                &c,
                "SELECT COALESCE(SUM(amount_cents), 0) FROM expense_claims
                 WHERE status IN ('approved', 'reimbursed')
                   AND claim_date >= date('now', '-90 day')",
            )
            .unwrap_or(0);
            expenses_cents = spent / 3;
        } else {
            notes.push("spendcraft.db not found: expense burn reads 0".to_string());
        }

        let mut unassigned_assets = 0usize;
        if let Some(c) = read_only("assetcraft") {
            read += 1;
            unassigned_assets = count(
                &c,
                "SELECT COUNT(*) FROM assets
                 WHERE disposed_on IS NULL
                   AND (holder IS NULL OR TRIM(holder) = '')",
            )
            .unwrap_or(0)
            .max(0) as usize;
        }

        let mut expiring_contracts_90d = 0usize;
        if let Some(c) = read_only("contractcraft") {
            read += 1;
            expiring_contracts_90d = count(
                &c,
                "SELECT COUNT(*) FROM contracts
                 WHERE status = 'active'
                   AND date(start_date, '+' || COALESCE(initial_months, 12) || ' months')
                       BETWEEN date('now') AND date('now', '+90 day')",
            )
            .unwrap_or(0)
            .max(0) as usize;
        }

        let mut net_bank_flow_cents = 0i64;
        let books = if let Some(c) = read_only("buchcraft") {
            read += 1;
            net_bank_flow_cents =
                count(&c, "SELECT COALESCE(SUM(amount_cents), 0) FROM bank_tx").unwrap_or(0);
            bank_books(&c)
        } else {
            BankBooks::MissingFile
        };

        let mut csat_nps_available = false;
        let mut csat_nps_score = 0.0f64;
        if let Some(c) = read_only("formcraft") {
            read += 1;
            let n = count(
                &c,
                "SELECT COUNT(*) FROM responses WHERE nps_score IS NOT NULL",
            )
            .unwrap_or(0);
            if n > 0 {
                csat_nps_available = true;
                csat_nps_score = count_f(
                    &c,
                    "SELECT AVG(nps_score) FROM responses WHERE nps_score IS NOT NULL",
                )
                .unwrap_or(0.0);
            }
        }

        let configured = self.configured_cash_cents();
        let (cash_cents, cash_configured) = match books {
            BankBooks::Ready {
                account,
                cents,
                demo: false,
            } => {
                notes.push(format!(
                    "Treasury cash is the closing balance of BuchCraft account {account} ({cents} cents)."
                ));
                if configured.is_some() {
                    notes.push("The HelmCraft cash setting was not used.".to_string());
                }
                (cents, true)
            }
            BankBooks::Ready {
                account,
                cents,
                demo: true,
            } => {
                notes.push(format!(
                    "BuchCraft account {account} closing balance is {cents} cents on a demo ledger. It is not used as treasury cash."
                ));
                fallback_cash(configured)
            }
            BankBooks::NoLines => {
                notes.push(
                    "BuchCraft has no ledger lines. Treasury cash stays the setting. Net bank flow is movement of bank_tx, not a balance."
                        .to_string(),
                );
                fallback_cash(configured)
            }
            BankBooks::Unreadable => {
                notes.push(
                    "BuchCraft bank account could not be read. Treasury cash stays the setting."
                        .to_string(),
                );
                fallback_cash(configured)
            }
            BankBooks::MissingFile => {
                notes.push("buchcraft.db not found: bank balance and bank flow read 0".to_string());
                fallback_cash(configured)
            }
        };

        let mrr_cents = if mrr_bill_cents > 0 {
            mrr_bill_cents
        } else {
            arr_cents / 12
        };
        let arr_cents = if arr_cents > 0 {
            arr_cents
        } else {
            mrr_cents * 12
        };
        let burn_monthly_cents = payroll_cents + expenses_cents;
        let runway_months = if burn_monthly_cents > 0 && cash_cents > 0 {
            ((cash_cents as f64 / burn_monthly_cents as f64) * 10.0).round() / 10.0
        } else {
            0.0
        };

        if !cash_configured {
            notes.push(
                "Treasury cash is not configured; set it in Settings or HELMCRAFT_CASH_CENTS"
                    .to_string(),
            );
        }

        let mut action_items = Vec::new();
        if overdue_cents > 0 {
            action_items.push(format!(
                "[Treasury] €{:.2} overdue receivables awaiting dunning",
                overdue_cents as f64 / 100.0
            ));
        }
        if !cash_configured {
            action_items.push(
                "[Treasury] Set the treasury cash balance for a real runway figure".to_string(),
            );
        }
        if azg_violations > 0 {
            action_items.push(format!(
                "[Compliance] {azg_violations} uncleared AZG working-time violation(s)"
            ));
        }
        if at_risk_accounts > 0 {
            action_items.push(format!(
                "[Risk] {at_risk_accounts} account(s) in the red churn band"
            ));
        }
        if blocked_issues > 0 {
            action_items.push(format!(
                "[Risk] {blocked_issues} open issue(s) blocked by another issue"
            ));
        }
        if pending_offers > 0 {
            action_items.push(format!(
                "[People] {pending_offers} offer(s) awaiting candidate signature"
            ));
        }
        if expiring_contracts_90d > 0 {
            action_items.push(format!(
                "[Risk] {expiring_contracts_90d} contract(s) renew within 90 days"
            ));
        }
        if cash_configured && runway_months > 0.0 && runway_months < 6.0 {
            action_items.push(format!(
                "[Treasury] Runway is {runway_months:.1} months; below the six-month floor"
            ));
        }

        ExecutiveSnapshot {
            timestamp: Utc::now(),
            treasury: TreasuryMetrics {
                cash_cents,
                cash_configured,
                burn_monthly_cents,
                burn_payroll_cents: payroll_cents,
                burn_expenses_cents: expenses_cents,
                runway_months,
                mrr_cents,
                arr_cents,
                receivables_cents,
                overdue_cents,
                net_bank_flow_cents,
            },
            people: PeopleMetrics {
                headcount_fte,
                open_requisitions: open_reqs,
                active_candidates,
                pending_offers,
                azg_violations,
                unassigned_assets,
            },
            ops_risk: OpsRiskMetrics {
                active_issues,
                blocked_issues,
                open_tickets,
                sla_breaches,
                at_risk_accounts,
                expiring_contracts_90d,
                csat_nps_score,
                csat_nps_available,
            },
            action_items,
            data_notes: notes,
            sources_read: read,
            sources_total: SOURCES.len(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        match LOCK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    struct DataRootGuard {
        previous: Option<String>,
    }

    impl DataRootGuard {
        fn set(root: &Path) -> Self {
            let previous = std::env::var("HELMCRAFT_DATA_ROOT").ok();
            std::env::set_var("HELMCRAFT_DATA_ROOT", root);
            Self { previous }
        }
    }

    impl Drop for DataRootGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var("HELMCRAFT_DATA_ROOT", value),
                None => std::env::remove_var("HELMCRAFT_DATA_ROOT"),
            }
        }
    }

    fn write_source(root: &Path, app: &str, sql: &str) -> Result<(), String> {
        let dir = root.join(app);
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let conn = Connection::open(dir.join(format!("{app}.db"))).map_err(|e| e.to_string())?;
        conn.execute_batch(sql).map_err(|e| e.to_string())?;
        Ok(())
    }

    #[test]
    fn snapshot_roundtrip_and_settings() {
        let _lock = env_lock();
        let root = std::env::temp_dir().join(format!("helmcraft-empty-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let _guard = DataRootGuard::set(&root);
        let store = HelmStore::open_in_memory().unwrap();
        store.set_cash_cents(1_250_000).unwrap();
        assert_eq!(store.configured_cash_cents(), Some(1_250_000));
        let snap = store.aggregate_live_fleet();
        assert!(snap.treasury.cash_configured);
        assert_eq!(snap.treasury.cash_cents, 1_250_000);
        let id = store.save_snapshot(&snap).unwrap();
        let listed = store.list_snapshots(10).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, id);
        assert_eq!(store.snapshot_count().unwrap(), 1);
        assert!(store.delete_snapshot(id).unwrap());
        assert_eq!(store.snapshot_count().unwrap(), 0);
    }

    #[test]
    fn source_health_covers_every_source() {
        let _lock = env_lock();
        let store = HelmStore::open_in_memory().unwrap();
        let health = store.source_health();
        assert_eq!(health.len(), SOURCES.len());
        for row in &health {
            assert!(!row.app.is_empty());
            assert!(["ok", "empty", "missing", "error"].contains(&row.status.as_str()));
        }
    }

    #[test]
    fn snapshot_serializes_all_fields() {
        let _lock = env_lock();
        let store = HelmStore::open_in_memory().unwrap();
        let snap = store.aggregate_live_fleet();
        let json = serde_json::to_value(&snap).unwrap();
        assert!(json.get("treasury").is_some());
        assert!(json["treasury"].get("cash_configured").is_some());
        assert!(json.get("data_notes").is_some());
        assert_eq!(snap.sources_total, SOURCES.len());
    }

    #[test]
    fn live_fleet_reads_payroll_azg_spend_and_bank_flow() -> Result<(), String> {
        let _lock = env_lock();
        let root = std::env::temp_dir().join(format!("helmcraft-fleet-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let _guard = DataRootGuard::set(&root);

        write_source(
            &root,
            "teamcraft",
            "CREATE TABLE employees (monthly_gross_cents INTEGER NOT NULL, active INTEGER NOT NULL);
             INSERT INTO employees VALUES (300000, 1);
             INSERT INTO employees VALUES (999999, 0);
             CREATE TABLE violations (cleared_at TEXT);
             INSERT INTO violations VALUES (NULL);
             INSERT INTO violations VALUES ('2026-01-01');",
        )?;
        write_source(
            &root,
            "buchcraft",
            "CREATE TABLE bank_tx (amount_cents INTEGER NOT NULL);
             INSERT INTO bank_tx VALUES (10000);
             INSERT INTO bank_tx VALUES (-4000);",
        )?;
        write_source(
            &root,
            "spendcraft",
            "CREATE TABLE expense_claims (
                 amount_cents INTEGER NOT NULL,
                 status TEXT NOT NULL,
                 claim_date TEXT NOT NULL
             );
             INSERT INTO expense_claims VALUES (9000, 'approved', date('now'));
             INSERT INTO expense_claims VALUES (50000, 'draft', date('now'));",
        )?;

        let store = HelmStore::open_in_memory().map_err(|e| e.to_string())?;
        store.set_cash_cents(909_000).map_err(|e| e.to_string())?;
        let snap = store.aggregate_live_fleet();

        if snap.sources_read != 3 {
            return Err(format!("sources_read {}", snap.sources_read));
        }
        if snap.people.headcount_fte != 1 {
            return Err(format!("headcount {}", snap.people.headcount_fte));
        }
        if snap.people.azg_violations != 1 {
            return Err(format!("azg {}", snap.people.azg_violations));
        }
        if snap.treasury.burn_payroll_cents != 300_000 {
            return Err(format!("payroll {}", snap.treasury.burn_payroll_cents));
        }
        if snap.treasury.burn_expenses_cents != 3_000 {
            return Err(format!("expenses {}", snap.treasury.burn_expenses_cents));
        }
        if snap.treasury.burn_monthly_cents != 303_000 {
            return Err(format!("burn {}", snap.treasury.burn_monthly_cents));
        }
        if snap.treasury.net_bank_flow_cents != 6_000 {
            return Err(format!("bank {}", snap.treasury.net_bank_flow_cents));
        }
        if !snap.treasury.cash_configured || snap.treasury.cash_cents != 909_000 {
            return Err("cash setting was not the treasury figure".into());
        }
        if !snap
            .data_notes
            .iter()
            .any(|note| note.contains("no ledger lines"))
        {
            return Err("a bank_tx-only file was treated as a ledger".into());
        }
        if (snap.treasury.runway_months - 3.0).abs() > 0.05 {
            return Err(format!("runway {}", snap.treasury.runway_months));
        }
        if snap
            .data_notes
            .iter()
            .any(|note| note.contains("teamcraft.db not found"))
        {
            return Err("teamcraft was reported missing".into());
        }
        if !snap
            .data_notes
            .iter()
            .any(|note| note.contains("billcraft.db not found"))
        {
            return Err("a missing source was not noted".into());
        }

        drop(_guard);
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn bank_account_balance_is_cash_and_a_demo_ledger_is_not() -> Result<(), String> {
        let _lock = env_lock();
        let root = std::env::temp_dir().join(format!("helmcraft-bank-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        let _guard = DataRootGuard::set(&root);
        write_source(
            &root,
            "buchcraft",
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES ('bank_account', '2800');
             CREATE TABLE entries (
                 id INTEGER PRIMARY KEY,
                 entry_date TEXT NOT NULL,
                 kreis TEXT NOT NULL,
                 beleg_nr TEXT NOT NULL,
                 description TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 reverses_id INTEGER,
                 reversed_by_id INTEGER
             );
             INSERT INTO entries VALUES (1, '2026-01-02', 'BK', '1', 'Opening', '2026-01-02', NULL, NULL);
             CREATE TABLE lines (
                 id INTEGER PRIMARY KEY,
                 entry_id INTEGER NOT NULL,
                 account INTEGER NOT NULL,
                 amount_cents INTEGER NOT NULL,
                 tax_key TEXT,
                 is_tax INTEGER NOT NULL DEFAULT 0,
                 memo TEXT
             );
             INSERT INTO lines VALUES (1, 1, 2800, 50000, NULL, 0, NULL);
             INSERT INTO lines VALUES (2, 1, 3300, -50000, NULL, 0, NULL);
             CREATE TABLE bank_tx (amount_cents INTEGER NOT NULL);
             INSERT INTO bank_tx VALUES (999);",
        )?;
        let store = HelmStore::open_in_memory().map_err(|e| e.to_string())?;
        store.set_cash_cents(909_000).map_err(|e| e.to_string())?;
        let snap = store.aggregate_live_fleet();
        if snap.treasury.cash_cents != 50_000 || !snap.treasury.cash_configured {
            return Err(format!("ledger cash {}", snap.treasury.cash_cents));
        }
        if snap.treasury.net_bank_flow_cents != 999 {
            return Err(format!("flow {}", snap.treasury.net_bank_flow_cents));
        }
        if !snap
            .data_notes
            .iter()
            .any(|note| note.contains("account 2800") && note.contains("50000"))
        {
            return Err(format!("notes {:?}", snap.data_notes));
        }
        if !snap
            .data_notes
            .iter()
            .any(|note| note.contains("setting was not used"))
        {
            return Err("the setting should have been set aside".into());
        }

        let _ = fs::remove_file(root.join("buchcraft").join("buchcraft.db"));
        write_source(
            &root,
            "buchcraft",
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO meta VALUES ('bank_account', '2800');
             INSERT INTO meta VALUES ('demo', '1');
             CREATE TABLE entries (
                 id INTEGER PRIMARY KEY,
                 entry_date TEXT NOT NULL,
                 kreis TEXT NOT NULL,
                 beleg_nr TEXT NOT NULL,
                 description TEXT NOT NULL,
                 created_at TEXT NOT NULL,
                 reverses_id INTEGER,
                 reversed_by_id INTEGER
             );
             INSERT INTO entries VALUES (1, '2026-01-02', 'BK', '1', 'Opening', '2026-01-02', NULL, NULL);
             CREATE TABLE lines (
                 id INTEGER PRIMARY KEY,
                 entry_id INTEGER NOT NULL,
                 account INTEGER NOT NULL,
                 amount_cents INTEGER NOT NULL,
                 tax_key TEXT,
                 is_tax INTEGER NOT NULL DEFAULT 0,
                 memo TEXT
             );
             INSERT INTO lines VALUES (1, 1, 2800, 800000, NULL, 0, NULL);",
        )?;
        let demo = store.aggregate_live_fleet();
        if demo.treasury.cash_cents != 909_000 {
            return Err(format!("demo cash {}", demo.treasury.cash_cents));
        }
        if !demo
            .data_notes
            .iter()
            .any(|note| note.contains("demo ledger") && note.contains("800000"))
        {
            return Err(format!("demo notes {:?}", demo.data_notes));
        }
        drop(_guard);
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }
}
