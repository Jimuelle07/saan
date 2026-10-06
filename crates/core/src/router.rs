//! Local, deterministic query router: decides semantic vs grep vs glob.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Semantic,
    Grep,
    Glob,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Semantic => "semantic",
            Mode::Grep => "grep",
            Mode::Glob => "glob",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "semantic" => Some(Mode::Semantic),
            "grep" => Some(Mode::Grep),
            "glob" => Some(Mode::Glob),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteSource {
    /// The user typed a prefix (`grep:`, `glob:`, `/regex/`).
    Explicit,
    /// Local heuristics.
    Rules,
    /// Jev chose the mode.
    Jev,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Route {
    pub mode: Mode,
    /// Query with any routing prefix removed (a regex for grep, a pattern for glob).
    pub query: String,
    /// 0..1, how sure the local rules are. Below [`CONFIDENT`] the router may ask Jev.
    pub confidence: f32,
    pub source: RouteSource,
}

/// Local routes at or above this confidence never consult Jev.
pub const CONFIDENT: f32 = 0.75;

fn route(mode: Mode, query: &str, confidence: f32, source: RouteSource) -> Route {
    Route { mode, query: query.trim().to_string(), confidence, source }
}

fn looks_like_filename(q: &str) -> bool {
    let Some((stem, ext)) = q.rsplit_once('.') else { return false };
    !stem.is_empty()
        && (1..=5).contains(&ext.len())
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && ext.chars().any(|c| c.is_ascii_alphabetic())
        && stem.chars().all(|c| c.is_alphanumeric() || "-_.".contains(c))
}

pub fn local_route(input: &str) -> Route {
    let q = input.trim();
    for (prefix, mode) in [("grep:", Mode::Grep), ("glob:", Mode::Glob), ("find:", Mode::Semantic)] {
        if let Some(rest) = q.strip_prefix(prefix) {
            return route(mode, rest, 1.0, RouteSource::Explicit);
        }
    }
    if q.len() > 2 && q.starts_with('/') && q.ends_with('/') {
        return route(Mode::Grep, &q[1..q.len() - 1], 1.0, RouteSource::Explicit);
    }
    let one_token = !q.contains(char::is_whitespace);
    if one_token && q.contains(['*', '?', '[']) {
        return route(Mode::Glob, q, 0.95, RouteSource::Rules);
    }
    if one_token && (looks_like_filename(q) || q.contains('/') && !q.starts_with('/')) {
        return route(Mode::Glob, q, 0.85, RouteSource::Rules);
    }
    if q.contains(['(', ')', '{', '}', ';', '=', '<', '>']) || q.contains("::") {
        // Code-looking text: most likely an exact search, but could be a description.
        return route(Mode::Grep, &regex::escape(q), 0.6, RouteSource::Rules);
    }
    if one_token {
        // A lone word ("dijkstra", "HashMap") is ambiguous between topic and identifier.
        return route(Mode::Semantic, q, 0.5, RouteSource::Rules);
    }
    route(Mode::Semantic, q, 0.9, RouteSource::Rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[track_caller]
    fn check(input: &str, mode: Mode, query: &str, explicit: bool) {
        let r = local_route(input);
        assert_eq!((r.mode, r.query.as_str()), (mode, query), "{input}");
        assert_eq!(r.source == RouteSource::Explicit, explicit, "{input}");
    }

    #[test]
    fn prefixes_win_over_heuristics() {
        check("grep: *.md", Mode::Grep, "*.md", true);
        check("glob:notes about rust", Mode::Glob, "notes about rust", true);
        check("find: todo.txt", Mode::Semantic, "todo.txt", true);
        check("/fn \\w+\\(/", Mode::Grep, "fn \\w+\\(", true);
    }

    #[test]
    fn heuristics() {
        check("*.pdf", Mode::Glob, "*.pdf", false);
        check("todo.txt", Mode::Glob, "todo.txt", false);
        check("work/projectA", Mode::Glob, "work/projectA", false);
        check("fn main()", Mode::Grep, "fn main\\(\\)", false);
        check("how to proof bread overnight", Mode::Semantic, "how to proof bread overnight", false);
        // Versions and decimals are not file names.
        check("v1.2", Mode::Semantic, "v1.2", false);
    }

    #[test]
    fn only_ambiguous_routes_are_below_confidence_threshold() {
        assert!(local_route("dijkstra").confidence < CONFIDENT);
        assert!(local_route("x = 1;").confidence < CONFIDENT);
        assert!(local_route("trip itinerary for japan").confidence >= CONFIDENT);
        assert!(local_route("todo.txt").confidence >= CONFIDENT);
    }
}
