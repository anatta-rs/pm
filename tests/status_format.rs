//! Integration tests for pm-status markdown rendering.

use pm::status::{MilestoneSummary, PrSummary, RepoSummary};

#[test]
fn test_render_repo_with_milestones() {
    let summaries = vec![RepoSummary {
        owner: "anatta-rs".to_string(),
        repo: "anatta".to_string(),
        open_prs: vec![PrSummary {
            number: 42,
            title: "Fix auth middleware".to_string(),
            merge_state: "MERGEABLE".to_string(),
        }],
        open_issues: 5,
        milestones: vec![MilestoneSummary {
            title: "v0.2".to_string(),
            due_on: Some("2026-05-15".to_string()),
            open: 3,
            closed: 7,
            description: Some("Major release".to_string()),
        }],
    }];

    let md = pm::status::render_markdown(&summaries, "anatta-rs", "2026-04-28T12:00Z");

    // 1. Header
    assert!(md.contains("# pm-status — 2026-04-28T12:00Z"));

    // 2. Per-repo table
    assert!(md.contains("| anatta-rs/anatta |"));
    assert!(md.contains("| 1 |")); // 1 PR
    assert!(md.contains("| 5 |")); // 5 issues
    assert!(md.contains("v0.2 [7/10]")); // milestone in table

    // 3. Aggregate
    assert!(md.contains("**Totals:** 1 PRs, 5 issues, 1 milestones"));

    // 4. Cross-repo milestones
    assert!(md.contains("## Milestones (all repos)"));
    assert!(md.contains("| v0.2 |"));
    assert!(md.contains("| anatta-rs/anatta |"));
    assert!(md.contains("70%")); // 7 closed / 10 total

    // 5. In-flight PRs
    assert!(md.contains("## In-flight PRs"));
    assert!(md.contains("| #42 | Fix auth middleware |"));
    assert!(md.contains("| MERGEABLE |"));
}

#[test]
fn test_render_no_milestones_omitted() {
    let summaries = vec![RepoSummary {
        owner: "test".to_string(),
        repo: "repo".to_string(),
        open_prs: vec![],
        open_issues: 0,
        milestones: vec![],
    }];

    let md = pm::status::render_markdown(&summaries, "test", "2026-04-28T00:00Z");

    // Milestones section should not appear
    assert!(!md.contains("## Milestones (all repos)"));
    // In-flight PRs section should not appear
    assert!(!md.contains("## In-flight PRs"));
}

#[test]
fn test_render_multiple_repos_aggregation() {
    let summaries = vec![
        RepoSummary {
            owner: "anatta-rs".to_string(),
            repo: "pm".to_string(),
            open_prs: vec![PrSummary {
                number: 1,
                title: "Add status binary".to_string(),
                merge_state: "MERGEABLE".to_string(),
            }],
            open_issues: 2,
            milestones: vec![],
        },
        RepoSummary {
            owner: "anatta-rs".to_string(),
            repo: "anatta".to_string(),
            open_prs: vec![],
            open_issues: 3,
            milestones: vec![MilestoneSummary {
                title: "v1.0".to_string(),
                due_on: Some("2026-06-01".to_string()),
                open: 5,
                closed: 10,
                description: None,
            }],
        },
        RepoSummary {
            owner: "Lsh0x".to_string(),
            repo: "rs-stats".to_string(),
            open_prs: vec![],
            open_issues: 1,
            milestones: vec![],
        },
    ];

    let md = pm::status::render_markdown(&summaries, "anatta-rs,Lsh0x", "2026-04-28T12:00Z");

    // Aggregate totals
    assert!(md.contains("**Totals:** 1 PRs, 6 issues, 1 milestones"));

    // All repos in table
    assert!(md.contains("| anatta-rs/pm |"));
    assert!(md.contains("| anatta-rs/anatta |"));
    assert!(md.contains("| Lsh0x/rs-stats |"));

    // Milestones section with cross-repo table
    assert!(md.contains("## Milestones (all repos)"));
    assert!(md.contains("| v1.0 |"));
    assert!(md.contains("| anatta-rs/anatta |")); // in milestones table

    // In-flight PRs section
    assert!(md.contains("## In-flight PRs"));
    assert!(md.contains("| #1 | Add status binary |"));
}

#[test]
fn test_milestone_progress_calculation() {
    let summaries = vec![RepoSummary {
        owner: "test".to_string(),
        repo: "repo".to_string(),
        open_prs: vec![],
        open_issues: 0,
        milestones: vec![
            MilestoneSummary {
                title: "m1".to_string(),
                due_on: None,
                open: 0,
                closed: 0,
                description: None,
            },
            MilestoneSummary {
                title: "m2".to_string(),
                due_on: Some("2026-05-20".to_string()),
                open: 1,
                closed: 3,
                description: None,
            },
        ],
    }];

    let md = pm::status::render_markdown(&summaries, "test", "2026-04-28T00:00Z");

    // m1: 0%
    assert!(md.contains("| m1 |"));
    // m2: 3/4 = 75%
    assert!(md.contains("| m2 |"));
    assert!(md.contains("75%"));
}

#[test]
fn test_milestones_sorted_by_due_date() {
    let summaries = vec![RepoSummary {
        owner: "test".to_string(),
        repo: "repo".to_string(),
        open_prs: vec![],
        open_issues: 0,
        milestones: vec![
            MilestoneSummary {
                title: "late".to_string(),
                due_on: Some("2026-06-01".to_string()),
                open: 0,
                closed: 0,
                description: None,
            },
            MilestoneSummary {
                title: "soon".to_string(),
                due_on: Some("2026-05-01".to_string()),
                open: 0,
                closed: 0,
                description: None,
            },
            MilestoneSummary {
                title: "undated".to_string(),
                due_on: None,
                open: 0,
                closed: 0,
                description: None,
            },
        ],
    }];

    let md = pm::status::render_markdown(&summaries, "test", "2026-04-28T00:00Z");

    // Find positions in the cross-repo milestones table (after the section header)
    let milestone_section = md
        .find("## Milestones (all repos)")
        .expect("milestones section not found");
    let remaining = &md[milestone_section..];

    let soon_pos = remaining.find("soon").expect("soon not found");
    let late_pos = remaining.find("late").expect("late not found");
    let undated_pos = remaining.find("undated").expect("undated not found");

    // soon (2026-05-01) should come before late (2026-06-01)
    assert!(
        soon_pos < late_pos,
        "soon should appear before late in milestones section"
    );
    // late should come before undated (null dates last)
    assert!(
        late_pos < undated_pos,
        "late should appear before undated in milestones section"
    );
}
