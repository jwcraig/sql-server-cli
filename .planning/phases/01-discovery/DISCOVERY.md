# Discovery: Stored Procedure Parameter Inspection

**Date:** 2026-01-11
**Phase:** 01-discovery
**Purpose:** Document current implementation patterns and SQL Server catalog availability for parameter metadata enhancement.

## Current Implementation Analysis

### Procedure Parameter Handling

**Location:** `src/commands/describe.rs:603-687` (`describe_procedure` function)

**Current Query (lines 614-628):**
```sql
SELECT
    p.name AS param_name,
    TYPE_NAME(p.user_type_id) AS data_type,
    p.max_length,
    p.precision,
    p.scale,
    p.is_output
FROM sys.parameters p
INNER JOIN sys.objects o ON p.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type = 'P'
ORDER BY p.parameter_id
```

**Key Finding:** The query already fetches `precision` and `scale` but these are NOT displayed in the output.

**JSON Output (lines 644-665):**
```rust
json!({
    "name": value_to_string(row.first()),
    "dataType": value_to_string(row.get(1)),
    "maxLength": row.get(2).and_then(...),
    "isOutput": value_to_bool(row.get(5)),
})
```
Missing from JSON: precision (row index 3), scale (row index 4)

**Text Output (lines 666-684):**
Uses `format_params_result_set()` helper at line 1511 which creates a ResultSet with columns:
- name, dataType, maxLength, isOutput

Missing from display: precision, scale (even though queried!)

### Function Parameter Handling

**Location:** `src/commands/describe.rs:689-802` (`describe_function` function)

**Current Query (lines 724-737):**
```sql
SELECT
    p.name AS param_name,
    TYPE_NAME(p.user_type_id) AS data_type,
    p.max_length,
    p.is_output
FROM sys.parameters p
...
AND p.parameter_id > 0  -- Excludes return value (parameter_id = 0)
ORDER BY p.parameter_id
```

**Key Finding:** Function query is even more limited - missing precision and scale entirely.

**JSON Output (lines 753-778):**
Only shows: name, dataType

**Text Output (lines 779-800):**
Uses `format_fn_params_result_set()` at line 1554 which only shows:
- name, dataType

### Output Helper Functions

| Function | Location | Columns Displayed |
|----------|----------|-------------------|
| `format_params_result_set()` | Line 1511-1552 | name, dataType, maxLength, isOutput |
| `format_fn_params_result_set()` | Line 1554-1578 | name, dataType |

**Pattern Notes:**
- Both functions transform raw query ResultSet into display ResultSet
- Row indices hardcoded (fragile if query column order changes)
- `is_output` boolean converted to "yes"/"no" string for display

### Data Structures

**No ParameterInfo struct exists.** Parameter data flows through:
1. Query result → `ResultSet` with rows of `Vec<Value>`
2. Transform function → Display `ResultSet`
3. Render via `table::render_result_set_table()`

The inline approach works but makes enhancement harder. A dedicated `ParameterInfo` struct would improve:
- Type safety
- Code clarity
- Easier addition of new fields

---

## SQL Server Catalog Analysis

### sys.parameters Columns

| Column | Type | Description | Currently Used |
|--------|------|-------------|----------------|
| object_id | int | Parent object ID | Yes (join) |
| name | sysname | Parameter name (includes @) | Yes |
| parameter_id | int | Ordinal position | Yes (ORDER BY) |
| user_type_id | int | User-defined type ID | Yes (TYPE_NAME) |
| system_type_id | int | System type ID | No |
| max_length | smallint | Max length in bytes | Yes |
| precision | tinyint | Precision for numeric types | Yes (not displayed!) |
| scale | tinyint | Scale for numeric types | Yes (not displayed!) |
| is_output | bit | 1 = OUTPUT parameter | Yes |
| **has_default_value** | bit | 1 = Has default | **No - should add** |
| **is_nullable** | bit | 1 = Nullable | **No - should add** |
| is_readonly | bit | 1 = READONLY (table-valued) | No (edge case) |
| default_value | sql_variant | NULL always | N/A (never populated) |

### Default Value Challenge

**Important:** SQL Server's `sys.parameters.has_default_value` only indicates whether a default exists - it does NOT contain the actual default value. The `default_value` column is always NULL.

**Options for extracting actual defaults:**
1. Parse from `OBJECT_DEFINITION()` - complex regex parsing
2. Use `sp_describe_first_result_set` - only for result sets
3. Show boolean flag only (recommended for simplicity)

**Recommendation:** Display `has_default_value` as boolean. Extracting actual values is:
- Complex (requires parsing procedure definition)
- Fragile (syntax variations, comments, etc.)
- Low value (users can see defaults in DDL output)

### Type Resolution

Current approach uses `TYPE_NAME(p.user_type_id)` which correctly resolves:
- Built-in types (int, varchar, etc.)
- User-defined types (aliases)
- Table types

### Parameter Direction Mapping

SQL Server only has two directions:
| is_output | Direction | SQL Syntax |
|-----------|-----------|------------|
| 0 | IN | Default (no keyword) |
| 1 | OUTPUT | `OUTPUT` keyword |

Note: SQL Server does NOT have INOUT - OUTPUT parameters can be passed values but this is declared with OUTPUT only.

**Display Recommendation:** Show direction as:
- "IN" for is_output=0
- "OUTPUT" for is_output=1

This matches SQL Server's actual syntax and documentation.

---

## Recommended Query Structure

### Enhanced Procedure Parameter Query

```sql
SELECT
    p.name AS param_name,
    TYPE_NAME(p.user_type_id) AS data_type,
    p.max_length,
    p.precision,
    p.scale,
    p.is_output,
    p.has_default_value,
    p.is_nullable
FROM sys.parameters p
INNER JOIN sys.objects o ON p.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type = 'P'
ORDER BY p.parameter_id
```

### Enhanced Function Parameter Query

```sql
SELECT
    p.name AS param_name,
    TYPE_NAME(p.user_type_id) AS data_type,
    p.max_length,
    p.precision,
    p.scale,
    p.is_output,
    p.has_default_value,
    p.is_nullable
FROM sys.parameters p
INNER JOIN sys.objects o ON p.object_id = o.object_id
INNER JOIN sys.schemas s ON o.schema_id = s.schema_id
WHERE o.name = @P1
  AND (@P2 IS NULL OR s.name = @P2)
  AND o.type IN ('FN', 'IF', 'TF', 'AF')
  AND p.parameter_id > 0
ORDER BY p.parameter_id
```

---

## Key Decisions

### Decision 1: Direction Terminology
**Choice:** Use "IN" and "OUTPUT" (not IN/OUT/INOUT)
**Rationale:** Matches SQL Server syntax and documentation. Users write `@param OUTPUT`, not `@param OUT`.

### Decision 2: Default Value Display
**Choice:** Show boolean flag ("Has Default: yes/no")
**Rationale:**
- Actual value extraction requires parsing OBJECT_DEFINITION
- Complex, fragile, and low value-add
- DDL output already shows full definition including defaults

### Decision 3: Type Formatting
**Choice:** Format type with length/precision/scale inline
**Examples:**
- `varchar(50)` not `varchar` + `maxLength: 50`
- `decimal(10,2)` not `decimal` + `precision: 10, scale: 2`
- `int` (no suffix for fixed-size types)

**Rationale:** Matches how users would write the type in SQL. More readable. Uses existing `format_type_spec()` helper at line 1186.

### Decision 4: Data Structure Approach
**Choice:** Create ParameterInfo struct (Phase 3)
**Rationale:**
- Current inline approach with row indices is fragile
- Struct provides type safety and clear field names
- Makes future enhancements easier

---

## Integration Points Summary

| Component | Location | Changes Needed |
|-----------|----------|----------------|
| Procedure query | Line 614-628 | Add has_default_value, is_nullable |
| Function query | Line 724-737 | Add precision, scale, has_default_value, is_nullable |
| Procedure JSON output | Line 644-665 | Add missing fields |
| Function JSON output | Line 753-778 | Add all enhanced fields |
| `format_params_result_set()` | Line 1511-1552 | Add direction, formatted type, default, nullable |
| `format_fn_params_result_set()` | Line 1554-1578 | Add same enhanced fields |

---

## Next Steps

1. **Phase 2:** Update database queries to include has_default_value, is_nullable
2. **Phase 3:** Create ParameterInfo struct for type-safe handling
3. **Phase 4-5:** Wire enhanced data through procedure/function flows
4. **Phase 6-8:** Update output formatters (table, JSON, markdown)
5. **Phase 9:** Handle edge cases (user-defined types, table-valued params)
6. **Phase 10:** Add integration tests

---

*Discovery complete: 2026-01-11*
