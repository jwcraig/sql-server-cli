# SSCLI-COMPARE(1)

## NAME

sscli compare - detect schema drift between two SQL Server connections

## SYNOPSIS

`sscli compare --target <profile> [--source <profile>] [--schema <name> ...] [options]`

## DESCRIPTION

`compare` pulls metadata from two SQL Server connections (profiles or explicit
connection strings), normalizes definitions, and reports drift. It can emit a
summary, a unified diff for a single object, or a SQL apply script to align
the target with the source.

## OPTIONS

- `--target`, `--right` **PROFILE**  
  Required. Profile name from `.sql-server/config.*` to treat as the environment you want to align (e.g., `prod`).

- `--source`, `--left` **PROFILE**  
  Reference profile (e.g., `dev`, `stage`). Defaults to the global `--profile` or config default.

- `--source-connection`, `--left-connection` **CONN**  
  Override the source profile with a connection string (URL or ADO style).

- `--target-connection`, `--right-connection` **CONN**  
  Override the target profile with a connection string.

- `--schema`, `--schemas` **NAME[,NAME...]**  
  Limit drift detection to specific schemas (repeat or comma-separated).

- `--object` **schema.name|name**  
  Focus on a single module (proc/view/function/trigger) and emit a unified diff.

- `--summary`  
  Output a compact JSON or pretty/markdown summary instead of full snapshots.

- `--pretty`  
  Pretty text summary (with `--summary`). Honors global output format.

- `--ignore-whitespace`  
  Collapse whitespace before comparing definitions.

- `--strip-comments`  
  Remove `/* ... */` and `-- ...` comments before comparing.

- `--apply-script` `[path|-]`  
  Generate SQL to align target to source. Default path:
  `db-apply-diff-YYYYMMDD-HHMMSS.sql` in the current directory. Use `-` for stdout.

- `--include-drops`  
  Include DROP statements for objects missing from the source.

## EXIT STATUS

0 on no drift, 3 when drift is detected (summary/object/apply modes), 1 on error.

## EXAMPLES

Compare default profile to prod and emit summary:  
`sscli compare --target prod --summary --pretty`

Diff a single procedure, ignoring whitespace:  
`sscli compare --target prod --object dbo.GetUsers --ignore-whitespace`

Generate apply script to stdout:  
`sscli compare --target prod --apply-script - --include-drops`
