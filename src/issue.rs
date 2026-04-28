//! `Issue` — what we author against an [`crate::IssueTracker`].

use serde::{Deserialize, Serialize};

/// Open or closed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    /// Active issue.
    #[default]
    Open,
    /// Resolved / dropped.
    Closed,
}

/// An issue definition, authored locally before round-tripping through a
/// tracker.
///
/// Natural key is `title` within `(repo, milestone?)`. Trackers MUST be
/// idempotent on natural-key match (re-applying a spec is a no-op when the
/// fields haven't changed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// Title — also the natural key. Empty titles are rejected by the
    /// tracker.
    pub title: String,
    /// Markdown body.
    #[serde(default)]
    pub body: String,
    /// Label names (must already exist or be in the same plan).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub labels: Vec<String>,
    /// Login(s) of the user(s) the issue is assigned to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assignees: Vec<String>,
    /// Optional milestone title (must already exist or be in the same plan).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub milestone: Option<String>,
    /// Issue state.
    #[serde(default)]
    pub state: IssueState,
}

impl Issue {
    /// Construct an open issue with just a title.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: String::new(),
            labels: Vec::new(),
            assignees: Vec::new(),
            milestone: None,
            state: IssueState::Open,
        }
    }

    /// Builder: markdown body.
    #[must_use]
    pub fn with_body(mut self, body: impl Into<String>) -> Self {
        self.body = body.into();
        self
    }

    /// Builder: replace labels with the given iterator.
    #[must_use]
    pub fn with_labels<I, S>(mut self, labels: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.labels = labels.into_iter().map(Into::into).collect();
        self
    }

    /// Builder: replace assignees with the given iterator.
    #[must_use]
    pub fn with_assignees<I, S>(mut self, who: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.assignees = who.into_iter().map(Into::into).collect();
        self
    }

    /// Builder: associate with a milestone (by title).
    #[must_use]
    pub fn with_milestone(mut self, m: impl Into<String>) -> Self {
        self.milestone = Some(m.into());
        self
    }

    /// Builder: set issue state.
    #[must_use]
    pub fn with_state(mut self, s: IssueState) -> Self {
        self.state = s;
        self
    }

    /// Issues without titles are invalid; the rest is up to the backend.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        !self.title.trim().is_empty()
    }
}

/// A returned issue with the backend-assigned numeric id and URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueRef {
    /// Backend-assigned issue number (GitHub: per-repo).
    pub number: u64,
    /// Title at creation/last-sync time.
    pub title: String,
    /// Browser URL.
    pub url: String,
    /// State at last sync.
    #[serde(default)]
    pub state: IssueState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn new_defaults_to_open_no_labels() {
        let i = Issue::new("Fix the auth middleware");
        assert_eq!(i.title, "Fix the auth middleware");
        assert!(i.body.is_empty());
        assert!(i.labels.is_empty());
        assert!(i.assignees.is_empty());
        assert!(i.milestone.is_none());
        assert_eq!(i.state, IssueState::Open);
    }

    #[test]
    fn builders_chain() {
        let i = Issue::new("t")
            .with_body("b")
            .with_labels(["bug", "area:graph"])
            .with_assignees(["Lsh0x"])
            .with_milestone("v0.5")
            .with_state(IssueState::Closed);
        assert_eq!(i.body, "b");
        assert_eq!(i.labels, vec!["bug", "area:graph"]);
        assert_eq!(i.assignees, vec!["Lsh0x"]);
        assert_eq!(i.milestone.as_deref(), Some("v0.5"));
        assert_eq!(i.state, IssueState::Closed);
    }

    #[test]
    fn is_valid_rejects_blank_title() {
        assert!(Issue::new("ok").is_valid());
        assert!(!Issue::new("").is_valid());
        assert!(!Issue::new("   ").is_valid());
    }

    #[test]
    fn state_default_is_open() {
        assert_eq!(IssueState::default(), IssueState::Open);
    }

    #[test]
    fn state_serde_lowercase() {
        assert_eq!(
            serde_json::to_string(&IssueState::Open).expect("ok"),
            "\"open\""
        );
        assert_eq!(
            serde_json::to_string(&IssueState::Closed).expect("ok"),
            "\"closed\""
        );
    }

    #[test]
    fn serde_omits_empty_collections() {
        let i = Issue::new("t");
        let j = serde_json::to_string(&i).expect("ok");
        assert!(!j.contains("labels"), "labels absent: {j}");
        assert!(!j.contains("assignees"), "assignees absent: {j}");
        assert!(!j.contains("milestone"), "milestone absent: {j}");
    }

    #[test]
    fn serde_roundtrip_full() {
        let i = Issue::new("t")
            .with_body("b")
            .with_labels(["x"])
            .with_assignees(["y"])
            .with_milestone("m");
        let j = serde_json::to_string(&i).expect("ok");
        let back: Issue = serde_json::from_str(&j).expect("ok");
        assert_eq!(i, back);
    }

    #[test]
    fn issue_ref_serde() {
        let r = IssueRef {
            number: 42,
            title: "t".into(),
            url: "https://x.test/issues/42".into(),
            state: IssueState::Open,
        };
        let j = serde_json::to_string(&r).expect("ok");
        let back: IssueRef = serde_json::from_str(&j).expect("ok");
        assert_eq!(r, back);
    }
}
