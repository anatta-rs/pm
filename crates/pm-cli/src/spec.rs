//! YAML/JSON spec format for `pm apply`.
//!
//! ```yaml
//! repo: anatta-rs/anatta
//! labels:
//!   - { name: "type:bug",     color: "d73a4a", description: "Something is broken" }
//!   - { name: "area:graph",   color: "0075ca" }
//! milestones:
//!   - title: "v0.5 — Multi-tenant"
//!     description: "GitHub-orgs style namespace model"
//!     due_on: "2026-06-01"
//! issues:
//!   - title: "I7: fix /api/v1/health 401"
//!     body: |
//!       Hook blocks before handler.
//!     milestone: "v0.5 — Multi-tenant"
//!     labels: ["type:bug"]
//!     assignees: ["Lsh0x"]
//! ```
//!
//! All fields are optional except `repo` (any of the three lists may be
//! empty). Re-applying a spec is a no-op.

use pm_core::{Issue, IssueState, Label, Milestone, MilestoneState};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level spec document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    /// `owner/name` — required.
    pub repo: String,
    /// Labels to upsert.
    #[serde(default)]
    pub labels: Vec<SpecLabel>,
    /// Milestones to upsert (created before issues so the latter can reference them).
    #[serde(default)]
    pub milestones: Vec<SpecMilestone>,
    /// Issues to upsert.
    #[serde(default)]
    pub issues: Vec<SpecIssue>,
}

/// Spec entry for a label (mirrors [`Label`] but lives in a separate type
/// to keep the on-disk format independent from the runtime types).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecLabel {
    /// Label name.
    pub name: String,
    /// Hex colour (no `#`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// One-line description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl From<SpecLabel> for Label {
    fn from(s: SpecLabel) -> Self {
        Self {
            name: s.name,
            color: s.color,
            description: s.description,
        }
    }
}

/// Spec entry for a milestone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecMilestone {
    /// Milestone title.
    pub title: String,
    /// Optional description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
    /// Defaults to open.
    #[serde(default)]
    pub state: MilestoneState,
}

impl From<SpecMilestone> for Milestone {
    fn from(s: SpecMilestone) -> Self {
        let mut m = Milestone::new(s.title).with_state(s.state);
        if let Some(d) = s.description {
            m = m.with_description(d);
        }
        if let Some(d) = s.due_on {
            m = m.with_due_on(d);
        }
        m
    }
}

/// Spec entry for an issue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecIssue {
    /// Issue title.
    pub title: String,
    /// Markdown body.
    #[serde(default)]
    pub body: String,
    /// Label names (must exist or be in this same spec).
    #[serde(default)]
    pub labels: Vec<String>,
    /// Assignees by login.
    #[serde(default)]
    pub assignees: Vec<String>,
    /// Optional milestone title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    /// Defaults to open.
    #[serde(default)]
    pub state: IssueState,
}

impl From<SpecIssue> for Issue {
    fn from(s: SpecIssue) -> Self {
        let mut i = Issue::new(s.title)
            .with_body(s.body)
            .with_labels(s.labels)
            .with_assignees(s.assignees)
            .with_state(s.state);
        if let Some(m) = s.milestone {
            i = i.with_milestone(m);
        }
        i
    }
}

/// Errors a spec file can produce.
#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    /// I/O failure.
    #[error("read {path}: {source}")]
    Io {
        /// Path the loader was trying to read.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// YAML/JSON deserialisation failed.
    #[error("parse {path}: {message}")]
    Parse {
        /// Path the loader was reading.
        path: String,
        /// Parser error message.
        message: String,
    },
    /// `repo:` is missing or malformed.
    #[error("repo {value:?} must be of the form owner/name")]
    BadRepo {
        /// What the user wrote.
        value: String,
    },
}

impl Spec {
    /// Load a spec from a file. Detects YAML vs JSON by extension; falls
    /// back to YAML for unknown extensions (YAML is a superset of JSON).
    pub fn from_path(path: impl AsRef<Path>) -> std::result::Result<Self, SpecError> {
        let p = path.as_ref();
        let raw = std::fs::read_to_string(p).map_err(|source| SpecError::Io {
            path: p.display().to_string(),
            source,
        })?;
        let path_str = p.display().to_string();
        if matches!(p.extension().and_then(|e| e.to_str()), Some("json")) {
            serde_json::from_str(&raw).map_err(|e| SpecError::Parse {
                path: path_str,
                message: e.to_string(),
            })
        } else {
            serde_yaml::from_str(&raw).map_err(|e| SpecError::Parse {
                path: path_str,
                message: e.to_string(),
            })
        }
    }

    /// Split `repo` into `(owner, name)`. Errors if it doesn't have exactly
    /// one slash.
    pub fn split_repo(&self) -> std::result::Result<(&str, &str), SpecError> {
        match self.repo.split_once('/') {
            Some((o, r)) if !o.is_empty() && !r.is_empty() && !r.contains('/') => Ok((o, r)),
            _ => Err(SpecError::BadRepo {
                value: self.repo.clone(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn parses_minimal_yaml() {
        let yaml = "repo: o/r\n";
        let spec: Spec = serde_yaml::from_str(yaml).expect("ok");
        assert_eq!(spec.repo, "o/r");
        assert!(spec.labels.is_empty());
        assert!(spec.milestones.is_empty());
        assert!(spec.issues.is_empty());
    }

    #[test]
    fn parses_full_yaml() {
        let yaml = r"
repo: anatta-rs/anatta
labels:
  - name: type:bug
    color: d73a4a
milestones:
  - title: v0.5
    due_on: 2026-06-01
issues:
  - title: Fix it
    body: |
      multi
      line
    labels: [type:bug]
    milestone: v0.5
";
        let spec: Spec = serde_yaml::from_str(yaml).expect("ok");
        assert_eq!(spec.labels.len(), 1);
        assert_eq!(spec.milestones.len(), 1);
        assert_eq!(spec.issues.len(), 1);
        assert_eq!(spec.issues[0].body, "multi\nline\n");
        assert_eq!(spec.issues[0].labels, vec!["type:bug"]);
        assert_eq!(spec.issues[0].milestone.as_deref(), Some("v0.5"));
    }

    #[test]
    fn split_repo_handles_owner_slash_name() {
        let s = Spec {
            repo: "anatta-rs/anatta".into(),
            labels: vec![],
            milestones: vec![],
            issues: vec![],
        };
        assert_eq!(s.split_repo().expect("ok"), ("anatta-rs", "anatta"));
    }

    #[test]
    fn split_repo_rejects_malformed() {
        for bad in ["", "noslash", "/r", "o/", "a/b/c"] {
            let s = Spec {
                repo: bad.into(),
                labels: vec![],
                milestones: vec![],
                issues: vec![],
            };
            assert!(s.split_repo().is_err(), "must reject {bad:?}");
        }
    }

    #[test]
    fn from_path_reads_yaml() {
        let dir = std::env::temp_dir().join(format!("pm-spec-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("ok");
        let p = dir.join("plan.yaml");
        std::fs::write(&p, "repo: o/r\n").expect("ok");
        let s = Spec::from_path(&p).expect("ok");
        assert_eq!(s.repo, "o/r");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn from_path_reads_json() {
        let dir = std::env::temp_dir().join(format!("pm-spec-test-json-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("ok");
        let p = dir.join("plan.json");
        std::fs::write(&p, r#"{"repo":"o/r"}"#).expect("ok");
        let s = Spec::from_path(&p).expect("ok");
        assert_eq!(s.repo, "o/r");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn from_path_reports_missing_file() {
        let err = Spec::from_path("/no/such/file.yaml").expect_err("must fail");
        assert!(matches!(err, SpecError::Io { .. }));
    }

    #[test]
    fn from_path_reports_parse_error() {
        let dir = std::env::temp_dir().join(format!("pm-spec-test-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("ok");
        let p = dir.join("bad.yaml");
        std::fs::write(&p, "repo: [unclosed").expect("ok");
        let err = Spec::from_path(&p).expect_err("must fail");
        assert!(matches!(err, SpecError::Parse { .. }));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn label_conversion_round_trips() {
        let s = SpecLabel {
            name: "x".into(),
            color: Some("ff00ff".into()),
            description: Some("y".into()),
        };
        let l: Label = s.into();
        assert_eq!(l.name, "x");
        assert_eq!(l.color.as_deref(), Some("ff00ff"));
    }

    #[test]
    fn milestone_conversion_round_trips() {
        let s = SpecMilestone {
            title: "v0.5".into(),
            description: Some("d".into()),
            due_on: Some("2026-06-01".into()),
            state: MilestoneState::Open,
        };
        let m: Milestone = s.into();
        assert_eq!(m.title, "v0.5");
        assert_eq!(m.description.as_deref(), Some("d"));
        assert_eq!(m.due_on.as_deref(), Some("2026-06-01"));
    }

    #[test]
    fn issue_conversion_round_trips() {
        let s = SpecIssue {
            title: "X".into(),
            body: "b".into(),
            labels: vec!["bug".into()],
            assignees: vec!["me".into()],
            milestone: Some("v0.5".into()),
            state: IssueState::Open,
        };
        let i: Issue = s.into();
        assert_eq!(i.title, "X");
        assert_eq!(i.body, "b");
        assert_eq!(i.labels, vec!["bug"]);
        assert_eq!(i.assignees, vec!["me"]);
        assert_eq!(i.milestone.as_deref(), Some("v0.5"));
    }
}
