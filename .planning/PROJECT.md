# Stored Procedure Parameter Inspection

## What This Is

Enhancement to sscli's `describe` command that adds complete parameter metadata for stored procedures. Currently `describe` shows procedure definitions but not parameter types, defaults, or directions — forcing users to run catalog queries manually.

## Core Value

**Complete parameter visibility.** When inspecting a stored procedure, users should see everything they need to call it correctly: parameter names, types, lengths, defaults, and directions.

## Requirements

### Validated

- ✓ `describe` command displays procedure definitions — existing
- ✓ Multi-format output support (table, markdown, JSON, CSV) — existing
- ✓ Object type detection (tables, views, procedures, functions) — existing
- ✓ Schema-qualified object naming — existing

### Active

- [x] Display stored procedure parameters as separate section with complete metadata
- [x] Include parameter direction (IN/OUT/INOUT)
- [x] Include data type with length/precision/scale
- [x] Include default values where defined
- [x] Include nullability information
- [x] Support function parameters (scalar and table-valued)
- [x] Consistent output across all formats (table, JSON, markdown)

### Out of Scope

(None — full parameter metadata is in scope)

## Context

- sscli is a Rust CLI for SQL Server with layered architecture
- `describe` command exists at `src/commands/describe.rs`
- Database layer uses Tiberius driver with async Tokio runtime
- INFORMATION_SCHEMA.PARAMETERS provides parameter metadata
- sys.parameters + sys.types gives richer type info including defaults
- Output layer already handles multi-format rendering

## Constraints

- **Tech stack**: Rust, Tiberius, existing architecture patterns
- **Compatibility**: Follow existing `describe` command patterns

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Separate "Parameters:" section | Clear separation from definition, easier to parse | Implemented |
| Query sys.parameters over INFORMATION_SCHEMA | Richer metadata including defaults | Implemented |
| Include functions alongside procedures | Consistent behavior, user expectation | Implemented |

---
*Last updated: 2026-01-11 after parameter metadata implementation*
