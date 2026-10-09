# Agent-First SQL Server Operations Plan

## Purpose

Make `sscli` the fastest, lowest-friction SQL Server tool for coding agents and
operators who need trustworthy database inspection, evidence capture, and
planned maintenance workflows.

The core product principle is agent usefulness over restrictive guardrails.
`sscli` should not surprise agents with hidden parser restrictions or make them
fall back to `sqlcmd` for normal operational work. The safety model should come
from clear command names, explicit targets, dry runs, deterministic output,
secret-safe connection handling, and purpose-built workflows for high-risk
operations.

## Background

During a PlantLogiq UAT merge/restore workflow, `sscli status`, `describe`,
`sql`, markdown output, and explicit connection flags were useful for rapid SQL
Server inspection. The workflow still fell back to raw `docker exec ... sqlcmd`
for maintenance commands, raw evidence files, and restore validation.

The current repo direction already supports full SQL execution. The README
documents `sql` as the canonical raw-SQL surface and says the shipped tool is
intended to be full-capability. There is an older read-only validator in
`src/safety/read_only.rs`, but it is not wired into `sql`. This plan follows the
current product direction: do not re-lock `sql`; instead add agent-friendly
formats, connection ergonomics, and first-class maintenance workflows.

## Goals

- Keep `sql` frictionless for ad hoc SQL, scripts, file input, and stdin input.
- Emit machine-stable evidence formats directly from `sql`.
- Keep diagnostics on stderr and data on stdout for automation.
- Reduce command-line password exposure.
- Make ephemeral restore-container connections easy to reuse.
- Add first-class SQL Server backup, verify, filelist, checkdb, and restore-test
  workflows.
- Add health and count helpers for merge/restore validation.
- Preserve markdown as the default human/agent reading format.
- Avoid hard-coded app-specific helpers while making app-specific comparisons
  easy to build from exact raw query output.

## Non-Goals

- Do not turn `sscli` into an interactive TUI.
- Do not make a locked-down or read-only distribution in this plan.
- Do not hide high-risk operations behind vague commands.
- Do not implement PlantLogiq-specific object-key logic in the generic tool.
- Do not replace SQL Server backup/restore semantics with a custom abstraction
  that obscures what SQL Server will execute.

## Command Surface

### Raw SQL Output Modes

Add raw, stdout-first output modes to `sql`:

```bash
sscli sql --format tsv --no-headers --raw "SELECT ..."
sscli sql --format csv --raw --output counts.csv "SELECT ..."
sscli sql --format jsonl "SELECT ..."
sscli sql --format tsv --raw --output candidate-attachment-keys.tsv "SELECT ..."
```

Recommended flags:

- `--format pretty|markdown|json|tsv|csv|jsonl`
- `--output <path|->`
- `--no-headers`
- `--raw`
- `--null <value>`
- `--result-set <n>`
- `--all-result-sets`

Make `--format` global from the start. Agents will naturally try both
`sscli --format tsv "SELECT ..."` and
`sscli --format jsonl sql --file query.sql`, so the top-level SQL shorthand
must recognize the new global/raw flags. `--json`, `--markdown`, and `--pretty`
remain aliases. Row formats initially need only work for `sql`; unsupported
commands should fail with a clear error instead of silently falling back.

Keep the existing `--csv <file>` path for compatibility, but treat
`--format csv --output <file>` as the future-facing model. This avoids turning
the current path-valued `--csv` option into an ambiguous flag.

Raw formats must avoid display formatting. For example, integer values should
not include thousands separators in TSV/CSV/JSONL output. Markdown and pretty
tables can remain optimized for reading.

Raw output contract:

- stdout is data only: no target banner, result-set labels, truncation notices,
  progress, warnings, or summary text.
- stderr carries target banners, progress, warnings, and errors.
- `--raw` implies no truncation and no display formatting.
- `--no-headers` applies only to TSV and CSV.
- `--null` defaults to an empty string for TSV/CSV and JSON `null` for
  JSON/JSONL.
- JSONL emits one row object per line for the selected result set, not wrapper
  metadata.
- If exactly one tabular result set exists, raw mode emits it by default.
- If multiple tabular result sets exist and neither `--result-set` nor
  `--all-result-sets` is supplied, fail with a clear message.
- If `--output <path>` is used with multiple result sets, require `{n}` in the
  path or fail. Do not silently suffix filenames in raw evidence mode.

### Connection Ergonomics

Before adding new secret input mechanisms, redact secrets everywhere by default.
Resolved config/debug output should not emit plaintext passwords.

Recommended config/debug contract:

- `connection.password: null`
- `connection.passwordSet: true|false`
- `connection.passwordSource: cli|env|profile|url|stdin|none`
- `connection.passwordEnv: "PLQ_SQL_PASSWORD"` when known

Avoid a casual `--show-secrets` path. If it becomes necessary for debugging, it
must be explicit and excluded from agent-facing guidance.

Add global secret-safe password sources:

```bash
sscli --password-env PLQ_SQL_PASSWORD sql "SELECT 1"
printf '%s' "$PLQ_SQL_PASSWORD" | sscli --password-stdin sql "SELECT 1"
```

Add `SSCLI_URL` as a first-class environment variable:

```bash
export SSCLI_URL='sqlserver://sa:pass@127.0.0.1:15433/WDM?trustServerCertificate=true'
sscli sql "SELECT COUNT_BIG(*) FROM dbo.T_MP"
```

URL query params should support:

- `trustServerCertificate=true|false`
- `encrypt=true|false`
- `timeoutMs=<number>`

Longer term, add named ephemeral targets:

```bash
sscli target save local-merge \
  --server 127.0.0.1 \
  --port 15433 \
  --database WDM_UAT_MERGE_CANDIDATE \
  --user sa \
  --password-env PLQ_SQL_PASSWORD \
  --trust-cert

sscli --target local-merge sql "SELECT 1"
```

Do URL/env support before `target save`; it is smaller, script-friendly, and
likely solves most restore-container friction.

### Maintenance Lane

Add a deliberately named `admin` lane for high-risk SQL Server operations:

```bash
sscli admin sql "DBCC CHECKDB (...) WITH PHYSICAL_ONLY, TABLOCK, NO_INFOMSGS"
sscli admin sql --dry-run "DBCC CHECKDB (...) WITH PHYSICAL_ONLY, TABLOCK, NO_INFOMSGS"
```

The escape hatch should exist, but typed commands should be preferred:

```bash
sscli admin backup create \
  --database WDM \
  --to /var/opt/mssql/backup/WDM.bak \
  --copy-only \
  --compression \
  --checksum \
  --stats 5

sscli admin backup verify --from /var/opt/mssql/backup/WDM.bak --checksum
sscli admin backup filelist --from /var/opt/mssql/backup/WDM.bak --format json

sscli admin backup restore-plan \
  --from /var/opt/mssql/backup/WDM.bak \
  --database WDM_VERIFY_20260628 \
  --data-dir /var/opt/mssql/data \
  --format json

sscli admin checkdb --database WDM --physical-only --docker-volume-safe
```

Use `admin backup ...` rather than changing the existing `backups` command
first. Today `backups` is a read-only backup-history command, so `admin backup`
avoids a breaking rename while giving maintenance work a clear home.

`restore-plan` is a non-executing primitive. It should emit logical names,
proposed physical paths, generated `RESTORE DATABASE ... WITH MOVE ...` SQL,
detected existing database conflicts, and the confirmation required for an
executing restore. It gives agents a durable restore artifact before any
database is modified.

All backup and restore path help text must state that paths are resolved by SQL
Server, usually inside the SQL Server host or container. A macOS agent path and
a SQL Server container path are not the same thing.

### Restore-Test Workflow

Add a composed restore verification workflow after the primitives and
`restore-plan` exist:

```bash
sscli admin backup restore-test \
  --from /var/opt/mssql/backup/WDM_UAT_MERGED.bak \
  --database WDM_UAT_MERGED_BACKUP_VERIFY_20260628 \
  --data-dir /var/opt/mssql/data \
  --drop-after \
  --compare-counts dbo.T_MP,dbo.data_set,dbo.measurement,dbo.note,dbo.attachment
```

Workflow steps:

1. `RESTORE FILELISTONLY` and derive logical file names.
2. Build `RESTORE DATABASE ... WITH MOVE ...`.
3. Run restore with optional progress stats.
4. Run `RESTORE VERIFYONLY WITH CHECKSUM` when requested.
5. Run optional `DBCC CHECKDB` with `TABLOCK` support.
6. Run optional count comparison.
7. Drop the verification database only when `--drop-after` is explicit.

### Count and Health Helpers

Add merge/restore validation commands:

```bash
sscli compare-counts \
  --source-db WDM_UAT_RECOVERY \
  --target-db WDM_UAT_MERGE_CANDIDATE \
  --tables dbo.T_MP,dbo.data_set,dbo.measurement,dbo.note,dbo.attachment \
  --format tsv

sscli compare-counts \
  --source-connection "$SOURCE_URL" \
  --target-connection "$TARGET_URL" \
  --tables-file core-tables.txt \
  --format json
```

Start with same-server database comparisons, then add cross-connection support.
Use `COUNT_BIG(*)`, stable table ordering, and exit code `3` for mismatches.

Add health summaries:

```bash
sscli health constraints --tables dbo.T_MP,dbo.measurement,dbo.attachment
sscli health triggers --tables dbo.T_MP,dbo.measurement,dbo.attachment
sscli health identities --tables dbo.T_MP,dbo.data_set,dbo.measurement
```

Stable columns:

- constraints: `table`, `object`, `kind`, `enabled`, `trusted`,
  `deleteAction`, `updateAction`, `status`
- triggers: `table`, `object`, `kind`, `enabled`, `isInsteadOfTrigger`,
  `status`
- identities: `table`, `column`, `seed`, `increment`, `identityCurrent`,
  `maxId`, `delta`, `status`

Filters:

- `--all-user-tables`
- `--schema <name>`
- `--tables <schema.table,...>`
- `--tables-file <path>`

## Safety Model

The tool should be safe because it is explicit and observable, not because it
blocks useful work.

Principles:

- Show the resolved target on stderr unless explicitly quieted.
- Keep stdout clean for data formats.
- Redact secrets in config output, logs, target files, and audit summaries by
  default.
- Require exact database names for destructive restore/drop actions.
- Prefer `--dry-run` for every generated maintenance command.
- Print generated SQL for maintenance commands in dry-run mode.
- Make `admin` commands auditable with JSON summaries.
- Use typed flags for common dangerous operations instead of asking agents to
  hand-write complex SQL.
- Keep raw `admin sql` available for cases typed commands do not cover.

Confirmation policy:

- `admin sql` is full-capability and does not use parser restrictions.
- `restore-test --drop-after` requires the target database to be created by the
  same invocation or `--confirm <database-name>`.
- Any command that overwrites or drops an existing database requires
  `--replace-existing --confirm <database-name>`.
- Only require confirmation for raw `admin sql` when sscli can reliably detect a
  destructive overwrite/drop pattern. If detection is not reliable, do not
  pretend it is; keep confirmations on typed commands where sscli knows intent.

This is intentionally lighter than a strict permission system. It should slow
down accidental destructive actions without slowing down planned agent work.

Exit code policy:

- `0`: success or validation passed
- `1`: execution or connection error
- `2`: invalid usage or configuration
- `3`: validation mismatch, drift, or unhealthy state
- `4`: operation completed with warnings

## Diagnostics and Logging

For raw evidence modes:

- stdout contains only requested data.
- stderr contains target banners, warnings, progress, and errors.
- `--quiet-target` suppresses target banners.
- `--quiet` suppresses non-error output.

TLS warning behavior:

- When `--trust-cert true` is explicitly supplied, suppress the noisy tiberius
  trust warning by default.
- Keep the warning for implicit config/default trust unless the user opts into
  quiet behavior.
- Respect explicit verbose logging and `RUST_LOG` where possible.

## Implementation Sequence

### Phase 0: Secret Redaction and Parser Contract

Files likely touched:

- `src/output/json.rs`
- `src/config/loader.rs`
- `src/cli/args.rs`
- `tests/config_command_test.rs`
- `tests/sql_command_test.rs`

Tasks:

1. Redact resolved passwords from config/debug JSON by default.
2. Emit `passwordSet`, `passwordSource`, and `passwordEnv` metadata where known.
3. Add parser tests for top-level SQL shorthand with the planned global/raw
   flags.
4. Preserve existing `--json`, `--markdown`, and `--pretty` behavior while
   preparing for global `--format`.

### Phase 1: Raw Evidence Output

Files likely touched:

- `src/cli/args.rs`
- `src/commands/sql.rs`
- `src/output/`
- `tests/sql_command_test.rs`

Tasks:

1. Add global `--format` with SQL support for TSV, CSV, and JSONL.
2. Add `--output`, `--no-headers`, `--raw`, `--null`, `--result-set`, and
   `--all-result-sets`.
3. Add TSV, CSV-to-stdout/file, and JSONL renderers.
4. Preserve existing `--json`, `--markdown`, `--pretty`, and `--csv <file>`.
5. Ensure raw stdout is data-only.
6. Fail clearly on unsupported command/format combinations.
7. Add tests for nulls, headers, result-set selection, multi-result behavior,
   output paths with `{n}`, top-level shorthand, and stderr separation.

### Phase 2: Connection, TLS, and Secret Inputs

Files likely touched:

- `src/cli/args.rs`
- `src/config/loader.rs`
- `src/config/env.rs`
- `src/main.rs`
- `src/db/connection.rs`
- `tests/config_command_test.rs`
- `tests/sql_command_test.rs`

Tasks:

1. Add global `--password-env`.
2. Add global `--password-stdin` if it can be made unambiguous with
   `sql --stdin`; otherwise defer it.
3. Add `SSCLI_URL`.
4. Parse URL query params for trust cert, encryption, and timeout.
5. Detect explicit `--trust-cert true`.
6. Suppress the known tiberius trust warning for that explicit case.
7. Preserve warnings for implicit/default trust when not in raw quiet modes.
8. Add regression coverage for clean raw stdout/stderr.

### Phase 3: Count Comparison

Files likely touched:

- `src/cli/args.rs`
- `src/commands/compare_counts.rs`
- `src/commands/mod.rs`
- `src/db/`
- `tests/p2_commands_test.rs`

Tasks:

1. Add same-server `--source-db` and `--target-db`.
2. Add `--tables` and `--tables-file`.
3. Emit table, source count, target count, delta, and status.
4. Use exit code `3` for mismatches.
5. Add cross-connection support after same-server behavior is stable.

### Phase 4: Admin SQL, Restore Planning, Verify, and CheckDB

Files likely touched:

- `src/cli/args.rs`
- `src/commands/admin.rs`
- `src/commands/mod.rs`
- `tests/integration_commands_test.rs`

Tasks:

1. Add `admin sql` with dry-run and target visibility.
2. Add `admin backup filelist`.
3. Add `admin backup verify`.
4. Add `admin backup restore-plan`.
5. Add `admin checkdb --physical-only --docker-volume-safe`.
6. Document SQL Server host/container path semantics in help and examples.
7. Add dry-run output for every generated SQL command.

### Phase 5: Backup Create and Restore-Test Composition

Files likely touched:

- `src/commands/admin.rs`
- `src/commands/compare_counts.rs`
- integration tests that can run against a disposable SQL Server container

Tasks:

1. Add `admin backup create`.
2. Compose filelist, restore, verify, checkdb, compare-counts, and optional
   cleanup.
3. Require explicit confirmation for overwrite/drop actions.
4. Emit a JSON audit summary with redacted secrets, paths, database names,
   actions, timings, and validation results.
5. Add documentation with Docker/macOS sparse-snapshot notes and TABLOCK
   behavior.

### Phase 6: Health Helpers

Files likely touched:

- `src/cli/args.rs`
- `src/commands/health.rs`
- `src/commands/mod.rs`
- `tests/p2_commands_test.rs`

Tasks:

1. Add `health constraints`.
2. Add `health triggers`.
3. Add `health identities`.
4. Support `--all-user-tables`, `--schema`, `--tables`, and `--tables-file`.
5. Emit markdown, JSON, TSV, and CSV via the shared output model.

### Phase 7: Optional Ephemeral Targets

Files likely touched:

- `src/cli/args.rs`
- `src/config/`
- `src/commands/target.rs`
- `README.md`

Tasks:

1. Add `target save`, `target list`, `target remove`, and `--target`.
2. Store password references as env var names, not raw secrets.
3. Prefer project-local target files for restore workflows.
4. Keep normal profiles as the stable long-lived connection mechanism.
5. Only build this phase if `SSCLI_URL`, `--env-file`, and `--password-env`
   still leave meaningful restore-container friction.

## Verification Strategy

For every phase:

- Run `cargo fmt --check`.
- Run `cargo clippy -- -D warnings`.
- Run `cargo test`.
- Add command parser tests for every new CLI flag.
- Add output contract tests for stdout/stderr separation.

For DB-backed commands:

- Add smoke tests gated by SQL Server env vars.
- Use disposable databases for destructive paths.
- Test dry-run without requiring SQL Server where possible.
- For restore-test, verify against a small generated backup fixture or a
  documented local container recipe.

Before release:

- Update `README.md`.
- Update bundled agent skill assets.
- Run `cargo build --release`.
- Run `cargo install --path . --force`.

For the first milestone, update `README.md` and `assets/SKILL.md.template`
before release, not as an afterthought. `sscli` is positioned as an agent tool;
if the bundled skill does not teach raw output and password-safe connection
patterns, agents will keep using old habits.

## Open Questions

1. Should raw output imply `--quiet-target`, or should it keep target banners on
   stderr by default? Current recommendation: keep banners on stderr by default.
2. Should `admin sql` record local audit logs, or is structured stdout/stderr
   enough?
3. Should restore-test support cross-container host paths explicitly, or simply
   document that backup/data paths are SQL Server host/container paths?
4. Should `backups` eventually be renamed to `backup history`, or should it stay
   as an alias forever?
5. Is `--password-stdin` worth the ambiguity with `sql --stdin`, or should
   `--password-env` and `SSCLI_URL` be the only first-milestone secret inputs?

## Recommended First Milestone

Ship a small but high-impact release with:

1. Secret redaction everywhere by default.
2. Global `--format` with `sql` support for TSV, CSV, and JSONL.
3. Clean raw stdout contract, including multi-result failure behavior.
4. `--password-env`.
5. `SSCLI_URL` with trust/encrypt/timeout query params.
6. Explicit TLS warning cleanup for `--trust-cert true`.
7. Parser tests for top-level SQL shorthand with the new flags.
8. `README.md` and bundled skill updates.

This removes the daily evidence-capture and connection friction without
touching destructive operations. After that, add `compare-counts`,
`restore-plan`, admin backup filelist/verify, and admin checkdb in the next
release.

## Nash Review Notes

Nash verdict: `adopt-staged`, with revisions before implementation.

Accepted changes from the review:

- Treat secret leakage as a Phase 0 blocker.
- Make `--format` global and ensure top-level SQL shorthand understands the new
  flags.
- Tighten raw output semantics before coding.
- Move `compare-counts` before restore workflows.
- Avoid ceremonial `admin sql --confirm db-maintenance`; keep confirmations on
  typed destructive operations where intent is known.
- Add `restore-plan` before executing restore-test workflows.
- Document SQL Server host/container path semantics directly in admin help.
- Define per-command health schemas.
- Add validation-oriented exit codes.
- Update bundled agent skill assets in the first milestone.

Main risk assessment from Nash:

- Overstated risk: full-capability `admin sql`; `sql` is already full-capability
  and the product rejects hidden read-only restrictions.
- Understated risk: plaintext secret leakage and impure raw stdout.
