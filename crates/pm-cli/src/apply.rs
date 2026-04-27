//! Apply a [`Spec`] against an [`IssueTracker`] — labels first, then
//! milestones, then issues. Reports a per-section count of upserts.

use crate::spec::Spec;
use pm_core::{IssueTracker, Result};

/// What `apply` did. All counts represent successful upsert calls; the
/// trait promises idempotency, so re-running yields the same numbers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyReport {
    /// Number of labels upserted.
    pub labels: usize,
    /// Number of milestones upserted.
    pub milestones: usize,
    /// Number of issues upserted.
    pub issues: usize,
}

/// Apply the spec to `tracker`. Order is labels → milestones → issues so
/// that issues can reference them.
pub async fn apply<T: IssueTracker>(spec: &Spec, tracker: &T) -> Result<ApplyReport> {
    let mut report = ApplyReport::default();

    for l in &spec.labels {
        tracker.upsert_label(&l.clone().into()).await?;
        report.labels += 1;
    }
    for m in &spec.milestones {
        tracker.upsert_milestone(&m.clone().into()).await?;
        report.milestones += 1;
    }
    for i in &spec.issues {
        tracker.upsert_issue(&i.clone().into()).await?;
        report.issues += 1;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::{SpecIssue, SpecLabel, SpecMilestone};
    use pm_core::{
        Issue, IssueRef, IssueState, Label, Milestone, MilestoneRef, MilestoneState, PmError,
    };
    use std::sync::Mutex;

    #[derive(Default)]
    struct Counter {
        labels: Mutex<usize>,
        milestones: Mutex<usize>,
        issues: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl IssueTracker for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }
        async fn upsert_label(&self, l: &Label) -> Result<Label> {
            *self.labels.lock().expect("p") += 1;
            Ok(l.clone())
        }
        async fn list_labels(&self) -> Result<Vec<Label>> {
            Ok(vec![])
        }
        async fn upsert_milestone(&self, m: &Milestone) -> Result<MilestoneRef> {
            *self.milestones.lock().expect("p") += 1;
            Ok(MilestoneRef {
                id: 1,
                title: m.title.clone(),
                state: m.state,
            })
        }
        async fn list_milestones(&self) -> Result<Vec<MilestoneRef>> {
            Ok(vec![])
        }
        async fn upsert_issue(&self, i: &Issue) -> Result<IssueRef> {
            if !i.is_valid() {
                return Err(PmError::InvalidInput("empty title".into()));
            }
            *self.issues.lock().expect("p") += 1;
            Ok(IssueRef {
                number: 1,
                title: i.title.clone(),
                url: "u".into(),
                state: i.state,
            })
        }
        async fn list_issues(&self) -> Result<Vec<IssueRef>> {
            Ok(vec![])
        }
    }

    fn sample_spec() -> Spec {
        Spec {
            repo: "o/r".into(),
            labels: vec![SpecLabel {
                name: "bug".into(),
                color: None,
                description: None,
            }],
            milestones: vec![SpecMilestone {
                title: "v0.5".into(),
                description: None,
                due_on: None,
                state: MilestoneState::Open,
            }],
            issues: vec![
                SpecIssue {
                    title: "fix it".into(),
                    body: String::new(),
                    labels: vec!["bug".into()],
                    assignees: vec![],
                    milestone: Some("v0.5".into()),
                    state: IssueState::Open,
                },
                SpecIssue {
                    title: "fix it again".into(),
                    body: String::new(),
                    labels: vec![],
                    assignees: vec![],
                    milestone: None,
                    state: IssueState::Open,
                },
            ],
        }
    }

    #[tokio::test]
    async fn apply_counts_each_section() {
        let counter = Counter::default();
        let report = apply(&sample_spec(), &counter).await.expect("ok");
        assert_eq!(report.labels, 1);
        assert_eq!(report.milestones, 1);
        assert_eq!(report.issues, 2);
    }

    #[tokio::test]
    async fn apply_reports_zero_for_empty_spec() {
        let counter = Counter::default();
        let spec = Spec {
            repo: "o/r".into(),
            labels: vec![],
            milestones: vec![],
            issues: vec![],
        };
        let report = apply(&spec, &counter).await.expect("ok");
        assert_eq!(report, ApplyReport::default());
    }

    #[tokio::test]
    async fn apply_propagates_tracker_error() {
        let counter = Counter::default();
        let mut spec = sample_spec();
        spec.issues.push(SpecIssue {
            title: String::new(),
            body: String::new(),
            labels: vec![],
            assignees: vec![],
            milestone: None,
            state: IssueState::Open,
        });
        let err = apply(&spec, &counter).await.expect_err("must fail");
        assert!(matches!(err, PmError::InvalidInput(_)));
    }
}
