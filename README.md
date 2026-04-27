# pm

> Bulk project management as a YAML file. Author your labels, milestones,
> and issues once; apply them anywhere; re-run as often as you want.

`pm` is for the moment in a project where you go _"I need to file twenty
issues across three milestones, and I'd rather not click them in one by
one."_ You write them in a spec file, run `pm apply`, and walk away —
and re-running tomorrow is a no-op.

```yaml
# plan.yaml
repo: anatta-rs/anatta

labels:
  - { name: "type:bug",   color: "d73a4a", description: "Something is broken" }
  - { name: "area:graph", color: "0075ca" }

milestones:
  - title: "v0.5 — Multi-tenant"
    description: "GitHub-orgs style namespace model"
    due_on: "2026-06-01"

issues:
  - title: "I7: fix /api/v1/health 401"
    body: |
      Hook blocks before handler — health probe gets 401.
      Add `/api/v1/health` to BOOTSTRAP_WRITE_PATHS.
    milestone: "v0.5 — Multi-tenant"
    labels: ["type:bug"]
    assignees: ["Lsh0x"]
```

```sh
export GITHUB_TOKEN=ghp_…
pm apply plan.yaml
# ✓ applied anatta-rs/anatta: 2 label(s), 1 milestone(s), 1 issue(s)
```

## Crates

| Crate | What it is |
|---|---|
| [`pm-core`](crates/pm-core)     | Trait + types. `IssueTracker`, `Issue`, `Milestone`, `Label`, `PmError`. Zero backends. |
| [`pm-github`](crates/pm-github) | GitHub Issues backend (REST API v3). Bearer auth via PAT. |
| [`pm-cli`](crates/pm-cli)       | The `pm` binary — YAML/JSON spec parser + `apply` / `list` commands. |

## Why a trait

The same shape works for GitLab, Forgejo, Linear, Jira — pick whichever
fits and the spec file doesn't change. v1 ships GitHub; backends are
~300 LOC each, so a contributor can land a new tracker in an afternoon.

## Idempotency contract

The trait promises **upsert by natural key**:

- labels by `name`,
- milestones by `title`,
- issues by `title`.

`pm apply plan.yaml` run twice in a row creates everything once and
no-ops on the second pass — no duplicate issues, no orphaned milestones.
This is what makes the spec file safe to keep in git and re-run from CI.

## Contributing

```sh
make hooks    # install pre-commit + pre-push
make check    # fmt + clippy + test
make ci       # full CI including coverage gate (≥ 95%)
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
