//! Integration tests for the optional Jev decision layer.
//!
//! The network is faked with a tiny in-process HTTP/1.1 server bound to
//! `127.0.0.1:0`, so these tests never reach `api.typesafe.ai` (or any other
//! non-loopback address) and never depend on the real service.
//!
//! Everything that reads or writes environment variables lives in the single
//! `env_gating_and_privacy_levels` test: `#[test]` functions in one binary share
//! one process environment and run on parallel threads, so splitting the cases
//! up would race on `SAAN_JEV`/`JEV_API_KEY`/`SAAN_JEV_URL`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use saan_core::jev::{self, Candidate, JevClient, Privacy};
use saan_core::router::Mode;
use serde_json::{json, Value};

/// The env vars the Jev layer reads. Cleared between cases.
const JEV_ENV: [&str; 5] = ["SAAN_JEV", "JEV_API_KEY", "SAAN_JEV_PRIVACY", "SAAN_JEV_URL", "SAAN_JEV_MODEL"];

fn clear_jev_env() {
    for key in JEV_ENV {
        std::env::remove_var(key);
    }
}

/// One request as observed by [`MockServer`].
#[derive(Debug, Clone)]
struct Recorded {
    path: String,
    authorization: Option<String>,
    body: String,
}

impl Recorded {
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).expect("recorded body is JSON")
    }
}

/// A loopback HTTP/1.1 server that answers one Jev question per request.
struct MockServer {
    url: String,
    seen: Arc<Mutex<Vec<Recorded>>>,
}

impl MockServer {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback port");
        let url = format!("http://{}", listener.local_addr().expect("bound address"));
        let seen: Arc<Mutex<Vec<Recorded>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { break };
                serve(stream, &sink);
            }
        });
        Self { url, seen }
    }

    fn count(&self) -> usize {
        self.seen.lock().expect("mock server mutex").len()
    }

    fn request(&self, index: usize) -> Recorded {
        let seen = self.seen.lock().expect("mock server mutex");
        assert!(index < seen.len(), "expected a request at index {index}, but only {} were recorded", seen.len());
        seen[index].clone()
    }
}

/// Read one request, record it, and answer it. Recording happens before the
/// response is written, so a completed client call implies a recorded request.
fn serve(mut stream: TcpStream, sink: &Mutex<Vec<Recorded>>) {
    let Ok(peer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(peer);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let path = request_line.split_whitespace().nth(1).unwrap_or_default().to_string();

    let mut authorization = None;
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else { continue };
        match name.to_ascii_lowercase().as_str() {
            "content-length" => content_length = value.trim().parse().unwrap_or(0),
            "authorization" => authorization = Some(value.trim().to_string()),
            _ => {}
        }
    }

    let mut raw = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut raw).is_err() {
        return;
    }
    let body = String::from_utf8_lossy(&raw).into_owned();

    sink.lock().expect("mock server mutex").push(Recorded { path, authorization, body: body.clone() });

    let reply = reply_for(&body);
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        reply.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(reply.as_bytes());
    let _ = stream.flush();
}

/// Canned answer for the question in `body`: `mode` -> `grep`, `best` -> `c1`.
fn reply_for(body: &str) -> String {
    let request: Value = serde_json::from_str(body).expect("request body is JSON");
    let asks_best = request["questions"].get("best").is_some();
    let (question, choice) = if asks_best { ("best", "c1") } else { ("mode", "grep") };
    json!({
        "model": "jev",
        "answers": {
            (question): {
                "type": "choice",
                "choice": choice,
                "confidence": 0.9,
                "probabilities": {},
            }
        },
        "usage": {},
    })
    .to_string()
}

fn candidates(rels: &[&str]) -> Vec<Candidate> {
    rels.iter()
        .map(|rel| Candidate { rel: (*rel).to_string(), snippet: format!("SECRET-SNIPPET from {rel}") })
        .collect()
}

#[test]
fn env_gating_and_privacy_levels() {
    let server = MockServer::start();

    // 1. Gating: Jev stays off (and silent) unless SAAN_JEV=on *and* the key is present.
    clear_jev_env();
    std::env::set_var("JEV_API_KEY", "test-key");
    assert!(JevClient::from_env().is_none(), "a key alone must not enable Jev");

    clear_jev_env();
    std::env::set_var("SAAN_JEV", "on");
    assert!(JevClient::from_env().is_none(), "SAAN_JEV=on without JEV_API_KEY must not enable Jev");

    clear_jev_env();
    std::env::set_var("SAAN_JEV", "off");
    std::env::set_var("JEV_API_KEY", "test-key");
    assert!(JevClient::from_env().is_none(), "SAAN_JEV=off must not enable Jev");

    assert_eq!(server.count(), 0, "a disabled client must not send anything");

    // 2. The key comes from JEV_API_KEY, and requests go to /v1/systemone.
    clear_jev_env();
    std::env::set_var("SAAN_JEV", "on");
    std::env::set_var("JEV_API_KEY", "test-key");
    std::env::set_var("SAAN_JEV_URL", &server.url);

    let client = JevClient::from_env().expect("SAAN_JEV=on + JEV_API_KEY must enable Jev");
    assert_eq!(client.privacy, Privacy::Query, "privacy defaults to level a");

    assert_eq!(client.route("dijkstra").expect("route"), Mode::Grep, "mock answers grep");
    assert_eq!(server.count(), 1, "exactly one routing request");
    let routing = server.request(0);
    assert_eq!(routing.path, "/v1/systemone");
    assert_eq!(routing.authorization.as_deref(), Some("Bearer test-key"));

    // 3. Level A (default): the query goes out, candidates never do.
    let routing_body = routing.json();
    assert_eq!(routing_body["state"], json!({ "query": "dijkstra" }), "level a sends the query and nothing else");
    assert_eq!(routing_body["state"], client.route_request("dijkstra")["state"]);
    assert_eq!(routing_body["model"], json!(jev::DEFAULT_MODEL));

    let cands = candidates(&["work/a/README.md", "work/b/notes.md", "work/c/main.rs"]);
    assert!(client.pick_request("dijkstra", &cands).is_none(), "level a must not build a pick request");
    assert_eq!(client.pick("dijkstra", &cands).expect("pick"), None);
    assert_eq!(server.count(), 1, "level a must not send a pick request");

    // 4. Level B: relative paths only, snippets stay local.
    clear_jev_env();
    std::env::set_var("SAAN_JEV", "on");
    std::env::set_var("JEV_API_KEY", "test-key");
    std::env::set_var("SAAN_JEV_URL", &server.url);
    std::env::set_var("SAAN_JEV_PRIVACY", "b");

    let client = JevClient::from_env().expect("enabled");
    assert_eq!(client.privacy, Privacy::Paths);

    let before = server.count();
    assert_eq!(client.pick("which readme", &cands).expect("pick"), Some(1), "mock answers c1");
    assert_eq!(server.count(), before + 1, "exactly one pick request");
    let picking = server.request(before);
    assert!(!picking.body.contains("SECRET-SNIPPET"), "level b must not send snippets: {}", picking.body);

    let picking_body = picking.json();
    assert_eq!(picking_body["state"], json!({ "query": "which readme" }));
    let criteria = picking_body["questions"]["best"]["criteria"].as_object().expect("criteria object");
    assert_eq!(criteria.len(), 3, "one criterion per candidate");
    for (key, value) in criteria {
        let fields = value.as_object().expect("criteria value is an object");
        assert_eq!(
            fields.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["path"],
            "{key} at level b must carry only `path`"
        );
    }
    let rels = ["work/a/README.md", "work/b/notes.md", "work/c/main.rs"];
    for (i, rel) in rels.into_iter().enumerate() {
        let entry = criteria.get(&format!("c{i}")).expect("criteria entry for candidate");
        assert_eq!(entry["path"], json!(rel), "c{i} must carry the candidate relative path");
    }

    // 5. Level C: paths plus snippets, truncated to SNIPPET_CHARS.
    clear_jev_env();
    std::env::set_var("SAAN_JEV", "on");
    std::env::set_var("JEV_API_KEY", "test-key");
    std::env::set_var("SAAN_JEV_URL", &server.url);
    std::env::set_var("SAAN_JEV_PRIVACY", "c");

    let client = JevClient::from_env().expect("enabled");
    assert_eq!(client.privacy, Privacy::Snippets);

    let long = "s".repeat(500);
    let long_cands: Vec<Candidate> = candidates(&["work/a/README.md", "work/b/notes.md", "work/c/main.rs"])
        .into_iter()
        .map(|cand| Candidate { snippet: long.clone(), ..cand })
        .collect();

    let before = server.count();
    assert_eq!(client.pick("which readme", &long_cands).expect("pick"), Some(1));
    assert_eq!(server.count(), before + 1, "exactly one pick request");
    let picking_body = server.request(before).json();
    let criteria = picking_body["questions"]["best"]["criteria"].as_object().expect("criteria object");
    assert_eq!(criteria.len(), 3);
    for (key, value) in criteria {
        let fields = value.as_object().expect("criteria value is an object");
        assert!(fields.contains_key("path"), "{key} at level c keeps `path`");
        let snippet = fields.get("snippet").and_then(Value::as_str).expect("level c sends a snippet");
        assert_eq!(snippet.chars().count(), jev::SNIPPET_CHARS, "{key} snippet must be truncated");
        assert_eq!(snippet, &long[..jev::SNIPPET_CHARS], "{key} snippet must be the head of the candidate snippet");
    }
    // Level C is the only level whose recorded request carries snippet text.
    let sent = server.request(before).body;
    assert!(sent.contains(&long[..jev::SNIPPET_CHARS]), "level c must send the snippet head");
    assert!(!sent.contains(&long), "level c must not send a snippet longer than SNIPPET_CHARS");

    // 6. A dead endpoint is an Err, not a panic or a hang.
    let dead_port = {
        let probe = TcpListener::bind("127.0.0.1:0").expect("bind probe port");
        let port = probe.local_addr().expect("probe address").port();
        drop(probe);
        port
    };

    clear_jev_env();
    std::env::set_var("SAAN_JEV", "on");
    std::env::set_var("JEV_API_KEY", "test-key");
    std::env::set_var("SAAN_JEV_URL", format!("http://127.0.0.1:{dead_port}"));

    let client = JevClient::from_env().expect("enabled");
    assert!(client.route("dijkstra").is_err(), "an unreachable Jev must surface as Err");

    clear_jev_env();
}
