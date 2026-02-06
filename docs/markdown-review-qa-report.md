# Markdown Documentation Review & QA Report

**Date:** 2026-02-06
**Scope:** All `*.md` files in the vigil project (27 files reviewed)
**Purpose:** Functional review of documentation quality, consistency, and accuracy
**Resolution:** All actionable tasks completed 2026-02-06

---

## Executive Summary

The vigil project has extensive documentation across 27+ markdown files spanning a root-level README, architecture docs, 18 feature specifications, a PRD review, test guide, and development context files. A comprehensive documentation sync was performed to bring all files in line with the actual implementation state.

---

## 1. Critical: Feature Status Mismatches

Multiple documents previously disagreed on which features are implemented.

### 1.1 docs/README.md vs claude.md

**Actionable tasks:**

- [x] Update `docs/README.md` feature table to include all features with correct statuses
- [x] Update `docs/features/005-cli-reporting.md` status from "Pending" to "Done"
- [x] Update `docs/features/006-polish-service.md` status from "Pending" to "Done"
- [x] Ensure all three sources of truth (docs/README.md, claude.md, individual feature specs) agree

### 1.2 Unchecked Task Checkboxes in Completed Features

**Actionable tasks:**

- [x] Convert `[ ]` to `[x]` for all completed tasks in features: 001, 002, 003, 004, 005, 006, 010, 013, 014, 015, 016
- [x] For Feature 012 (CI Pipeline Fix), confirmed implemented and status correct
- [x] For Feature 014, optional tasks ("ICMP rate-limiting detection" and "migration notice") annotated as optional/deferred

---

## 2. Critical: Outdated File Structure References

### 2.1 PLAN.md File Structure

**Actionable tasks:**

- [x] Update `PLAN.md` file structure to match actual codebase
- [x] Update `docs/architecture.md` file structure to match actual codebase
- [x] Update `docs/features/001-core-infrastructure.md` file tree to reflect final state

### 2.2 Architecture.md Outdated

**Actionable tasks:**

- [x] Add TCP/HTTP monitoring methods to architecture component diagram
- [x] Add `degraded_events` table to database section
- [x] Update data flow diagram to include gateway-first diagnosis
- [x] Update component descriptions to reference all monitor types (ping, TCP, HTTP)

---

## 3. High: Configuration Format Inconsistencies

### 3.1 Config File Location

**Actionable task:**

- [x] Update `PLAN.md` config path to `~/Library/Application Support/ch.kapptec.vigil/config.toml`

### 3.2 Target Configuration Syntax

**Actionable tasks:**

- [x] Determined canonical TOML config format from `config.rs` implementation
- [x] Updated all markdown files to use canonical config format consistently
- [x] Updated: `PLAN.md`, `docs/usage.md`, `docs/features/001-core-infrastructure.md`

---

## 4. High: PLAN.md Staleness

**Actionable tasks:**

- [x] Mark completed phases (1-6) with `[x]` checkboxes in PLAN.md
- [x] Add Phase 7+ for features 010-016 and Phase 8 for pending features
- [x] Update the Dependencies section to include `reqwest`, `tracing-appender`, and all other additions
- [x] Update the macOS Shell Commands section to note the `-W` flag behavior

---

## 5. High: docs/usage.md Inaccuracies

**Actionable tasks:**

- [x] Verified CLI flag names in `docs/usage.md` match actual clap definitions (`--last` for outages, `--period` for stats)
- [x] Updated service section with actual `vigil service install/uninstall/status/logs` commands
- [x] Fixed the `tail -f` command to use proper shell expansion

---

## 6. Medium: PRD.md Has No Resolution Tracking

**Actionable tasks:**

- [x] Added "Resolution Tracking" table to PRD.md cross-referencing features that address each issue
- [x] Command injection risk noted as needing audit (tracked in Feature 017)
- [x] Unresolved PRD concerns covered by Feature 017 (Code Audit Fixes)

---

## 7. Medium: claude.md (AI Context) Gaps

**Actionable tasks:**

- [x] Added features 007, 008, 012, 017, 018 to `claude.md` status table
- [x] Updated `claude.md` database schema to include all tables and columns (v3)
- [x] Clarified Feature 009 status as "Partial" with explanation of scope vs Feature 014
- [x] Updated Dependencies section with all actual dependencies

---

## 8. Medium: TEST.md Is Not a General Test Strategy

**Actionable tasks:**

- [x] Expanded `TEST.md` with general testing overview section (automated tests, test environments)
- [x] Documented how to run tests in dev mode vs test mode (`VIGIL_ENV=test`)
- [x] Retained Feature 010 manual testing section for reference

---

## 9. Medium: Feature 009 vs 014 Overlap

**Actionable tasks:**

- [x] Updated Feature 009 status to "Partial" with clear explanation of remaining work
- [x] Added "See Also" section in Feature 009 linking to Feature 014
- [x] Scope boundary documented: 014 = basic TCP/HTTP connectivity, 009 = advanced HTTP features

---

## 10. Low: Missing Standard Documentation

**Actionable tasks:**

- [x] Created `CHANGELOG.md` with v0.1.0 and v0.2.0 milestones
- [x] Updated `docs/README.md` feature index to include all features 001-018

---

## 11. Low: Stale Example Data

**Actionable task:**

- [ ] Consider updating example dates across documentation (low priority, cosmetic) — deferred

---

## 12. Low: instructions.md Provides No Value

**Actionable tasks:**

- [x] `instructions.md` repurposed as "Project History" document
- [x] `claude.md` and `docs/usage.md` cover all developer and user instructions

---

## Summary of Actionable Tasks by Priority

### Critical (blocks understanding of project state)
1. [x] Synchronize feature statuses across `docs/README.md`, `claude.md`, and all feature specs
2. [x] Update file structure references in `PLAN.md`, `docs/architecture.md`, and `docs/features/001-core-infrastructure.md`
3. [x] Fix configuration format inconsistencies across all documents

### High (causes confusion or errors)
4. [x] Mark completed task checkboxes in all "Done" feature specs
5. [x] Update `PLAN.md` to reflect actual implementation progress
6. [x] Fix CLI flag discrepancies and shell syntax error in `docs/usage.md`
7. [x] Update `claude.md` database schema and feature table

### Medium (improves documentation quality)
8. [x] Add resolution tracking to `PRD.md` issues
9. [x] Clarify Feature 009 vs 014 scope and status
10. [x] Expand `TEST.md` for general test coverage
11. [x] Update `docs/architecture.md` with TCP/HTTP/gateway-first components

### Low (polish and completeness)
12. [x] Create `CHANGELOG.md`
13. [x] Complete `docs/README.md` feature index (001-018)
14. [x] Repurpose `instructions.md` as "Project History"
15. [ ] Update example dates (cosmetic) — deferred
