//! Rust-side adapter for the Node.js browser worker (module 4).
//!
//! Spawns `node dist/main.js` with piped stdin/stdout/stderr, then speaks
//! the line-delimited JSON IPC contract from the shared block.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tracing::{debug, error, info, warn};

use crate::models::{WorkerRequest, WorkerResponse};
use crate::traits::BrowserWorker;

const REPLY_TIMEOUT_HEADROOM_MS: u64 = 5_000;

pub struct ProcessBrowserWorker {
    stdin_tx: SyncSender<WorkerRequest>,
    pending: Arc<Mutex<HashMap<String, SyncSender<WorkerResponse>>>>,
    child: Arc<Mutex<Child>>,
    reply_timeout: Duration,
    _writer: JoinHandle<()>,
    _reader: JoinHandle<()>,
    _stderr: JoinHandle<()>,
}

impl ProcessBrowserWorker {
    pub fn spawn(
        node_bin: &str,
        node_script: &str,
        cwd: &str,
        response_wait_ms: u64,
    ) -> std::io::Result<Self> {
        let mut child = Command::new(node_bin)
            .arg(node_script)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let pending: Arc<Mutex<HashMap<String, SyncSender<WorkerResponse>>>> = Arc::new(Mutex::new(HashMap::new()));

        let (stdin_tx, stdin_rx) = mpsc::sync_channel::<WorkerRequest>(256);

        let writer = spawn_writer(stdin, stdin_rx);
        let reader = spawn_reader(stdout, Arc::clone(&pending));
        let stderr_h = spawn_stderr_reader(stderr);

        info!(node = node_bin, script = node_script, "browser worker spawned");

        Ok(Self {
            stdin_tx,
            pending,
            child: Arc::new(Mutex::new(child)),
            reply_timeout: Duration::from_millis(response_wait_ms.saturating_add(REPLY_TIMEOUT_HEADROOM_MS)),
            _writer: writer,
            _reader: reader,
            _stderr: stderr_h,
        })
    }
}

impl BrowserWorker for ProcessBrowserWorker {
    fn send(&self, req: WorkerRequest) -> WorkerResponse {
        let req_id = req.req_id.clone();
        let (tx, rx) = mpsc::sync_channel::<WorkerResponse>(1);

        self.pending.lock().expect("pending map poisoned").insert(req_id.clone(), tx);

        if self.stdin_tx.send(req).is_err() {
            self.pending.lock().expect("pending map poisoned").remove(&req_id);
            return WorkerResponse::Error { req_id, message: "browser worker stdin is closed".into() };
        }

        match rx.recv_timeout(self.reply_timeout) {
            Ok(resp) => resp,
            Err(_) => {
                self.pending.lock().expect("pending map poisoned").remove(&req_id);
                warn!(req_id = %req_id, "worker reply timeout");
                WorkerResponse::Error { req_id, message: "browser worker reply timeout".into() }
            }
        }
    }
}

impl Drop for ProcessBrowserWorker {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        debug!("browser worker shut down");
    }
}

fn spawn_writer(mut stdin: ChildStdin, rx: Receiver<WorkerRequest>) -> JoinHandle<()> {
    thread::spawn(move || {
        for req in rx {
            let line = match serde_json::to_string(&req) {
                Ok(s) => s,
                Err(e) => { error!(error = %e, req_id = %req.req_id, "failed to encode WorkerRequest"); continue; }
            };
            if stdin.write_all(line.as_bytes()).is_err() || stdin.write_all(b"\n").is_err() {
                error!("failed to write to worker stdin");
                break;
            }
            if stdin.flush().is_err() {
                error!("failed to flush worker stdin");
                break;
            }
        }
    })
}

fn spawn_reader(stdout: ChildStdout, pending: Arc<Mutex<HashMap<String, SyncSender<WorkerResponse>>>>) -> JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => { error!(error = %e, "worker stdout read error"); break; }
            };
            if line.trim().is_empty() { continue; }
            let resp: WorkerResponse = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => { warn!(error = %e, raw = %line, "worker emitted non-conforming line"); continue; }
            };
            let req_id = match &resp {
                WorkerResponse::Ok { req_id, .. } => req_id.clone(),
                WorkerResponse::Error { req_id, .. } => req_id.clone(),
            };
            let waiter = pending.lock().expect("pending map poisoned").remove(&req_id);
            match waiter {
                Some(tx) => { let _ = tx.send(resp); }
                None => warn!(req_id = %req_id, "worker reply for unknown req_id"),
            }
        }
        debug!("worker stdout closed");
    })
}

fn spawn_stderr_reader(stderr: std::process::ChildStderr) -> JoinHandle<()> {
    thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            debug!(target: "browser_worker", "{line}");
        }
    })
}
