# Bootstrap specs

One YAML per `anatta-rs/*` repo. `make bootstrap` upserts everything
listed here against GitHub. Re-runs are no-ops thanks to upsert-by-
natural-key.

## Usage

```sh
export GITHUB_TOKEN=ghp_…   # PAT with issues:write on every target repo
make bootstrap-dry          # eyeball what's about to land
make bootstrap              # apply all specs
```

## Files

| Spec | Repo | Issue count |
|---|---|---|
| `anatta-rs-polystore.yaml`      | [`polystore`](https://github.com/anatta-rs/polystore)           | 8 |
| `anatta-rs-ast-to-mermaid.yaml` | [`ast-to-mermaid`](https://github.com/anatta-rs/ast-to-mermaid) | 7 |
| `anatta-rs-ingester.yaml`       | [`ingester`](https://github.com/anatta-rs/ingester)             | 10 |
| `anatta-rs-dork.yaml`           | [`dork`](https://github.com/anatta-rs/dork)                     | 7 |
| `anatta-rs-pm.yaml`             | [`pm`](https://github.com/anatta-rs/pm)                         | 9 |

## Editing specs

These files _are_ the source of truth — the GitHub UI is a projection.
When you want to add an issue, edit the spec and re-run
`make bootstrap`. When the spec drops an issue, run `pm prune` (once
that's implemented; until then close it manually in GitHub).
