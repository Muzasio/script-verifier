//! SQLite-backed implementation of the orchestrator's `Store` trait.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::models::{Claim, Correction, Evidence, RunRecord, ScriptRecord, VerificationRun};

#[derive(Debug)]
pub enum StoreError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    MissingId(&'static str),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            StoreError::Json(e) => write!(f, "json error: {e}"),
            StoreError::MissingId(what) => write!(f, "missing id: {what}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Sqlite(e) => Some(e),
            StoreError::Json(e) => Some(e),
            StoreError::MissingId(_) => None,
        }
    }
}

impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self { StoreError::Sqlite(e) }
}
impl From<serde_json::Error> for StoreError {
    fn from(e: serde_json::Error) -> Self { StoreError::Json(e) }
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS scripts (id TEXT PRIMARY KEY, original_text TEXT, created_at TEXT);
CREATE TABLE IF NOT EXISTS claims (id TEXT PRIMARY KEY, script_id TEXT, text TEXT, original_wording TEXT, context TEXT, status TEXT);
CREATE TABLE IF NOT EXISTS evidence (id TEXT PRIMARY KEY, claim_id TEXT, source_url TEXT, source_title TEXT, excerpt TEXT, source_tier TEXT, verdict TEXT, provider TEXT);
CREATE TABLE IF NOT EXISTS verification_runs (id TEXT PRIMARY KEY, claim_id TEXT, stage TEXT, provider TEXT, input_prompt TEXT, raw_response TEXT, timestamp TEXT);
CREATE TABLE IF NOT EXISTS corrections (id TEXT PRIMARY KEY, claim_id TEXT, original_text TEXT, corrected_text TEXT, reason TEXT, evidence_ids TEXT);
CREATE TABLE IF NOT EXISTS runs (id TEXT PRIMARY KEY, script_id TEXT, status TEXT, started_at TEXT, finished_at TEXT);
"#;

pub struct SqliteStore {
    conn: Mutex<Connection>,
}

impl SqliteStore {
    pub fn open(path: &str) -> Result<Self, StoreError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        Self::open(":memory:")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("store mutex poisoned")
    }

    pub fn insert_script(&self, id: &str, original_text: &str, created_at: &str) -> Result<(), StoreError> {
        self.lock().execute(
            "INSERT OR REPLACE INTO scripts (id, original_text, created_at) VALUES (?1, ?2, ?3)",
            params![id, original_text, created_at],
        )?;
        Ok(())
    }

    pub fn get_script(&self, id: &str) -> Result<Option<ScriptRecord>, StoreError> {
        let conn = self.lock();
        let rec = conn.query_row(
            "SELECT id, original_text, created_at FROM scripts WHERE id = ?1",
            params![id],
            |row| Ok(ScriptRecord { id: row.get(0)?, original_text: row.get(1)?, created_at: row.get(2)? }),
        ).optional()?;
        Ok(rec)
    }

    pub fn list_scripts(&self) -> Result<Vec<ScriptRecord>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, original_text, created_at FROM scripts ORDER BY rowid")?;
        let rows = stmt.query_map([], |row| Ok(ScriptRecord { id: row.get(0)?, original_text: row.get(1)?, created_at: row.get(2)? }))?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    pub fn insert_claim(&self, script_id: &str, c: &Claim) -> Result<(), StoreError> {
        if c.id.trim().is_empty() { return Err(StoreError::MissingId("claim.id")); }
        self.lock().execute(
            "INSERT OR REPLACE INTO claims (id, script_id, text, original_wording, context, status) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![c.id, script_id, c.text, c.original_wording, c.context, c.status],
        )?;
        Ok(())
    }

    pub fn update_claim_status(&self, id: &str, status: &str) -> Result<(), StoreError> {
        self.lock().execute("UPDATE claims SET status = ?1 WHERE id = ?2", params![status, id])?;
        Ok(())
    }

    pub fn get_claims_by_script(&self, script_id: &str) -> Result<Vec<Claim>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, text, original_wording, context, status FROM claims WHERE script_id = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map(params![script_id], row_to_claim)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    pub fn get_claim(&self, id: &str) -> Result<Option<Claim>, StoreError> {
        let conn = self.lock();
        let rec = conn.query_row(
            "SELECT id, text, original_wording, context, status FROM claims WHERE id = ?1",
            params![id], row_to_claim,
        ).optional()?;
        Ok(rec)
    }

    pub fn insert_evidence(&self, e: &Evidence) -> Result<String, StoreError> {
        let id = match e.id.as_deref() {
            Some(s) if !s.trim().is_empty() => s.to_string(),
            _ => new_id("ev"),
        };
        self.lock().execute(
            "INSERT OR REPLACE INTO evidence (id, claim_id, source_url, source_title, excerpt, source_tier, verdict, provider) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, e.claim_id, e.source_url, e.source_title, e.excerpt, e.source_tier, e.verdict, e.provider],
        )?;
        Ok(id)
    }

    pub fn get_evidence_by_claim(&self, claim_id: &str) -> Result<Vec<Evidence>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, claim_id, source_url, source_title, excerpt, source_tier, verdict, provider FROM evidence WHERE claim_id = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map(params![claim_id], row_to_evidence)?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    pub fn get_evidence(&self, id: &str) -> Result<Option<Evidence>, StoreError> {
        let conn = self.lock();
        let rec = conn.query_row(
            "SELECT id, claim_id, source_url, source_title, excerpt, source_tier, verdict, provider FROM evidence WHERE id = ?1",
            params![id], row_to_evidence,
        ).optional()?;
        Ok(rec)
    }

    pub fn insert_verification_run(&self, r: &VerificationRun) -> Result<(), StoreError> {
        if r.id.trim().is_empty() { return Err(StoreError::MissingId("verification_run.id")); }
        self.lock().execute(
            "INSERT OR REPLACE INTO verification_runs (id, claim_id, stage, provider, input_prompt, raw_response, timestamp) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![r.id, r.claim_id, r.stage, r.provider, r.input_prompt, r.raw_response, r.timestamp],
        )?;
        Ok(())
    }

    pub fn get_runs_by_claim(&self, claim_id: &str) -> Result<Vec<VerificationRun>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, claim_id, stage, provider, input_prompt, raw_response, timestamp FROM verification_runs WHERE claim_id = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map(params![claim_id], |row| Ok(VerificationRun {
            id: row.get(0)?, claim_id: row.get(1)?, stage: row.get(2)?, provider: row.get(3)?,
            input_prompt: row.get(4)?, raw_response: row.get(5)?, timestamp: row.get(6)?,
        }))?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }

    pub fn get_verification_run(&self, id: &str) -> Result<Option<VerificationRun>, StoreError> {
        let conn = self.lock();
        let rec = conn.query_row(
            "SELECT id, claim_id, stage, provider, input_prompt, raw_response, timestamp FROM verification_runs WHERE id = ?1",
            params![id], |row| Ok(VerificationRun {
                id: row.get(0)?, claim_id: row.get(1)?, stage: row.get(2)?, provider: row.get(3)?,
                input_prompt: row.get(4)?, raw_response: row.get(5)?, timestamp: row.get(6)?,
            }),
        ).optional()?;
        Ok(rec)
    }

    pub fn insert_correction(&self, c: &Correction) -> Result<String, StoreError> {
        let id = new_id("corr");
        let evidence_ids_json = serialize_evidence_ids(&c.evidence_ids)?;
        self.lock().execute(
            "INSERT OR REPLACE INTO corrections (id, claim_id, original_text, corrected_text, reason, evidence_ids) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, c.claim_id, c.original_text, c.corrected_text, c.reason, evidence_ids_json],
        )?;
        Ok(id)
    }

    pub fn get_correction_by_claim(&self, claim_id: &str) -> Result<Option<Correction>, StoreError> {
        let conn = self.lock();
        let row = conn.query_row(
            "SELECT claim_id, original_text, corrected_text, reason, evidence_ids FROM corrections WHERE claim_id = ?1 ORDER BY rowid DESC LIMIT 1",
            params![claim_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)),
        ).optional()?;

        match row {
            None => Ok(None),
            Some((claim_id, original_text, corrected_text, reason, ids_json)) => {
                let evidence_ids = deserialize_evidence_ids(&ids_json)?;
                Ok(Some(Correction { claim_id, original_text, corrected_text, reason, evidence_ids }))
            }
        }
    }

    pub fn list_corrections(&self, claim_id: &str) -> Result<Vec<Correction>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT claim_id, original_text, corrected_text, reason, evidence_ids FROM corrections WHERE claim_id = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map(params![claim_id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?)))?;

        let mut out = Vec::new();
        for r in rows {
            let (claim_id, original_text, corrected_text, reason, ids_json) = r?;
            let evidence_ids = deserialize_evidence_ids(&ids_json)?;
            out.push(Correction { claim_id, original_text, corrected_text, reason, evidence_ids });
        }
        Ok(out)
    }

    pub fn insert_run(&self, r: &RunRecord) -> Result<(), StoreError> {
        if r.id.trim().is_empty() { return Err(StoreError::MissingId("run.id")); }
        self.lock().execute(
            "INSERT OR REPLACE INTO runs (id, script_id, status, started_at, finished_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![r.id, r.script_id, r.status, r.started_at, r.finished_at],
        )?;
        Ok(())
    }

    pub fn update_run_status(&self, id: &str, status: &str, finished_at: Option<&str>) -> Result<(), StoreError> {
        self.lock().execute("UPDATE runs SET status = ?1, finished_at = ?2 WHERE id = ?3", params![status, finished_at, id])?;
        Ok(())
    }

    pub fn get_run(&self, id: &str) -> Result<Option<RunRecord>, StoreError> {
        let conn = self.lock();
        let rec = conn.query_row(
            "SELECT id, script_id, status, started_at, finished_at FROM runs WHERE id = ?1",
            params![id], |row| Ok(RunRecord { id: row.get(0)?, script_id: row.get(1)?, status: row.get(2)?, started_at: row.get(3)?, finished_at: row.get(4)? }),
        ).optional()?;
        Ok(rec)
    }

    pub fn list_runs(&self, script_id: &str) -> Result<Vec<RunRecord>, StoreError> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT id, script_id, status, started_at, finished_at FROM runs WHERE script_id = ?1 ORDER BY rowid")?;
        let rows = stmt.query_map(params![script_id], |row| Ok(RunRecord { id: row.get(0)?, script_id: row.get(1)?, status: row.get(2)?, started_at: row.get(3)?, finished_at: row.get(4)? }))?;
        let mut out = Vec::new();
        for r in rows { out.push(r?); }
        Ok(out)
    }
}

fn row_to_claim(row: &rusqlite::Row<'_>) -> rusqlite::Result<Claim> {
    Ok(Claim { id: row.get(0)?, text: row.get(1)?, original_wording: row.get(2)?, context: row.get(3)?, status: row.get(4)? })
}

fn row_to_evidence(row: &rusqlite::Row<'_>) -> rusqlite::Result<Evidence> {
    Ok(Evidence {
        id: Some(row.get(0)?), claim_id: row.get(1)?, source_url: row.get(2)?, source_title: row.get(3)?,
        excerpt: row.get(4)?, source_tier: row.get(5)?, verdict: row.get(6)?, provider: row.get(7)?,
    })
}

use crate::traits::Store;

impl Store for SqliteStore {
    fn save_script(&self, id: &str, original_text: &str, created_at: &str) {
        if let Err(e) = SqliteStore::insert_script(self, id, original_text, created_at) {
            tracing::error!(error = %e, "save_script failed");
        }
    }
    fn get_script(&self, id: &str) -> Option<ScriptRecord> {
        match SqliteStore::get_script(self, id) { Ok(v) => v, Err(e) => { tracing::error!(error = %e, "get_script failed"); None } }
    }
    fn list_scripts(&self) -> Vec<ScriptRecord> {
        SqliteStore::list_scripts(self).unwrap_or_else(|e| { tracing::error!(error = %e, "list_scripts failed"); Vec::new() })
    }

    fn save_claim(&self, script_id: &str, c: &Claim) {
        if let Err(e) = SqliteStore::insert_claim(self, script_id, c) {
            tracing::error!(error = %e, claim_id = %c.id, "save_claim failed");
        }
    }
    fn get_claim(&self, id: &str) -> Option<Claim> {
        match SqliteStore::get_claim(self, id) { Ok(v) => v, Err(e) => { tracing::error!(error = %e, "get_claim failed"); None } }
    }
    fn list_claims(&self, script_id: &str) -> Vec<Claim> {
        SqliteStore::get_claims_by_script(self, script_id).unwrap_or_else(|e| { tracing::error!(error = %e, "list_claims failed"); Vec::new() })
    }

    fn save_evidence(&self, e: &Evidence) {
        if let Err(err) = SqliteStore::insert_evidence(self, e) {
            tracing::error!(error = %err, claim_id = %e.claim_id, "save_evidence failed");
        }
    }
    fn get_evidence(&self, id: &str) -> Option<Evidence> {
        match SqliteStore::get_evidence(self, id) { Ok(v) => v, Err(e) => { tracing::error!(error = %e, "get_evidence failed"); None } }
    }
    fn list_evidence(&self, claim_id: &str) -> Vec<Evidence> {
        SqliteStore::get_evidence_by_claim(self, claim_id).unwrap_or_else(|e| { tracing::error!(error = %e, "list_evidence failed"); Vec::new() })
    }

    fn save_verification_run(&self, r: &VerificationRun) {
        if let Err(e) = SqliteStore::insert_verification_run(self, r) { tracing::error!(error = %e, "save_verification_run failed"); }
    }
    fn get_verification_run(&self, id: &str) -> Option<VerificationRun> {
        match SqliteStore::get_verification_run(self, id) { Ok(v) => v, Err(e) => { tracing::error!(error = %e, "get_verification_run failed"); None } }
    }
    fn list_verification_runs(&self, claim_id: &str) -> Vec<VerificationRun> {
        SqliteStore::get_runs_by_claim(self, claim_id).unwrap_or_else(|e| { tracing::error!(error = %e, "list_verification_runs failed"); Vec::new() })
    }

    fn save_correction(&self, c: &Correction) {
        if let Err(e) = SqliteStore::insert_correction(self, c) { tracing::error!(error = %e, claim_id = %c.claim_id, "save_correction failed"); }
    }
    fn list_corrections(&self, claim_id: &str) -> Vec<Correction> {
        SqliteStore::list_corrections(self, claim_id).unwrap_or_else(|e| { tracing::error!(error = %e, "list_corrections failed"); Vec::new() })
    }

    fn save_run(&self, r: &RunRecord) {
        if let Err(e) = SqliteStore::insert_run(self, r) { tracing::error!(error = %e, "save_run failed"); }
    }
    fn get_run(&self, id: &str) -> Option<RunRecord> {
        match SqliteStore::get_run(self, id) { Ok(v) => v, Err(e) => { tracing::error!(error = %e, "get_run failed"); None } }
    }
    fn list_runs(&self, script_id: &str) -> Vec<RunRecord> {
        SqliteStore::list_runs(self, script_id).unwrap_or_else(|e| { tracing::error!(error = %e, "list_runs failed"); Vec::new() })
    }
    fn update_run_status(&self, id: &str, status: &str, finished_at: Option<&str>) {
        if let Err(e) = SqliteStore::update_run_status(self, id, status, finished_at) {
            tracing::error!(error = %e, run_id = %id, "update_run_status failed");
        }
    }
}

pub fn serialize_evidence_ids(ids: &[String]) -> Result<String, StoreError> {
    Ok(serde_json::to_string(ids)?)
}

pub fn deserialize_evidence_ids(s: &str) -> Result<Vec<String>, StoreError> {
    if s.trim().is_empty() { return Ok(Vec::new()); }
    Ok(serde_json::from_str(s)?)
}

static ID_SEQ: AtomicU64 = AtomicU64::new(0);

fn new_id(prefix: &str) -> String {
    let n = ID_SEQ.fetch_add(1, Ordering::SeqCst);
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_micros() as u64).unwrap_or(0);
    format!("{prefix}-{t:x}{n:04x}")
}
