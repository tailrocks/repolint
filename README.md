# repolint

Repository structure map generator and gate for the Unified Repository Standard.
## Repository map

<!-- MAP:BEGIN -->
> Leaf tier · drift-checked in `mise run fleet:check` · details live in each component's README

| Path | What lives here |
|---|---|
| [`src/`](src/) | CLI, configuration, map generation, and report implementation |
| [`tests/`](tests/) | CLI and map behavior tests |

### Root files worth knowing

| Path | Purpose |
|---|---|
| [`repolint.toml`](repolint.toml) | Repository-local adoption and map sources |
<!-- MAP:END -->

## Usage

Run from a repository root:

```sh
repolint check
repolint check --format json
repolint map --write
```

`check` never writes files. `map --write` is the only repolint write side and
updates only the marked `## Repository map` block in `README.md`; prose outside
the block remains repository-owned.

Without `repolint.toml [repo]`, checks report an adoption warning and exit 0.
An adopted repository must cache `tier`, `kind`, `visibility`, and `research`
under `[repo]`. Workspace and Polyglot repositories require a map; Leaf maps
are optional and their map findings are warnings.

## Configuration

The single root configuration is `repolint.toml`:

```toml
[repo]
tier = "workspace"       # leaf | workspace | polyglot
kind = "app"              # app | iac | dist | ci-producer | out-of-scope
visibility = "private"    # public | private | internal
research = false

[map.dirs]
"crates" = "Rust workspace members"
"crates/example" = "Example component"

[map.files]
"compatibility.toml" = "Terminal compatibility matrix"

[generated]
[[generated.entry]]
name = "schema"
command = "mise run gen:schema"
outputs = ["packages/typescript/.generated/"]

[checks]
"map.gate" = "error"     # off | warn | error

[[ignore]]
check = "future.check"
paths = ["legacy/**"]
reason = "Tracked migration exception"
```

`[map.dirs]` is an override and gap-fill table. Descriptions otherwise come
from a component manifest (`Cargo.toml` or `package.json`) and then the first
non-heading, non-badge, non-comment prose sentence in its `README.md`.
Heading-only READMEs intentionally produce `MISSING` and a gate finding.
`[map.files]` is opt-in and lists root files only. Generated output paths are
excluded from discovery.

## M1 ownership boundary

M1 implements `map.gate`: map generation, exact depth-1 directory coverage,
component rows for `apps/`, `services/`, `crates/`, `packages/`, and `tools/`,
description resolution, marker/position validation, dead-link detection, and
source drift detection. It emits human or JSON reports.

The mechanical M1 checks are consumed from `alint` alongside repolint:

- `taxonomy.forbidden-zones`
- `containment.no-parent-escape`
- `containment.no-absolute-home`
- `ignore.present`
- `mise.root-toml`
- `mise.verbs`
- `docker.context-ignore`
- `docker.build-task-is-scan`
- `generated.drift`
- `docs.no-run-artifacts`

`zizmor` owns the GHA checks `ci.pinned-actions` and
`ci.permissions-block`. No M1 code duplicates either engine.

## Exit ladder

`repolint` uses stable application statuses:

- `0`: clean, or warnings without `--error-on-warn`
- `1`: error findings, or warnings with `--error-on-warn`
- `2`: invalid CLI usage, unreadable input, or invalid configuration

The adopted `alint` v0.15.x ladder is: `0` no errors, `1` one or more errors,
`2` configuration error, and `3` internal error. Warnings do not fail unless
`--fail-on-warning` is supplied. Preserve that status when chaining the tools.

## M1 ambiguities pinned

- Map markers are recognized by the `<!-- MAP:BEGIN` and `<!-- MAP:END`
  prefixes, but generated output uses the canonical exact markers
  `<!-- MAP:BEGIN -->` and `<!-- MAP:END -->`. A gate requires one balanced pair.
- `[map.dirs]` keys are relative paths. `trigger` is a single glob string in
  the deferred-item schema; deferred ratchet evaluation is outside M1.
- First-seen storage for un-adopted repositories belongs to the fleet audit's
  committed census state; repolint does not create or consume that artifact.

## Non-goals

Generator integration, `init`/scaffold, deferred ratchets, fleet audit,
Cargo/Swift dependency graphs, Docker catalog semantics, SARIF, and `--fix`
remain post-M1 work.
