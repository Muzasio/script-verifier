//! Per-run append-only log. One file per verification run, written
//! directly by the orchestrator (not through tracing), so a run's
//! complete history lives in a single file:
//!
//!   logs/runs/20260921-070002_s-1a0c2c3bcc100.log
//!
//! Format: ISO-8601 timestamp, two spaces, message. Greppable.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct RunLog {
    path: PathBuf,
    file: Mutex<File>,
}

impl RunLog {
    /// Creates `logs/runs/<date>_<script_id>.log`. Never fails the
    /// pipeline: on error the caller gets `None` and continues without
    /// a per-run file (the daily logs still capture everything).
    pub fn create(script_id: &str) -> Option<Self> {
        let dir = Path::new("logs").join("runs");
        if let Err(e) = fs::create_dir_all(&dir) {
            tracing::warn!(error = %e, "run_log: could not create logs/runs/");
            return None;
        }
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("{stamp}_{script_id}.txt"));
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => Some(Self {
                path,
                file: Mutex::new(file),
            }),
            Err(e) => {
                tracing::warn!(error = %e, path = %path.display(), "run_log: open failed");
                None
            }
        }
    }

    /// Appends one line. Failures are logged once and then ignored —
    /// losing a per-run file must not fail a verification.
    pub fn write(&self, line: &str) {
        let ts = chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        if let Ok(mut f) = self.file.lock() {
            if let Err(e) = writeln!(f, "{ts}  {line}") {
                tracing::warn!(error = %e, "run_log: write failed");
            }
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}
