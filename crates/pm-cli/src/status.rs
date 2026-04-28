//! Cross-repo project status snapshot: PRs, issues, milestones.

use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// Summary of a pull request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrSummary {
    /// PR number.
    pub number: u64,
    /// PR title.
    pub title: String,
    /// Merge state (e.g., "MERGEABLE", "CONFLICTING", "BEHIND").
    pub merge_state: String,
}

/// Summary of a milestone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MilestoneSummary {
    /// Milestone title.
    pub title: String,
    /// Due date (ISO 8601, e.g., "2026-05-15").
    pub due_on: Option<String>,
    /// Open issues count.
    pub open: u32,
    /// Closed issues count.
    pub closed: u32,
    /// Milestone description (optional).
    pub description: Option<String>,
}

/// Summary of a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSummary {
    /// Repository owner.
    pub owner: String,
    /// Repository name.
    pub repo: String,
    /// Open pull requests.
    pub open_prs: Vec<PrSummary>,
    /// Count of open issues (excluding pull requests).
    pub open_issues: u32,
    /// Milestones across all states.
    pub milestones: Vec<MilestoneSummary>,
}

/// Render a cross-repo status as Markdown.
///
/// Includes:
/// 1. Header with UTC timestamp.
/// 2. Per-repo table with PR/issue/milestone counts.
/// 3. Aggregate totals.
/// 4. Cross-repo milestones table (if any), sorted by due date.
/// 5. In-flight PRs table (if any).
#[must_use]
pub fn render_markdown(summaries: &[RepoSummary], _scope: &str, now_utc: &str) -> String {
    let mut out = String::new();

    // 1. Header
    let _ = writeln!(out, "# pm-status — {now_utc}\n");

    // 2. Per-repo table
    out.push_str("## Repos\n\n");
    out.push_str("| Repo | Open PRs | Open Issues | Milestones (open) |\n");
    out.push_str("|------|----------|-------------|-------------------|\n");

    let mut total_prs = 0u64;
    let mut total_issues = 0u32;
    let mut all_milestones = Vec::new();

    for summary in summaries {
        let repo_link = format!("{}/{}", summary.owner, summary.repo);
        let open_prs = summary.open_prs.len();
        total_prs += open_prs as u64;

        total_issues += summary.open_issues;

        let milestone_cell = if summary.milestones.is_empty() {
            String::new()
        } else {
            summary
                .milestones
                .iter()
                .map(|m| {
                    let total = m.open + m.closed;
                    format!("{} [{}/{}]", m.title, m.closed, total)
                })
                .collect::<Vec<_>>()
                .join("<br>")
        };

        let _ = writeln!(
            out,
            "| {} | {} | {} | {} |",
            repo_link, open_prs, summary.open_issues, milestone_cell
        );

        // Collect for cross-repo milestone table
        for m in &summary.milestones {
            all_milestones.push((summary.owner.clone(), summary.repo.clone(), m.clone()));
        }
    }

    // 3. Aggregate
    let _ = writeln!(
        out,
        "\n**Totals:** {} PRs, {} issues, {} milestones\n",
        total_prs,
        total_issues,
        all_milestones.len()
    );

    // 4. Cross-repo milestones (if any)
    if !all_milestones.is_empty() {
        out.push_str("## Milestones (all repos)\n\n");
        out.push_str("| Milestone | Repo | Progress | Due | Description |\n");
        out.push_str("|-----------|------|----------|-----|--------------|\n");

        // Sort by due_on (nulls last), then by title
        all_milestones.sort_by(|a, b| {
            let (_, _, m_a) = a;
            let (_, _, m_b) = b;
            match (&m_a.due_on, &m_b.due_on) {
                (None, None) => m_a.title.cmp(&m_b.title),
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (Some(_), None) => std::cmp::Ordering::Less,
                (Some(d_a), Some(d_b)) => d_a.cmp(d_b).then_with(|| m_a.title.cmp(&m_b.title)),
            }
        });

        for (owner, repo, m) in &all_milestones {
            let total = m.open + m.closed;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let progress = if total == 0 {
                String::from("0%")
            } else {
                let pct = (f64::from(m.closed) / f64::from(total) * 100.0).round() as u32;
                format!("{pct}%")
            };
            let due = m.due_on.as_deref().unwrap_or("—");
            let desc = m.description.as_deref().unwrap_or("");
            let _ = writeln!(
                out,
                "| {} | {}/{} | {}/{} {} | {} | {} |",
                m.title, owner, repo, m.closed, total, progress, due, desc
            );
        }

        out.push('\n');
    }

    // 5. In-flight PRs (if any)
    let all_prs: Vec<_> = summaries
        .iter()
        .flat_map(|s| {
            s.open_prs
                .iter()
                .map(move |pr| (s.owner.clone(), s.repo.clone(), pr.clone()))
        })
        .collect();

    if !all_prs.is_empty() {
        out.push_str("## In-flight PRs\n\n");
        out.push_str("| Repo | PR | Title | Status |\n");
        out.push_str("|------|----|----|--------|\n");

        for (owner, repo, pr) in &all_prs {
            let _ = writeln!(
                out,
                "| {}/{} | #{} | {} | {} |",
                owner, repo, pr.number, pr.title, pr.merge_state
            );
        }

        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_no_milestones() {
        let summaries = vec![RepoSummary {
            owner: "anatta-rs".to_string(),
            repo: "pm".to_string(),
            open_prs: vec![],
            open_issues: 0,
            milestones: vec![],
        }];

        let md = render_markdown(&summaries, "anatta-rs", "2026-04-28T12:00Z");
        assert!(md.contains("# pm-status — 2026-04-28T12:00Z"));
        assert!(md.contains("| anatta-rs/pm |"));
        assert!(!md.contains("## Milestones (all repos)"));
        assert!(!md.contains("## In-flight PRs"));
    }

    #[test]
    fn test_render_with_milestones() {
        let summaries = vec![RepoSummary {
            owner: "anatta-rs".to_string(),
            repo: "anatta".to_string(),
            open_prs: vec![PrSummary {
                number: 42,
                title: "Fix auth".to_string(),
                merge_state: "MERGEABLE".to_string(),
            }],
            open_issues: 5,
            milestones: vec![MilestoneSummary {
                title: "v0.2".to_string(),
                due_on: Some("2026-05-15".to_string()),
                open: 3,
                closed: 7,
                description: Some("Release milestone".to_string()),
            }],
        }];

        let md = render_markdown(&summaries, "anatta-rs", "2026-04-28T12:00Z");
        assert!(md.contains("## Milestones (all repos)"));
        assert!(md.contains("| v0.2 |"));
        assert!(md.contains("[7/10]"));
        assert!(md.contains("## In-flight PRs"));
        assert!(md.contains("| #42 | Fix auth |"));
    }

    #[test]
    fn test_render_multiple_repos() {
        let summaries = vec![
            RepoSummary {
                owner: "anatta-rs".to_string(),
                repo: "pm".to_string(),
                open_prs: vec![],
                open_issues: 2,
                milestones: vec![],
            },
            RepoSummary {
                owner: "Lsh0x".to_string(),
                repo: "rs-stats".to_string(),
                open_prs: vec![PrSummary {
                    number: 1,
                    title: "Add benchmarks".to_string(),
                    merge_state: "CONFLICTING".to_string(),
                }],
                open_issues: 1,
                milestones: vec![],
            },
        ];

        let md = render_markdown(&summaries, "anatta-rs,Lsh0x", "2026-04-28T12:00Z");
        assert!(md.contains("**Totals:** 1 PRs, 3 issues"));
        assert!(md.contains("| anatta-rs/pm |"));
        assert!(md.contains("| Lsh0x/rs-stats |"));
    }
}
