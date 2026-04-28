//! Apply a [`Spec`] or [`MultiSpec`] against an [`IssueTracker`] — labels first, then
//! milestones, then issues. Reports a per-section count of upserts.

use crate::spec::{MultiSpec, Spec};
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

/// Per-repo report when applying a multi-repo spec.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiRepoReport {
    /// Target repo (e.g., `owner/name`).
    pub repo: String,
    /// Results for this repo.
    pub report: ApplyReport,
}

/// Summary of a multi-repo apply operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiApplyReport {
    /// Per-repo breakdown.
    pub repos: Vec<MultiRepoReport>,
    /// Total across all repos.
    pub totals: ApplyReport,
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

/// Build a single-repo [`Spec`] from a [`MultiSpec`] for a specific repo.
/// Includes shared labels/milestones and filters issues to those targeting the repo.
fn spec_for_repo(multi: &MultiSpec, repo: &str) -> Spec {
    let issues = multi
        .issues
        .iter()
        .filter(|issue| issue.repo == repo)
        .map(|issue| crate::spec::SpecIssue {
            title: issue.title.clone(),
            body: issue.body.clone(),
            labels: issue.labels.clone(),
            assignees: issue.assignees.clone(),
            milestone: issue.milestone.clone(),
            state: issue.state,
        })
        .collect();

    Spec {
        repo: repo.to_string(),
        labels: multi.shared_labels.clone(),
        milestones: multi.shared_milestones.clone(),
        issues,
    }
}

/// Apply a multi-repo spec by building and applying a single-repo spec for each repo.
/// All repos receive the shared labels/milestones; issues are routed by their `repo` field.
pub async fn apply_multi(
    multi: &MultiSpec,
    tracker_fn: impl Fn(&str) -> Result<Box<dyn IssueTracker>>,
) -> Result<MultiApplyReport> {
    let mut repos = vec![];
    let mut totals = ApplyReport::default();

    for repo in &multi.repos {
        let tracker = tracker_fn(repo)?;
        let spec = spec_for_repo(multi, repo);
        let report = apply_dyn(&spec, tracker.as_ref()).await?;

        totals.labels = totals.labels.saturating_add(report.labels);
        totals.milestones = totals.milestones.saturating_add(report.milestones);
        totals.issues = totals.issues.saturating_add(report.issues);

        repos.push(MultiRepoReport {
            repo: repo.clone(),
            report,
        });
    }

    Ok(MultiApplyReport { repos, totals })
}

/// Apply a spec against a boxed trait object.
async fn apply_dyn(spec: &Spec, tracker: &dyn IssueTracker) -> Result<ApplyReport> {
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
    use crate::spec::{MultiSpecIssue, SpecIssue, SpecLabel, SpecMilestone};
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

    #[test]
    fn spec_for_repo_builds_correct_spec() {
        let multi = MultiSpec {
            repos: vec!["o1/r1".into(), "o2/r2".into()],
            shared_labels: vec![SpecLabel {
                name: "bug".into(),
                color: None,
                description: None,
            }],
            shared_milestones: vec![SpecMilestone {
                title: "v0.5".into(),
                description: None,
                due_on: None,
                state: MilestoneState::Open,
            }],
            issues: vec![
                MultiSpecIssue {
                    repo: "o1/r1".into(),
                    title: "Issue for r1".into(),
                    body: String::new(),
                    labels: vec!["bug".into()],
                    assignees: vec![],
                    milestone: Some("v0.5".into()),
                    state: IssueState::Open,
                },
                MultiSpecIssue {
                    repo: "o2/r2".into(),
                    title: "Issue for r2".into(),
                    body: String::new(),
                    labels: vec![],
                    assignees: vec![],
                    milestone: None,
                    state: IssueState::Open,
                },
            ],
        };

        let spec1 = super::spec_for_repo(&multi, "o1/r1");
        assert_eq!(spec1.repo, "o1/r1");
        assert_eq!(spec1.labels.len(), 1);
        assert_eq!(spec1.milestones.len(), 1);
        assert_eq!(spec1.issues.len(), 1);
        assert_eq!(spec1.issues[0].title, "Issue for r1");

        let spec2 = super::spec_for_repo(&multi, "o2/r2");
        assert_eq!(spec2.repo, "o2/r2");
        assert_eq!(spec2.labels.len(), 1);
        assert_eq!(spec2.milestones.len(), 1);
        assert_eq!(spec2.issues.len(), 1);
        assert_eq!(spec2.issues[0].title, "Issue for r2");
    }

    #[tokio::test]
    async fn apply_multi_applies_to_all_repos() {
        let multi = MultiSpec {
            repos: vec!["o1/r1".into(), "o2/r2".into()],
            shared_labels: vec![SpecLabel {
                name: "shared".into(),
                color: None,
                description: None,
            }],
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

        let trackers: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(vec![]);

        let tracker_fn = |repo: &str| -> Result<Box<dyn IssueTracker>> {
            trackers.lock().expect("p").push(repo.to_string());
            Ok(Box::new(Counter::default()))
        };

        let report = super::apply_multi(&multi, tracker_fn).await.expect("ok");

        assert_eq!(report.repos.len(), 2);
        assert_eq!(report.repos[0].repo, "o1/r1");
        assert_eq!(report.repos[0].report.labels, 1);
        assert_eq!(report.repos[0].report.issues, 1);
        assert_eq!(report.repos[1].repo, "o2/r2");
        assert_eq!(report.repos[1].report.labels, 1);
        assert_eq!(report.repos[1].report.issues, 1);
        assert_eq!(report.totals.labels, 2);
        assert_eq!(report.totals.issues, 2);
    }
}
