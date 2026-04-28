//! `Milestone` — a release / planning bucket issues can be assigned to.

use serde::{Deserialize, Serialize};

/// Open or closed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MilestoneState {
    /// Active milestone — issues can still be added/completed.
    #[default]
    Open,
    /// Frozen — typically because the release shipped.
    Closed,
}

/// A milestone definition. Natural key is `title` within a repository.
///
/// `due_on` is an ISO-8601 date (`YYYY-MM-DD`). Time-of-day is intentionally
/// not modelled — release dates are coarse-grained and timezone games here
/// only cause confusion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    /// Display title — also the natural key (no two milestones with the same title).
    pub title: String,
    /// Optional long-form description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional due date — ISO-8601 (`YYYY-MM-DD`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_on: Option<String>,
    /// Open by default.
    #[serde(default)]
    pub state: MilestoneState,
}

impl Milestone {
    /// Construct an open milestone with just a title.
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: None,
            due_on: None,
            state: MilestoneState::Open,
        }
    }

    /// Builder: long-form description.
    #[must_use]
    pub fn with_description(mut self, d: impl Into<String>) -> Self {
        self.description = Some(d.into());
        self
    }

    /// Builder: due date (`YYYY-MM-DD`).
    #[must_use]
    pub fn with_due_on(mut self, d: impl Into<String>) -> Self {
        self.due_on = Some(d.into());
        self
    }

    /// Builder: state.
    #[must_use]
    pub fn with_state(mut self, s: MilestoneState) -> Self {
        self.state = s;
        self
    }

    /// True if `due_on` looks like a YYYY-MM-DD ISO-8601 calendar date.
    /// Intentionally cheap — accepts any 10-char string of the right shape.
    #[must_use]
    pub fn has_valid_due_date(&self) -> bool {
        self.due_on.as_deref().is_some_and(|d| {
            d.len() == 10
                && d.as_bytes()[4] == b'-'
                && d.as_bytes()[7] == b'-'
                && d.bytes()
                    .enumerate()
                    .all(|(i, b)| matches!(i, 4 | 7) || b.is_ascii_digit())
        })
    }
}

/// A returned milestone with the backend-assigned numeric id (e.g. GitHub
/// milestone `number`). Distinct from [`Milestone`] so authoring stays
/// id-free.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MilestoneRef {
    /// Backend-assigned id (GitHub: milestone `number`).
    pub id: u64,
    /// Inline title for convenience — same as `Milestone::title`.
    pub title: String,
    /// Open or closed.
    #[serde(default)]
    pub state: MilestoneState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn new_is_open_with_no_optionals() {
        let m = Milestone::new("v0.5");
        assert_eq!(m.title, "v0.5");
        assert_eq!(m.state, MilestoneState::Open);
        assert!(m.description.is_none());
        assert!(m.due_on.is_none());
    }

    #[test]
    fn builders_chain() {
        let m = Milestone::new("v0.5")
            .with_description("multi-tenant work")
            .with_due_on("2026-06-01")
            .with_state(MilestoneState::Closed);
        assert_eq!(m.description.as_deref(), Some("multi-tenant work"));
        assert_eq!(m.due_on.as_deref(), Some("2026-06-01"));
        assert_eq!(m.state, MilestoneState::Closed);
    }

    #[test]
    fn has_valid_due_date_accepts_iso() {
        assert!(
            Milestone::new("x")
                .with_due_on("2026-06-01")
                .has_valid_due_date()
        );
        assert!(
            !Milestone::new("x")
                .with_due_on("06/01/2026")
                .has_valid_due_date()
        );
        assert!(
            !Milestone::new("x")
                .with_due_on("not a date")
                .has_valid_due_date()
        );
        assert!(!Milestone::new("x").has_valid_due_date());
    }

    #[test]
    fn state_default_is_open() {
        assert_eq!(MilestoneState::default(), MilestoneState::Open);
    }

    #[test]
    fn state_serde_lowercase() {
        let s = serde_json::to_string(&MilestoneState::Closed).expect("ok");
        assert_eq!(s, "\"closed\"");
    }

    #[test]
    fn serde_roundtrip() {
        let m = Milestone::new("v0.5")
            .with_description("x")
            .with_due_on("2026-06-01");
        let json = serde_json::to_string(&m).expect("serialize");
        let back: Milestone = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(m, back);
    }

    #[test]
    fn milestone_ref_serde() {
        let r = MilestoneRef {
            id: 42,
            title: "v0.5".into(),
            state: MilestoneState::Open,
        };
        let j = serde_json::to_string(&r).expect("ok");
        let back: MilestoneRef = serde_json::from_str(&j).expect("ok");
        assert_eq!(r, back);
    }
}
