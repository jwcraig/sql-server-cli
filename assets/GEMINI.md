# SQL Server Extension (Gemini CLI)

Use this extension when you need to connect to a SQL Server host/database to:

- Confirm connectivity / basic server status
- Browse schemas/tables/columns/relationships
- Describe a table (columns + types + indexes + constraints + triggers)
- Run safe ad-hoc queries (read-only by default)

## Commands

These commands run the local `sscli` CLI and inject output into the prompt.

- `/sscli:status`
- `/sscli:tables [--schema dbo] [--like %foo%]`
- `/sscli:describe <Object> [--schema dbo]`
- `/sscli:sql <SQL>`

## Notes

- The underlying CLI is expected to be on `PATH` as `sscli`.
- Prefer `--json` when you need structured data.
