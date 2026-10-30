//! Loads and validates `config.json`.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

pub const FIXED_SOURCE_TIERS: &[&str] = &[
    "rfc_standard",
    "official_docs",
    "paper",
    "reputable_technical",
    "other",
];

fn default_max_concurrency_per_provider() -> usize {
    3
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub providers: Vec<String>,
    pub roles: Roles,
    pub browser_profile_dir: String,
    pub timeouts_ms: TimeoutsMs,
    pub retries: i64,
    pub source_priority: Vec<String>,
    #[serde(default = "default_max_concurrency_per_provider")]
    pub max_concurrency_per_provider: usize,
}

impl Config {
    pub fn retries_u32(&self) -> u32 {
        self.retries.max(0).min(u32::MAX as i64) as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roles {
    pub research: String,
    pub challenge: String,
    pub recheck: String,
    #[serde(rename = "final")]
    pub final_pass: String,
}

impl Roles {
    pub fn as_pairs(&self) -> [(&'static str, &str); 4] {
        [
            ("research", self.research.as_str()),
            ("challenge", self.challenge.as_str()),
            ("recheck", self.recheck.as_str()),
            ("final", self.final_pass.as_str()),
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeoutsMs {
    pub page_load: u64,
    pub response_wait: u64,
}

#[derive(Debug)]
pub enum ConfigError {
    IoError(std::io::Error),
    ParseError(serde_json::Error),
    ValidationError(Vec<String>),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::IoError(e) => write!(f, "config I/O error: {e}"),
            ConfigError::ParseError(e) => write!(f, "config parse error: {e}"),
            ConfigError::ValidationError(problems) => {
                write!(f, "config validation failed ({} issue(s)):", problems.len())?;
                for p in problems {
                    write!(f, "\n  - {p}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ConfigError::IoError(e) => Some(e),
            ConfigError::ParseError(e) => Some(e),
            ConfigError::ValidationError(_) => None,
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(e: std::io::Error) -> Self {
        ConfigError::IoError(e)
    }
}

impl From<serde_json::Error> for ConfigError {
    fn from(e: serde_json::Error) -> Self {
        ConfigError::ParseError(e)
    }
}

pub fn default_config() -> Config {
    Config {
        providers: vec!["gemini".into(), "claude".into(), "deepseek".into()],
        roles: Roles {
            research: "gemini".into(),
            challenge: "claude".into(),
            recheck: "gemini".into(),
            final_pass: "claude".into(),
        },
        browser_profile_dir: "./profile".into(),
        timeouts_ms: TimeoutsMs {
            page_load: 30_000,
            response_wait: 60_000,
        },
        retries: 3,
        source_priority: FIXED_SOURCE_TIERS.iter().map(|s| (*s).to_string()).collect(),
        max_concurrency_per_provider: 3,
    }
}

pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    let p = Path::new(path);

    let raw = match fs::read_to_string(p) {
        Ok(s) => s,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let cfg = default_config();
            if let Err(w) = write_config(p, &cfg) {
                warn!(path = %path, error = %w, "no config found and could not write defaults; running with in-memory defaults");
            } else {
                info!(path = %path, "no config found; wrote defaults");
            }
            return Ok(cfg);
        }
        Err(e) => return Err(ConfigError::IoError(e)),
    };

    let cfg: Config = serde_json::from_str(&raw).map_err(ConfigError::ParseError)?;
    validate(&cfg)?;
    Ok(cfg)
}

pub fn load_from_repo_root() -> Result<Config, ConfigError> {
    for c in ["config.json", "../config.json", "../../config.json"] {
        if Path::new(c).exists() {
            return load_config(c);
        }
    }
    load_config("config.json")
}

fn write_config(path: &Path, cfg: &Config) -> Result<(), ConfigError> {
    let json = serde_json::to_string_pretty(cfg).map_err(ConfigError::ParseError)?;
    fs::write(path, json).map_err(ConfigError::IoError)?;
    Ok(())
}

pub fn validate(cfg: &Config) -> Result<(), ConfigError> {
    let mut problems: Vec<String> = Vec::new();

    let providers: HashSet<&str> = cfg.providers.iter().map(String::as_str).collect();
    for (role, provider) in cfg.roles.as_pairs() {
        if !providers.contains(provider) {
            problems.push(format!(
                "roles.{role} = {provider:?} is not listed in providers {:?}",
                cfg.providers,
            ));
        }
    }

    if cfg.retries < 0 {
        problems.push(format!("retries must be >= 0, got {}", cfg.retries));
    }
    if cfg.retries > u32::MAX as i64 {
        problems.push(format!("retries must be <= {}, got {}", u32::MAX, cfg.retries));
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for tier in &cfg.source_priority {
        if !FIXED_SOURCE_TIERS.contains(&tier.as_str()) {
            problems.push(format!(
                "source_priority contains unknown tier {tier:?}; allowed: {FIXED_SOURCE_TIERS:?}",
            ));
        }
        if !seen.insert(tier.as_str()) {
            problems.push(format!("source_priority contains duplicate {tier:?}"));
        }
    }

    if cfg.providers.is_empty() {
        problems.push("providers must contain at least one entry".into());
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(ConfigError::ValidationError(problems))
    }
}
