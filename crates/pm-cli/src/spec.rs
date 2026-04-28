//! YAML/JSON spec formats for `pm apply`.
//!
//! ## Single-repo Spec
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
//!
//! ## Multi-repo Spec
//!
//! ```yaml
//! repos:
//!   - anatta-rs/Anatta
//!   - anatta-rs/pm
//! shared_labels:
//!   - { name: "type:bug",     color: "d73a4a", description: "Something is broken" }
//! shared_milestones:
//!   - title: "v0.5 — Multi-tenant"
//!     due_on: "2026-06-01"
//! issues:
//!   - repo: anatta-rs/Anatta
//!     title: "I7: fix /api/v1/health 401"
//!     labels: ["type:bug"]
//!     milestone: "v0.5 — Multi-tenant"
//!   - repo: anatta-rs/pm
//!     title: "Multi-repo apply support"
//!     labels: ["type:bug"]
//! ```
//!
//! All repos in `repos` receive the full set of `shared_labels` and
//! `shared_milestones`. Each issue's `repo` field must be in `repos`.

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

/// Multi-repo spec for applying shared labels/milestones + per-repo issues
/// across multiple repos at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiSpec {
    /// List of target repos in `owner/name` format.
    pub repos: Vec<String>,
    /// Labels replicated to all `repos` as `shared_labels`.
    #[serde(default)]
    pub shared_labels: Vec<SpecLabel>,
    /// Milestones replicated to all `repos` as `shared_milestones`.
    #[serde(default)]
    pub shared_milestones: Vec<SpecMilestone>,
    /// Issues, each tagged with its target `repo` field.
    #[serde(default)]
    pub issues: Vec<MultiSpecIssue>,
}

/// Issue in a [`MultiSpec`] — has an explicit `repo` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MultiSpecIssue {
    /// Target repo in `owner/name` format (must be in `MultiSpec::repos`).
    pub repo: String,
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

impl MultiSpec {
    /// Validate that all issues reference repos in `self.repos`.
    /// Returns `SpecError::BadRepo` on mismatch.
    pub fn validate(&self) -> std::result::Result<(), SpecError> {
        for issue in &self.issues {
            if !self.repos.contains(&issue.repo) {
                return Err(SpecError::BadRepo {
                    value: format!(
                        "issue {title:?} references unknown repo {repo:?}; not in repos list",
                        title = issue.title,
                        repo = issue.repo
                    ),
                });
            }
        }
        Ok(())
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

/// Union of single-repo and multi-repo spec formats. Auto-detected from YAML.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnySpec {
    /// Single repo: `repo: owner/name`, `labels`, `milestones`, `issues`.
    Single(Spec),
    /// Multiple repos: `repos: [...]`, `shared_labels`, `shared_milestones`, `issues` with `repo` field.
    Multi(MultiSpec),
}

impl AnySpec {
    /// Load a spec from a file, auto-detecting format.
    /// Tries `MultiSpec` first (presence of `repos:` array), then `Spec` (presence of `repo:` field).
    pub fn from_path(path: impl AsRef<Path>) -> std::result::Result<Self, SpecError> {
        let p = path.as_ref();
        let raw = std::fs::read_to_string(p).map_err(|source| SpecError::Io {
            path: p.display().to_string(),
            source,
        })?;
        let path_str = p.display().to_string();

        let is_json = matches!(p.extension().and_then(|e| e.to_str()), Some("json"));

        // Try MultiSpec first
        if is_json {
            if let Ok(multi) = serde_json::from_str::<MultiSpec>(&raw) {
                multi.validate()?;
                return Ok(AnySpec::Multi(multi));
            }
        } else if let Ok(multi) = serde_yaml::from_str::<MultiSpec>(&raw) {
            multi.validate()?;
            return Ok(AnySpec::Multi(multi));
        }

        // Fall back to Spec
        let spec = if is_json {
            serde_json::from_str::<Spec>(&raw).map_err(|e| SpecError::Parse {
                path: path_str,
                message: e.to_string(),
            })?
        } else {
            serde_yaml::from_str::<Spec>(&raw).map_err(|e| SpecError::Parse {
                path: path_str,
                message: e.to_string(),
            })?
        };
        Ok(AnySpec::Single(spec))
    }
}

impl Spec {
    /// Load a spec from a file. Detects YAML vs JSON by extension; falls
    /// back to YAML for unknown extensions (YAML is a superset of JSON).
    ///
    /// Prefer [`AnySpec::from_path`] which auto-detects both single and multi-repo formats.
    #[allow(dead_code)]
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

    #[test]
    fn multi_spec_parses_minimal_yaml() {
        let yaml = r"
repos:
  - anatta-rs/Anatta
  - anatta-rs/pm
";
        let multi: MultiSpec = serde_yaml::from_str(yaml).expect("ok");
        assert_eq!(multi.repos.len(), 2);
        assert_eq!(multi.repos[0], "anatta-rs/Anatta");
        assert!(multi.shared_labels.is_empty());
        assert!(multi.shared_milestones.is_empty());
        assert!(multi.issues.is_empty());
    }

    #[test]
    fn multi_spec_parses_full_yaml() {
        let yaml = r"
repos:
  - anatta-rs/Anatta
  - anatta-rs/pm
shared_labels:
  - name: type:bug
    color: d73a4a
shared_milestones:
  - title: v0.5
    due_on: 2026-06-01
issues:
  - repo: anatta-rs/Anatta
    title: I7 fix
    labels: [type:bug]
    milestone: v0.5
  - repo: anatta-rs/pm
    title: Multi-repo apply support
    labels: [type:bug]
";
        let multi: MultiSpec = serde_yaml::from_str(yaml).expect("ok");
        assert_eq!(multi.repos.len(), 2);
        assert_eq!(multi.shared_labels.len(), 1);
        assert_eq!(multi.shared_milestones.len(), 1);
        assert_eq!(multi.issues.len(), 2);
        assert_eq!(multi.issues[0].repo, "anatta-rs/Anatta");
        assert_eq!(multi.issues[0].title, "I7 fix");
        assert_eq!(multi.issues[1].repo, "anatta-rs/pm");
    }

    #[test]
    fn multi_spec_validate_accepts_valid() {
        let multi = MultiSpec {
            repos: vec!["o1/r1".into(), "o2/r2".into()],
            shared_labels: vec![],
            shared_milestones: vec![],
            issues: vec![
                MultiSpecIssue {
                    repo: "o1/r1".into(),
                    title: "Issue 1".into(),
                    body: String::new(),
                    labels: vec![],
                    assignees: vec![],
                    milestone: None,
                    state: IssueState::Open,
                },
                MultiSpecIssue {
                    repo: "o2/r2".into(),
                    title: "Issue 2".into(),
                    body: String::new(),
                    labels: vec![],
                    assignees: vec![],
                    milestone: None,
                    state: IssueState::Open,
                },
            ],
        };
        assert!(multi.validate().is_ok());
    }

    #[test]
    fn multi_spec_validate_rejects_unknown_repo() {
        let multi = MultiSpec {
            repos: vec!["o1/r1".into()],
            shared_labels: vec![],
            shared_milestones: vec![],
            issues: vec![MultiSpecIssue {
                repo: "o2/unknown".into(),
                title: "Bad issue".into(),
                body: String::new(),
                labels: vec![],
                assignees: vec![],
                milestone: None,
                state: IssueState::Open,
            }],
        };
        assert!(multi.validate().is_err());
    }

    #[test]
    fn any_spec_detects_single() {
        let yaml = "repo: o/r\n";
        let _any = AnySpec::from_path("/dev/null").expect_err("will fail on read");
        // Deserialize directly to verify detection logic
        let spec: Spec = serde_yaml::from_str(yaml).expect("ok");
        assert_eq!(spec.repo, "o/r");
    }

    #[test]
    fn any_spec_detects_multi() {
        let yaml = r"
repos:
  - o/r1
  - o/r2
";
        let multi: MultiSpec = serde_yaml::from_str(yaml).expect("ok");
        assert_eq!(multi.repos.len(), 2);
    }

    #[test]
    fn any_spec_from_path_single() {
        let dir = std::env::temp_dir().join(format!("pm-anyspec-single-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("ok");
        let p = dir.join("plan.yaml");
        std::fs::write(&p, "repo: o/r\n").expect("ok");
        let any = AnySpec::from_path(&p).expect("ok");
        assert!(matches!(any, AnySpec::Single(_)));
        if let AnySpec::Single(spec) = any {
            assert_eq!(spec.repo, "o/r");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn any_spec_from_path_multi() {
        let dir = std::env::temp_dir().join(format!("pm-anyspec-multi-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("ok");
        let p = dir.join("plan.yaml");
        std::fs::write(&p, "repos:\n  - o/r1\n  - o/r2\n").expect("ok");
        let any = AnySpec::from_path(&p).expect("ok");
        assert!(matches!(any, AnySpec::Multi(_)));
        if let AnySpec::Multi(multi) = any {
            assert_eq!(multi.repos.len(), 2);
        }
        std::fs::remove_dir_all(&dir).ok();
    }
}
