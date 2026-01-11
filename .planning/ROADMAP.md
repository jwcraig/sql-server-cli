# Roadmap: Stored Procedure Parameter Inspection

## Overview

Enhance sscli's `describe` command to display complete parameter metadata for stored procedures and functions. Starting with discovery of existing patterns, through database query implementation, data modeling, multi-format output integration, and comprehensive testing.

## Domain Expertise

None

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

- [ ] **Phase 1: Discovery** - Analyze current describe command patterns and catalog views
- [ ] **Phase 2: Database Query** - Add sys.parameters catalog query to db layer
- [ ] **Phase 3: Data Model** - Create parameter metadata structs
- [ ] **Phase 4: Procedure Integration** - Wire parameters into procedure describe
- [ ] **Phase 5: Function Integration** - Extend to scalar/table-valued functions
- [ ] **Phase 6: Table Output** - Parameters section in table format
- [ ] **Phase 7: JSON Output** - Structured parameter data in JSON
- [ ] **Phase 8: Markdown Output** - Parameters in markdown format
- [ ] **Phase 9: Edge Cases** - Defaults, NULLability, user-defined types
- [ ] **Phase 10: Testing** - Integration test coverage

## Phase Details

### Phase 1: Discovery
**Goal**: Understand current describe.rs patterns, output flow, and SQL Server catalog views
**Depends on**: Nothing (first phase)
**Research**: Likely (existing code patterns, catalog schemas)
**Research topics**: describe.rs structure, object type detection, sys.parameters schema
**Plans**: TBD

Plans:
- [ ] 01-01: Analyze describe command flow and output patterns

### Phase 2: Database Query
**Goal**: Implement parameter metadata query using sys.parameters + sys.types
**Depends on**: Phase 1
**Research**: Likely (SQL Server catalog specifics)
**Research topics**: sys.parameters columns, sys.types join, default value extraction
**Plans**: TBD

Plans:
- [ ] 02-01: Add parameter query to db layer

### Phase 3: Data Model
**Goal**: Define Rust structs for parameter metadata
**Depends on**: Phase 2
**Research**: Unlikely (follows existing patterns)
**Plans**: TBD

Plans:
- [ ] 03-01: Create ParameterInfo struct and related types

### Phase 4: Procedure Integration
**Goal**: Integrate parameter display into stored procedure describe output
**Depends on**: Phase 3
**Research**: Unlikely (internal integration)
**Plans**: TBD

Plans:
- [ ] 04-01: Wire parameters into procedure describe flow

### Phase 5: Function Integration
**Goal**: Extend parameter support to scalar and table-valued functions
**Depends on**: Phase 4
**Research**: Unlikely (same pattern as procedures)
**Plans**: TBD

Plans:
- [ ] 05-01: Add function parameter support

### Phase 6: Table Output
**Goal**: Format parameters as table section in terminal output
**Depends on**: Phase 5
**Research**: Unlikely (existing table patterns)
**Plans**: TBD

Plans:
- [ ] 06-01: Add parameters section to table formatter

### Phase 7: JSON Output
**Goal**: Include structured parameter data in JSON output
**Depends on**: Phase 5
**Research**: Unlikely (existing JSON patterns)
**Plans**: TBD

Plans:
- [ ] 07-01: Add parameters to JSON output structure

### Phase 8: Markdown Output
**Goal**: Format parameters in markdown output
**Depends on**: Phase 5
**Research**: Unlikely (existing markdown patterns)
**Plans**: TBD

Plans:
- [ ] 08-01: Add parameters to markdown formatter

### Phase 9: Edge Cases
**Goal**: Handle defaults, NULLability, user-defined types, special scenarios
**Depends on**: Phases 6-8
**Research**: Unlikely (refinement of existing work)
**Plans**: TBD

Plans:
- [ ] 09-01: Handle parameter edge cases

### Phase 10: Testing
**Goal**: Comprehensive integration tests for parameter inspection
**Depends on**: Phase 9
**Research**: Unlikely (follows existing test patterns)
**Plans**: TBD

Plans:
- [ ] 10-01: Add integration tests for parameter output

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9 → 10

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Discovery | 0/1 | Not started | - |
| 2. Database Query | 0/1 | Not started | - |
| 3. Data Model | 0/1 | Not started | - |
| 4. Procedure Integration | 0/1 | Not started | - |
| 5. Function Integration | 0/1 | Not started | - |
| 6. Table Output | 0/1 | Not started | - |
| 7. JSON Output | 0/1 | Not started | - |
| 8. Markdown Output | 0/1 | Not started | - |
| 9. Edge Cases | 0/1 | Not started | - |
| 10. Testing | 0/1 | Not started | - |
