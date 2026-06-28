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
- Output is truncated by default (cells >140 chars, total >25KB). Use `--no-truncate` for full output.
