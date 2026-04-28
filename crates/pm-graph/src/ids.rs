//! Stable id derivation from natural keys.
//!
//! Every PM entity gets a deterministic [`polystore::EntityId`] of the
//! form `pm:<kind>:<owner>/<name>[#<natural-key>]`. The owner/name pair
//! lives in [`RepoCoord`]; the rest is composed from the entity itself.
//!
//! Encoding the natural key directly (rather than hashing) makes the IDs
//! human-readable, so you can grep the graph for `pm:issue:anatta-rs/anatta#…`
//! without an indirection table. Title characters that confuse the
//! id format (`#`, `:`, `\`, control bytes) are percent-escaped.

use polystore::EntityId;
use std::fmt::Write;

/// `(owner, repo)` coordinate — every PM node belongs to exactly one repo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoCoord {
    /// e.g. `"anatta-rs"`.
    pub owner: String,
    /// e.g. `"anatta"`.
    pub repo: String,
}

impl RepoCoord {
    /// Construct a coord from the two halves.
    #[must_use]
    pub fn new(owner: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            owner: owner.into(),
            repo: repo.into(),
        }
    }

    /// Render as `owner/name`.
    #[must_use]
    pub fn slash(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// Parse `owner/name`, returning `None` on malformed input.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.split_once('/') {
            Some((o, r)) if !o.is_empty() && !r.is_empty() && !r.contains('/') => {
                Some(Self::new(o, r))
            }
            _ => None,
        }
    }

    /// `pm:repo:<owner>/<name>` — the root node id.
    pub fn repo_id(&self) -> EntityId {
        EntityId::new(format!("pm:repo:{}", self.slash()))
    }

    /// `pm:milestone:<owner>/<name>#<title>`.
    pub fn milestone_id(&self, title: &str) -> EntityId {
        EntityId::new(format!(
            "pm:milestone:{}#{}",
            self.slash(),
            escape_natural_key(title)
        ))
    }

    /// `pm:issue:<owner>/<name>#<title>`.
    pub fn issue_id(&self, title: &str) -> EntityId {
        EntityId::new(format!(
            "pm:issue:{}#{}",
            self.slash(),
            escape_natural_key(title)
        ))
    }

    /// `pm:label:<owner>/<name>#<name>`.
    pub fn label_id(&self, name: &str) -> EntityId {
        EntityId::new(format!(
            "pm:label:{}#{}",
            self.slash(),
            escape_natural_key(name)
        ))
    }
}

/// Percent-escape characters that would confuse our id format.
/// We escape `#`, `:`, `%`, control bytes, and high bytes (non-ASCII) —
/// everything else passes through so titles stay legible.
fn escape_natural_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            // Safe ASCII: pass through. Excludes `#`, `:`, `%` (which would
            // confuse our id format) and the C0/DEL controls.
            0x20..=0x22 | 0x24 | 0x26..=0x39 | 0x3B..=0x7E => out.push(b as char),
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn coord_renders_slash_form() {
        let c = RepoCoord::new("anatta-rs", "anatta");
        assert_eq!(c.slash(), "anatta-rs/anatta");
    }

    #[test]
    fn coord_parse_round_trips() {
        let c = RepoCoord::parse("anatta-rs/anatta").expect("ok");
        assert_eq!(c.owner, "anatta-rs");
        assert_eq!(c.repo, "anatta");
    }

    #[test]
    fn coord_parse_rejects_malformed() {
        for bad in ["", "noslash", "/r", "o/", "a/b/c"] {
            assert!(RepoCoord::parse(bad).is_none(), "must reject {bad:?}");
        }
    }

    #[test]
    fn repo_id_is_deterministic() {
        let c = RepoCoord::new("o", "r");
        assert_eq!(c.repo_id().as_str(), "pm:repo:o/r");
        assert_eq!(c.repo_id(), c.repo_id());
    }

    #[test]
    fn milestone_id_carries_title() {
        let c = RepoCoord::new("o", "r");
        assert_eq!(c.milestone_id("v0.5").as_str(), "pm:milestone:o/r#v0.5");
    }

    #[test]
    fn issue_id_carries_title() {
        let c = RepoCoord::new("o", "r");
        assert_eq!(
            c.issue_id("Fix the auth").as_str(),
            "pm:issue:o/r#Fix the auth"
        );
    }

    #[test]
    fn ids_escape_dangerous_chars() {
        let c = RepoCoord::new("o", "r");
        // `#` would split the natural key, `:` breaks the prefix scheme.
        let id = c.issue_id("a#b:c%d");
        assert_eq!(id.as_str(), "pm:issue:o/r#a%23b%3Ac%25d");
    }

    #[test]
    fn ids_escape_unicode_per_byte() {
        let c = RepoCoord::new("o", "r");
        let id = c.milestone_id("v0.5 — multi-tenant");
        // The em-dash is 3 UTF-8 bytes (E2 80 94).
        assert!(
            id.as_str().contains("%E2%80%94"),
            "em-dash escaped per-byte: {id}"
        );
    }

    #[test]
    fn label_id_distinct_from_issue_with_same_text() {
        let c = RepoCoord::new("o", "r");
        let label = c.label_id("bug");
        let issue = c.issue_id("bug");
        assert_ne!(label, issue);
        assert!(label.as_str().starts_with("pm:label:"));
        assert!(issue.as_str().starts_with("pm:issue:"));
    }

    #[test]
    fn escape_passes_through_safe_chars() {
        assert_eq!(escape_natural_key("v0.5 — fix"), "v0.5 %E2%80%94 fix");
        assert_eq!(escape_natural_key("a-b_c.d"), "a-b_c.d");
    }
}
