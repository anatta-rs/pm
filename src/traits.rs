//! `IssueTracker` — the trait every backend implements.

use crate::error::Result;
use crate::issue::{Issue, IssueRef};
use crate::label::Label;
use crate::milestone::{Milestone, MilestoneRef};
use async_trait::async_trait;

/// A repository-scoped project-management backend.
///
/// Implementations hold their own auth + repo coordinates and expose
/// **upsert-by-natural-key** primitives:
///
/// - labels are keyed by `name`,
/// - milestones by `title`,
/// - issues by `(title, milestone?)`.
///
/// Re-running the same plan against the same backend MUST be a no-op
/// (modulo `updated_at`-style metadata).
///
/// `Send + Sync` so trackers can be shared across async tasks.
#[async_trait]
pub trait IssueTracker: Send + Sync {
    /// Backend identifier (`"github"`, `"gitlab"`, …) — used by consumers
    /// for logging.
    fn name(&self) -> &'static str;

    /// Create the label if it does not exist; otherwise update colour /
    /// description if they differ.
    async fn upsert_label(&self, label: &Label) -> Result<Label>;

    /// List every label defined on the repo.
    async fn list_labels(&self) -> Result<Vec<Label>>;

    /// Create the milestone (matched by `title`) or update its
    /// description / due date / state.
    async fn upsert_milestone(&self, m: &Milestone) -> Result<MilestoneRef>;

    /// List every milestone on the repo.
    async fn list_milestones(&self) -> Result<Vec<MilestoneRef>>;

    /// Create the issue (matched by `title`) or update body / labels /
    /// assignees / milestone / state.
    async fn upsert_issue(&self, issue: &Issue) -> Result<IssueRef>;

    /// List every issue on the repo (open + closed).
    async fn list_issues(&self) -> Result<Vec<IssueRef>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IssueState, MilestoneState, PmError};
    use std::sync::Mutex;

    struct FakeTracker {
        name: &'static str,
        labels: Mutex<Vec<Label>>,
        milestones: Mutex<Vec<MilestoneRef>>,
        issues: Mutex<Vec<IssueRef>>,
    }

    impl FakeTracker {
        fn new(name: &'static str) -> Self {
            Self {
                name,
                labels: Mutex::new(Vec::new()),
                milestones: Mutex::new(Vec::new()),
                issues: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl IssueTracker for FakeTracker {
        fn name(&self) -> &'static str {
            self.name
        }

        async fn upsert_label(&self, label: &Label) -> Result<Label> {
            let mut g = self.labels.lock().expect("poisoned");
            if let Some(existing) = g.iter_mut().find(|l| l.name == label.name) {
                *existing = label.clone();
            } else {
                g.push(label.clone());
            }
            Ok(label.clone())
        }

        async fn list_labels(&self) -> Result<Vec<Label>> {
            Ok(self.labels.lock().expect("poisoned").clone())
        }

        async fn upsert_milestone(&self, m: &Milestone) -> Result<MilestoneRef> {
            let mut g = self.milestones.lock().expect("poisoned");
            if let Some(existing) = g.iter_mut().find(|x| x.title == m.title) {
                existing.state = m.state;
                return Ok(existing.clone());
            }
            let new_ref = MilestoneRef {
                id: u64::try_from(g.len() + 1).unwrap_or(1),
                title: m.title.clone(),
                state: m.state,
            };
            g.push(new_ref.clone());
            Ok(new_ref)
        }

        async fn list_milestones(&self) -> Result<Vec<MilestoneRef>> {
            Ok(self.milestones.lock().expect("poisoned").clone())
        }

        async fn upsert_issue(&self, issue: &Issue) -> Result<IssueRef> {
            if !issue.is_valid() {
                return Err(PmError::InvalidInput("empty title".into()));
            }
            let mut g = self.issues.lock().expect("poisoned");
            if let Some(existing) = g.iter_mut().find(|x| x.title == issue.title) {
                existing.state = issue.state;
                return Ok(existing.clone());
            }
            let new_ref = IssueRef {
                number: u64::try_from(g.len() + 1).unwrap_or(1),
                title: issue.title.clone(),
                url: format!("https://fake/issues/{}", g.len() + 1),
                state: issue.state,
            };
            g.push(new_ref.clone());
            Ok(new_ref)
        }

        async fn list_issues(&self) -> Result<Vec<IssueRef>> {
            Ok(self.issues.lock().expect("poisoned").clone())
        }
    }

    #[tokio::test]
    async fn name_is_stable() {
        let t = FakeTracker::new("fake");
        assert_eq!(t.name(), "fake");
    }

    #[tokio::test]
    async fn upsert_label_is_idempotent() {
        let t = FakeTracker::new("fake");
        t.upsert_label(&Label::new("bug").with_color("d73a4a"))
            .await
            .expect("ok");
        t.upsert_label(&Label::new("bug").with_color("d73a4a"))
            .await
            .expect("ok");
        let all = t.list_labels().await.expect("ok");
        assert_eq!(all.len(), 1, "labels deduplicated by name");
    }

    #[tokio::test]
    async fn upsert_milestone_returns_id() {
        let t = FakeTracker::new("fake");
        let r = t
            .upsert_milestone(&Milestone::new("v0.5").with_state(MilestoneState::Open))
            .await
            .expect("ok");
        assert_eq!(r.title, "v0.5");
        assert_eq!(r.id, 1);
    }

    #[tokio::test]
    async fn upsert_issue_rejects_empty_title() {
        let t = FakeTracker::new("fake");
        let err = t
            .upsert_issue(&Issue::new(""))
            .await
            .expect_err("must fail");
        assert!(matches!(err, PmError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn upsert_issue_is_idempotent_by_title() {
        let t = FakeTracker::new("fake");
        let r1 = t.upsert_issue(&Issue::new("X")).await.expect("ok");
        let r2 = t
            .upsert_issue(&Issue::new("X").with_state(IssueState::Closed))
            .await
            .expect("ok");
        assert_eq!(r1.number, r2.number);
        assert_eq!(r2.state, IssueState::Closed);
    }
}
