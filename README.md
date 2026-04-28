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
| [`pm-cli`](crates/pm-cli)       | Binaries: `pm` (YAML/JSON spec + apply/list), `pm-status` (cross-repo project snapshot). |

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

## CLI tools

### `pm apply` — declarative project spec

#### Single-repo spec

```sh
export GITHUB_TOKEN=ghp_…
pm apply plan.yaml
# ✓ applied anatta-rs/anatta: 2 label(s), 1 milestone(s), 1 issue(s)
```

Idempotent YAML/JSON spec to GitHub — upsert labels, milestones, and issues by
natural key (`name`, `title`). Same spec re-runs forever without duplicates.

#### Multi-repo spec

Apply the same labels and milestones across multiple repositories, with per-repo issues:

```yaml
# org-plan.yaml
repos:
  - anatta-rs/Anatta
  - anatta-rs/pm
  - anatta-rs/dork

shared_labels:
  - { name: "type:bug",     color: "d73a4a", description: "Something is broken" }
  - { name: "area:graph",   color: "0075ca" }

shared_milestones:
  - title: "v0.5 — Multi-tenant"
    description: "GitHub-orgs style namespace model"
    due_on: "2026-06-01"

issues:
  - repo: anatta-rs/Anatta
    title: "I7: fix /api/v1/health 401"
    labels: ["type:bug"]
    milestone: "v0.5 — Multi-tenant"
  - repo: anatta-rs/pm
    title: "Multi-repo apply support"
    labels: ["type:bug"]
  - repo: anatta-rs/dork
    title: "Archive after v0.5"
    labels: ["type:bug"]
    milestone: "v0.5 — Multi-tenant"
```

```sh
pm apply org-plan.yaml
# multi-spec: 3 repos × 2 labels × 1 milestones × 3 issues
# ✓ anatta-rs/Anatta         2 label(s) (new 0), 1 milestone(s) (new 1), 1 issue(s) (new 1)
# ✓ anatta-rs/pm            2 label(s) (new 2), 1 milestone(s) (new 1), 1 issue(s) (new 1)
# ✓ anatta-rs/dork          2 label(s) (new 0), 1 milestone(s) (new 0), 1 issue(s) (new 1)
```

The format auto-detects: if `repos:` (array) is present, it's treated as multi-repo.
All repos receive `shared_labels` and `shared_milestones` (idempotent upsert).
Each issue must have a `repo:` field matching one in the `repos:` list.

### `pm-status` — cross-repo snapshot

```sh
pm-status anatta-rs,Lsh0x
# Scans all repos in owners anatta-rs and Lsh0x.
# Emits Markdown: open PRs, milestones with progress bars, in-flight task statuses.
```

Flags:
- `[SCOPE]` — comma-separated GitHub owners or `owner/repo` pairs (default: `anatta-rs,Lsh0x`).
- Auth via `GITHUB_TOKEN` env, falls back to `gh auth token`.

Output: Markdown to stdout, ready to paste into wikis or Slack.

## Contributing

```sh
make hooks    # install pre-commit + pre-push
make check    # fmt + clippy + test
make ci       # full CI including coverage gate (≥ 95%)
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
