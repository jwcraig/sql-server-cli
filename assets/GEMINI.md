# SQL Server Extension (Gemini CLI)

Use this extension when you need to connect to a SQL Server host/database to:

- Confirm connectivity / basic server status
- Browse schemas/tables/columns/relationships
- Describe a table (columns + types + indexes + constraints + triggers)
- Run ad-hoc SQL and capture raw row evidence

## Commands

These commands run the local `sscli` CLI and inject output into the prompt.

- `/sscli:status`
- `/sscli:tables [--schema dbo] [--like %foo%]`
- `/sscli:describe <Object> [--schema dbo]`
- `/sscli:sql <SQL>`

## Notes

- The underlying CLI is expected to be on `PATH` as `sscli`.
- Prefer `--json` when you need structured data.
- Prefer `--format tsv --no-headers --raw` for shell comparisons and evidence files.
- Prefer `--password-env NAME` or `SSCLI_URL` over `--password value`.
- Use `--quiet-tls-warning` when `--trust-cert true` is intentional and verbose logs would otherwise add TLS noise.
- Use `sscli admin ... --dry-run` for SQL Server maintenance planning before execution.
- `sscli admin sql` and `sscli sql` always print an `Execution` summary, so a statement with no result sets is still visibly distinct from one that never ran.
- A summary of `Status ok` proves execution, not effect. For destructive SQL add `--verify "<read-back query>" --expect-rows N`; a mismatch exits non-zero.
- A failed batch is not a rollback: statements before the failing one in that batch have committed. Read the state back instead of reporting "nothing happened".
- Multi-batch `GO` scripts (`sscli sql --file`) run like `sqlcmd -b`: one session, transactions and `SET` options carry across batches, and the first failing batch stops the script and rolls back any open transaction.
- Name the target with `--profile <name>`; an unknown profile is an error, and with no config sscli warns that it is using `localhost:1433/master`.
- Output is truncated by default (cells >140 chars, total >25KB). Use `--no-truncate` for full output.
