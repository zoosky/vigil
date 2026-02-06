# Markdown Documentation Review & QA Report

**Date:** 2026-02-06
**Scope:** All `*.md` files in the vigil project (27 files reviewed)
**Purpose:** Functional review of documentation quality, consistency, and accuracy
**Resolution:** All actionable tasks below have been addressed in follow-up commits.

---

## Executive Summary

The vigil project has extensive documentation across 27 markdown files spanning a root-level README, architecture docs, 16 feature specifications, a PRD review, test guide, and development context files. While the documentation is thorough in its initial creation, it has **fallen significantly out of sync with the actual implementation state**. The most critical issues are status mismatches between documents, outdated file structures, stale configuration examples, and incomplete cross-referencing.

---

## 1. Critical: Feature Status Mismatches

Multiple documents disagree on which features are implemented.

### 1.1 docs/README.md vs claude.md

| Feature | docs/README.md | claude.md | Feature Spec |
|---------|---------------|-----------|--------------|
| 005 CLI Reporting | **Pending** | Done | **Pending** |
| 006 Polish & Service | **Pending** | Done | **Pending** |
| 007-016 | **Not listed** | Mixed | Varies |

**Actionable tasks:**

- [ ] Update `docs/README.md` feature table to include all 16 features with correct statuses
- [ ] Update `docs/features/005-cli-reporting.md` status from "Pending" to "Done"
- [ ] Update `docs/features/006-polish-service.md` status from "Pending" to "Done"
- [ ] Ensure all three sources of truth (docs/README.md, claude.md, individual feature specs) agree

### 1.2 Unchecked Task Checkboxes in Completed Features

Features 001-006, 010, 013-016 are marked as "Done" but all their internal task checkboxes remain `[ ]` (unchecked). This creates confusion about what was actually implemented.

**Actionable tasks:**

- [ ] Convert `[ ]` to `[x]` for all completed tasks in features: 001, 002, 003, 004, 005, 006, 010, 013, 014, 015, 016
- [ ] For Feature 012 (CI Pipeline Fix), determine if it was implemented and update status accordingly
- [ ] For Feature 014, update the two optional tasks ("ICMP rate-limiting detection" and "migration notice") with explicit status (deferred/wontfix)

---

## 2. Critical: Outdated File Structure References

### 2.1 PLAN.md File Structure

`PLAN.md:299-321` shows a file structure missing several implemented files:

**Missing from PLAN.md file tree:**
- `src/monitor/tcp.rs` (Feature 014)
- `src/monitor/http.rs` (Feature 014)
- `src/cli/service.rs` (Feature 006)
- `src/cli/version.rs` (Feature 015)
- `src/cli/helpers.rs` (Feature 005)
- `src/cli/outage_detail.rs` (Feature 010)
- `build.rs` (Feature 015)
- `scripts/` directory

**Actionable tasks:**

- [ ] Update `PLAN.md` file structure to match actual codebase
- [ ] Update `docs/architecture.md:149-167` file structure to match actual codebase
- [ ] Update `docs/features/001-core-infrastructure.md:23-41` file tree to reflect final state

### 2.2 Architecture.md Outdated

`docs/architecture.md` is missing:
- TCP and HTTP monitoring components
- `degraded_events` database table (added in Feature 010)
- Unified connectivity dispatcher (`src/monitor/mod.rs`)
- Gateway-first diagnosis flow (Feature 013)

**Actionable tasks:**

- [ ] Add TCP/HTTP monitoring methods to architecture component diagram
- [ ] Add `degraded_events` table to database section
- [ ] Update data flow diagram to include gateway-first diagnosis
- [ ] Update component descriptions to reference all monitor types (ping, TCP, HTTP)

---

## 3. High: Configuration Format Inconsistencies

Different documents show different TOML configuration formats for the same settings.

### 3.1 Config File Location

| Document | Config Path |
|----------|------------|
| PLAN.md:209 | `~/.config/vigil/config.toml` |
| README.md:89 | `~/Library/Application Support/ch.kapptec.vigil/config.toml` |
| claude.md:62 | `~/Library/Application Support/ch.kapptec.vigil/config.toml` |

**Actionable task:**

- [ ] Update `PLAN.md:209` config path from `~/.config/vigil/config.toml` to `~/Library/Application Support/ch.kapptec.vigil/config.toml`

### 3.2 Target Configuration Syntax

| Document | Syntax |
|----------|--------|
| PLAN.md:219-224 | `[targets] targets = [{ name = "...", ip = "..." }]` |
| README.md:99-110 | `[[targets]] address = "..." name = "..."` |
| docs/usage.md:58-68 | `[targets] gateway = "..." [[targets.targets]]` |
| Feature 014 | `[targets] targets = [{ name, ip, method, port }]` |

**Actionable tasks:**

- [ ] Determine the canonical TOML config format from the actual `config.rs` implementation
- [ ] Update all markdown files to use the canonical config format consistently
- [ ] Specifically update: `PLAN.md`, `README.md`, `docs/usage.md`, `docs/features/001-core-infrastructure.md`

---

## 4. High: PLAN.md Staleness

`PLAN.md` is effectively frozen from the initial planning phase. All implementation phase checkboxes are unchecked and the document does not reflect actual progress.

**Actionable tasks:**

- [ ] Mark completed phases (1-4) with `[x]` checkboxes in PLAN.md
- [ ] Add Phase 7+ for features 007-016 or note them as extensions
- [ ] Update the Dependencies section to include `reqwest`, `tracing-appender`, and any other additions since initial plan
- [ ] Update the macOS Shell Commands section to note the `-W` flag behavior documented in Feature 016

---

## 5. High: docs/usage.md Inaccuracies

### 5.1 CLI Flag Discrepancies

- `docs/usage.md:107-114` uses `--last` flag syntax (`vigil outages --last 7d`) but the actual CLI may use `--period` or `-p` based on README.md examples (`vigil outages -p 7d`)
- Service installation section (line 182-186) says "instructions in 006-polish-service.md" despite the service commands being implemented

### 5.2 Shell Syntax Error

`docs/usage.md:227` has a shell command that won't work:
```bash
tail -f "~/Library/Application Support/ch.kapptec.vigil/monitor.log"
```
The tilde `~` inside double quotes is not expanded by the shell.

**Actionable tasks:**

- [ ] Verify and fix CLI flag names in `docs/usage.md` to match actual clap definitions
- [ ] Update service section with actual `vigil service install/start/stop/status` commands
- [ ] Fix the `tail -f` command to use proper shell expansion: `tail -f ~/Library/Application\ Support/ch.kapptec.vigil/monitor.log`

---

## 6. Medium: PRD.md Has No Resolution Tracking

`PRD.md` identifies 10 categories of issues with specific recommendations, but there is no tracking of which issues have been addressed by subsequent features.

For example:
- "Command injection vulnerability" (highest priority) - unclear if Feature 016's process timeout or other work addressed this
- "Database transaction safety" (high priority) - no feature tracks this
- "State machine race conditions" (high priority) - Feature 010 added periodic traceroutes but race condition status unknown

**Actionable tasks:**

- [ ] Add a "Resolution Status" column to each PRD issue, cross-referencing the feature that addressed it
- [ ] Audit whether command injection risk (PRD Section 8) has been mitigated in the ping/traceroute shell-out code
- [ ] Create issues or feature specs for unresolved PRD concerns (database transactions, config validation, flapping detection)

---

## 7. Medium: claude.md (AI Context) Gaps

`claude.md` serves as the development context file but has gaps:

### 7.1 Missing Features from Status Table

Features 007, 008, 012 are not listed in the status table at all.

### 7.2 Outdated Database Schema

`claude.md:113-117` shows only the original 3 tables (`outages`, `ping_log`, `traceroutes`) but is missing:
- `degraded_events` (Feature 010)
- `_meta` / `schema_version` (Feature 011)
- New columns on `traceroutes` (`degraded_event_id`, `trace_trigger`, `gateway_reachable`, `gateway_latency_ms`, `diagnosis`)

### 7.3 Feature 009 Status Confusion

Listed as "Pending" with a description of timing breakdown, cert tracking, and CLI commands, but Feature 014 (marked Done) implemented basic HTTP connectivity. The boundary between 009 and 014 is unclear.

**Actionable tasks:**

- [ ] Add features 007, 008, 012 to `claude.md` status table with correct statuses
- [ ] Update `claude.md` database schema to include all tables and columns
- [ ] Clarify the scope boundary between Feature 009 (advanced HTTP) and Feature 014 (basic TCP/HTTP)
- [ ] Update the "Testing" section to include the full test command suite

---

## 8. Medium: TEST.md Is Not a General Test Strategy

`TEST.md` is titled "Manual Testing Guide" but only covers Feature 010 (Enhanced Culprit Tracking). There is no general test strategy document.

**Actionable tasks:**

- [ ] Rename `TEST.md` to clarify its scope (e.g., "Manual Testing Guide - Feature 010") or expand it
- [ ] Create a general testing strategy document covering: unit tests, integration tests, manual test procedures for all features
- [ ] Document how to run tests in dev mode vs test mode (`VIGIL_ENV=test`)

---

## 9. Medium: Feature 009 vs 014 Overlap

Feature 009 (HTTP Endpoint Monitoring) is listed as "Pending" but notes "(basic HTTP connectivity implemented in Feature 014)". Feature 014 (TCP Connectivity Monitoring) is marked "Implemented" and includes both TCP and HTTP checks.

This creates confusion about:
- What remains to be done in Feature 009
- Whether Feature 009 is partially done, blocked, or superseded

**Actionable tasks:**

- [ ] Update Feature 009 status to clarify what specifically remains (timing breakdown, cert tracking, `vigil http`/`vigil certs` commands)
- [ ] Add a "Depends On" or "See Also" section linking 009 and 014
- [ ] Consider splitting 009's remaining work into a smaller, focused spec

---

## 10. Low: Missing Standard Documentation

### 10.1 No CHANGELOG

No `CHANGELOG.md` exists to track version history and breaking changes.

### 10.2 No CONTRIBUTING Guide

No guidance for contributors beyond what's in `claude.md`.

### 10.3 Feature Numbering Gap

Features jump from 006 to 007-016, but `docs/README.md` only lists 001-006. The numbering suggests a continuous series but the index is incomplete.

**Actionable tasks:**

- [ ] Create `CHANGELOG.md` with version history (at minimum v0.1.0 and v0.2.0 milestones)
- [ ] Update `docs/README.md` feature index to include all features 001-016

---

## 11. Low: Stale Example Data

All example outputs across documents use dates from January 2024 (e.g., `2024-01-15`). While this doesn't affect functionality, updating to more recent dates would improve perceived freshness.

**Actionable task:**

- [ ] Consider updating example dates across documentation (low priority, cosmetic)

---

## 12. Low: instructions.md Provides No Value

`instructions.md` contains only a transcript of the initial project creation conversation. It doesn't serve as actual instructions for developers or users.

**Actionable tasks:**

- [ ] Either remove `instructions.md` or repurpose it as a "Project History" document
- [ ] Ensure `claude.md` and `docs/usage.md` cover all necessary developer and user instructions

---

## Summary of Actionable Tasks by Priority

### Critical (blocks understanding of project state)
1. Synchronize feature statuses across `docs/README.md`, `claude.md`, and all feature specs
2. Update file structure references in `PLAN.md`, `docs/architecture.md`, and `docs/features/001-core-infrastructure.md`
3. Fix configuration format inconsistencies across all documents

### High (causes confusion or errors)
4. Mark completed task checkboxes in all "Done" feature specs
5. Update `PLAN.md` to reflect actual implementation progress
6. Fix CLI flag discrepancies and shell syntax error in `docs/usage.md`
7. Update `claude.md` database schema and feature table

### Medium (improves documentation quality)
8. Add resolution tracking to `PRD.md` issues
9. Clarify Feature 009 vs 014 scope and status
10. Expand or restructure `TEST.md` for general test coverage
11. Update `docs/architecture.md` with TCP/HTTP/gateway-first components

### Low (polish and completeness)
12. Create `CHANGELOG.md`
13. Complete `docs/README.md` feature index (001-016)
14. Decide fate of `instructions.md`
15. Update example dates (cosmetic)

---

## Files Reviewed

| File | Issues Found |
|------|-------------|
| `README.md` | Config format differs from implementation; good overall |
| `PLAN.md` | Frozen at planning stage; all checkboxes stale; outdated paths and structure |
| `PRD.md` | Good analysis but no resolution tracking |
| `TEST.md` | Too narrow; only covers Feature 010 |
| `claude.md` | Missing features 007/008/012; outdated schema; Feature 009 confusion |
| `instructions.md` | Not functional instructions; just creation transcript |
| `docs/architecture.md` | Outdated file structure; missing TCP/HTTP/degraded_events |
| `docs/README.md` | Only lists features 001-006; statuses wrong for 005/006 |
| `docs/usage.md` | CLI flag discrepancies; broken shell command; stale service section |
| `docs/features/001-*.md` | Outdated file tree; checkboxes unchecked despite "Done" |
| `docs/features/002-*.md` | Checkboxes unchecked despite "Done" |
| `docs/features/003-*.md` | Checkboxes unchecked despite "Done" |
| `docs/features/004-*.md` | Checkboxes unchecked despite "Done" |
| `docs/features/005-*.md` | Status says "Pending" but claude.md says "Done" |
| `docs/features/006-*.md` | Status says "Pending" but claude.md says "Done" |
| `docs/features/007-*.md` | Pending; not listed in claude.md status table |
| `docs/features/008-*.md` | Pending; not listed in claude.md status table |
| `docs/features/009-*.md` | Confusing overlap with Feature 014 |
| `docs/features/010-*.md` | Checkboxes unchecked despite "Done"; well-documented |
| `docs/features/011-*.md` | Good; marked "Implemented" |
| `docs/features/012-*.md` | Missing from claude.md; unclear if implemented |
| `docs/features/013-*.md` | Checkboxes unchecked despite "Done" |
| `docs/features/014-*.md` | Some tasks checked; good status tracking |
| `docs/features/015-*.md` | Checkboxes unchecked despite "Done" |
| `docs/features/016-*.md` | Checkboxes unchecked despite "Done" |
| `.claude/commands/pr.md` | Good; no issues found |
