//! Optional Jev (TypeSafe AI System One) decision layer.
//!
//! Off unless `SAAN_JEV=on` *and* `JEV_API_KEY` is set. What leaves the machine is
//! bounded by [`Privacy`]:
//!
//! | level | routing request | candidate pick request |
//! |---|---|---|
//! | `a` (default) | query | never sent |
//! | `b` | query | query + candidate relative paths |
//! | `c` | query | query + candidate relative paths + short snippets |
//!
//! Absolute paths, file contents beyond the snippet, and index data are never sent.

use std::time::Duration;

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

use crate::router::Mode;

pub const DEFAULT_BASE_URL: &str = "https://api.typesafe.ai";
pub const DEFAULT_MODEL: &str = "jev-latest";
const TIMEOUT: Duration = Duration::from_millis(2500);
/// Max characters of a snippet sent at privacy level `c`.
pub const SNIPPET_CHARS: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Privacy {
    /// A: query text only.
    Query,
    /// B: query + candidate relative paths.
    Paths,
    /// C: query + candidate relative paths + snippets.
    Snippets,
}

impl Privacy {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "a" | "query" => Some(Privacy::Query),
            "b" | "paths" => Some(Privacy::Paths),
            "c" | "snippets" => Some(Privacy::Snippets),
            _ => None,
        }
    }
}

/// A search result offered to Jev for disambiguation.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub rel: String,
    pub snippet: String,
}

pub struct JevClient {
    key: String,
    pub privacy: Privacy,
    base_url: String,
    model: String,
    agent: ureq::Agent,
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

impl JevClient {
    /// `None` (and therefore zero network traffic) unless `SAAN_JEV` is
    /// `on`/`1`/`true` and `JEV_API_KEY` is set. The key is read from nowhere else.
    pub fn from_env() -> Option<Self> {
        let enabled = env_nonempty("SAAN_JEV")
            .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "on" | "1" | "true"));
        if !enabled {
            return None;
        }
        let key = env_nonempty("JEV_API_KEY")?;
        let privacy = env_nonempty("SAAN_JEV_PRIVACY").and_then(|v| Privacy::parse(&v)).unwrap_or(Privacy::Query);
        let base_url = env_nonempty("SAAN_JEV_URL").unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        let model = env_nonempty("SAAN_JEV_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let agent = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT)).build().into();
        Some(Self { key, privacy, base_url: base_url.trim_end_matches('/').to_string(), model, agent })
    }

    /// Body for choosing a search mode. Contains only the query, at every level.
    pub fn route_request(&self, query: &str) -> Value {
        json!({
            "model": self.model,
            "state": { "query": query },
            "questions": {
                "mode": {
                    "type": "choice",
                    "instructions": "Which search finds the file the user wants for this `query`?",
                    "criteria": {
                        "semantic": "The query describes a topic, purpose or meaning in natural language.",
                        "grep": "The query is exact text, an identifier, code or a regex that appears inside files.",
                        "glob": "The query is a file name, extension or path pattern.",
                    }
                }
            }
        })
    }

    /// Body for picking the best candidate, or `None` at level A (nothing about
    /// files may be sent).
    pub fn pick_request(&self, query: &str, candidates: &[Candidate]) -> Option<Value> {
        if self.privacy == Privacy::Query || candidates.len() < 2 {
            return None;
        }
        let mut criteria = Map::new();
        for (i, c) in candidates.iter().enumerate() {
            let value = match self.privacy {
                Privacy::Snippets => json!({
                    "path": c.rel,
                    "snippet": c.snippet.chars().take(SNIPPET_CHARS).collect::<String>(),
                }),
                _ => json!({ "path": c.rel }),
            };
            criteria.insert(format!("c{i}"), value);
        }
        Some(json!({
            "model": self.model,
            "state": { "query": query },
            "questions": {
                "best": {
                    "type": "choice",
                    "instructions": "Which candidate file is the one the user is looking for with this `query`?",
                    "criteria": criteria,
                }
            }
        }))
    }

    fn post(&self, body: &Value) -> Result<Value> {
        let url = format!("{}/v1/systemone", self.base_url);
        let value = self
            .agent
            .post(&url)
            .header("Authorization", &format!("Bearer {}", self.key))
            .send_json(body)?
            .body_mut()
            .read_json::<Value>()?;
        Ok(value)
    }

    fn choice(response: &Value, question: &str) -> Result<(String, f64)> {
        let answer = &response["answers"][question];
        let choice = answer["choice"].as_str().ok_or_else(|| anyhow!("Jev answer missing `{question}`"))?;
        Ok((choice.to_string(), answer["confidence"].as_f64().unwrap_or(0.0)))
    }

    pub fn route(&self, query: &str) -> Result<Mode> {
        let (choice, _) = Self::choice(&self.post(&self.route_request(query))?, "mode")?;
        Mode::parse(&choice).ok_or_else(|| anyhow!("Jev returned unknown mode `{choice}`"))
    }

    /// Index into `candidates` Jev prefers, or `None` when the level forbids asking.
    pub fn pick(&self, query: &str, candidates: &[Candidate]) -> Result<Option<usize>> {
        let Some(body) = self.pick_request(query, candidates) else { return Ok(None) };
        let (choice, _) = Self::choice(&self.post(&body)?, "best")?;
        let i = choice
            .strip_prefix('c')
            .and_then(|n| n.parse::<usize>().ok())
            .filter(|&i| i < candidates.len())
            .ok_or_else(|| anyhow!("Jev returned unknown candidate `{choice}`"))?;
        Ok(Some(i))
    }
}
