# Feature 017: Code Audit — Bug Fixes & Hardening

**Status:** Pending
**Priority:** Critical/High mix
**Scope:** Fix bugs, security issues, and reliability problems discovered in code audit

---

## Executive Summary

Full source code review of the vigil codebase identified **19 issues** across 6 categories: state machine logic errors, missing process timeouts, security concerns, database integrity gaps, performance problems, and minor correctness issues. The most critical bug causes premature state transitions (ONLINE → OFFLINE in a single monitoring round), undermining the core purpose of the tool.

---

## 1. CRITICAL: State Machine Counter Increments Per-Result, Not Per-Round

**File:** `src/monitor/state.rs:96-120`
**Impact:** Core monitoring logic is broken — state transitions happen too fast

### Problem

`ConnectivityTracker::process()` is called once per ping result (one call per target per round). With N targets, there are N calls per monitoring round. The aggregate failure/success counters increment on **every call**:

```rust
if any_failing {
    self.aggregate_successes = 0;
    self.aggregate_failures += 1;  // Incremented N times per round!
}
```

With default config (3 targets: Gateway, Google DNS, Cloudflare) and thresholds (`degraded_threshold=2`, `offline_threshold=3`):

- Round 1: 3 results processed → `aggregate_failures` goes 1 → 2 → 3
  - At result 2: DEGRADED triggers (threshold=2)
  - At result 3: OFFLINE triggers (threshold=3)

**A single round of failure takes the monitor from ONLINE to OFFLINE instantly.** The intent is 2 rounds for DEGRADED and 3 rounds for OFFLINE.

### Fix

- [ ] Track the current "round" by counting results until all targets have reported
- [ ] Only increment aggregate counters once per complete round
- [ ] Option A: Buffer results until all targets report, then evaluate
- [ ] Option B: Track round completion via a counter (`results_this_round`) and only update aggregates when `results_this_round == targets.len()`

### Test Cases

- [ ] With 3 targets, verify DEGRADED requires 2 full rounds of all-fail
- [ ] With 3 targets, verify OFFLINE requires 3 full rounds of all-fail
- [ ] Mixed results (1 target failing, 2 healthy) should not trigger state change
- [ ] Recovery requires `recovery_threshold` full rounds of all-success

---

## 2. CRITICAL: Traceroute Command Has No Process Timeout

**File:** `src/monitor/traceroute.rs:30-46`
**Impact:** Monitor can block indefinitely during diagnosis

### Problem

The `trace()` method uses `.output().await` with no timeout:

```rust
let output = Command::new("traceroute")
    .args(["-n", "-q", "1", "-w", &self.timeout_secs.to_string(), "-m", &self.max_hops.to_string(), target])
    .output()
    .await;
```

Unlike `ping_target()` in `ping.rs` which has a careful `tokio::select!` with process kill, traceroute has no process-level timeout. A traceroute on a broken network with 30 hops at 2s timeout per hop can take 60+ seconds. During an outage — exactly when traceroute is invoked — the network stack is most likely to hang.

### Fix

- [ ] Add `tokio::select!` with a hard timeout (e.g., `30s` or configurable) around the traceroute subprocess, matching the pattern in `ping.rs:163-266`
- [ ] Kill the traceroute process if it exceeds the timeout
- [ ] Return an empty/failed TracerouteResult on timeout

---

## 3. CRITICAL: Gateway Ping Has No Process Timeout

**File:** `src/monitor/traceroute.rs:154-161`
**Impact:** Gateway ping can block diagnosis indefinitely

### Problem

`HopAnalyzer::ping_host()` uses `.output().await` without a process timeout:

```rust
let output = Command::new("ping")
    .args(["-c", "1", "-W", "2000", host])
    .output()
    .await;
```

This is the same class of bug as #2. The ping subprocess can hang when the network stack is broken, blocking the entire diagnosis flow. Additionally, the timeout is hardcoded to `2000ms` instead of using the configured `ping_timeout_ms`.

### Fix

- [ ] Add `tokio::select!` with process timeout, matching the pattern in `ping.rs`
- [ ] Accept `timeout_ms` and `process_timeout_ms` parameters from config instead of hardcoding `2000`
- [ ] Kill the subprocess on timeout

---

## 4. HIGH: Argument Injection in Shell Commands

**Files:** `src/monitor/ping.rs:140-141`, `src/monitor/traceroute.rs:34-46, 158`
**Impact:** Malicious config values could alter command behavior

### Problem

User-provided IP/hostname strings from `config.toml` are passed directly as arguments to `ping` and `traceroute`. While Rust's `Command::new()` doesn't use a shell (so no shell injection), a target IP like `--version` or `-V` would be interpreted as a flag:

```rust
Command::new("ping").args(["-c", "1", "-W", "2000", "--version"])
// Runs: ping -c 1 -W 2000 --version (prints version, exits 0 → false positive)
```

### Fix

- [ ] Validate target IPs/hostnames on config load: must match `^[a-zA-Z0-9.:_-]+$` regex and not start with `-`
- [ ] Use `--` separator before the target argument: `.args(["-c", "1", "-W", &timeout, "--", ip])`
- [ ] Add the same `--` separator to traceroute invocations

---

## 5. HIGH: No Database Transactions Around Multi-Step Persistence

**Files:** `src/main.rs:537-564`, `src/db.rs`
**Impact:** Crash between insert_outage and insert_traceroute leaves inconsistent data

### Problem

Outage creation and traceroute insertion are separate operations without a transaction:

```rust
// main.rs:537 — Step 1: Insert outage
match app.db.insert_outage(&outage_to_save) {
    Ok(id) => {
        // ... lots of code ...
        // main.rs:554 — Step 2: Insert traceroute (can fail independently)
        if let Err(e) = app.db.insert_traceroute(Some(id), ...) {
```

If the process crashes or is killed between these operations, the database has an outage without its diagnostic traceroute.

### Fix

- [ ] Add a `Database::transaction()` method that wraps a closure in `BEGIN/COMMIT`
- [ ] Wrap outage+traceroute insertion in a single transaction
- [ ] Wrap degraded_event+traceroute insertion in a single transaction
- [ ] Wrap cleanup operations in a single transaction (prevents orphaned records)

---

## 6. HIGH: Database Migrations Not Transactional

**File:** `src/db.rs:96-201`
**Impact:** Failed migration leaves database in inconsistent state

### Problem

Each migration uses `execute_batch()` which executes multiple SQL statements. If one statement fails mid-batch (e.g., `ALTER TABLE` succeeds but `INSERT INTO schema_version` fails), the database is left with new columns but the version isn't recorded. The next startup will try to re-apply the migration and fail on the duplicate column.

### Fix

- [ ] Wrap each migration in an explicit `BEGIN TRANSACTION` / `COMMIT`
- [ ] Use `self.conn.execute_batch("BEGIN; ... COMMIT;")` or use the `Transaction` API
- [ ] Add error recovery: check for partially-applied migrations by detecting existing columns

---

## 7. HIGH: Cleanup Orphans Traceroute Records

**File:** `src/db.rs:588-613`
**Impact:** Database accumulates orphaned traceroutes with invalid outage_id references

### Problem

The cleanup function deletes outages by `start_time < cutoff`, but traceroutes by their own `timestamp < cutoff`. For a long-running outage that started before the cutoff but has periodic traceroutes after the cutoff:

1. The outage is deleted (start_time is old)
2. Newer periodic traceroutes referencing that outage_id survive
3. These traceroutes now reference a non-existent outage (orphaned)

### Fix

- [ ] Delete traceroutes WHERE `outage_id IN (SELECT id FROM outages WHERE start_time < cutoff)` to cascade-delete related traceroutes
- [ ] Or: enable `PRAGMA foreign_keys = ON` and use `ON DELETE CASCADE` in the schema
- [ ] Apply the same approach to degraded_events ↔ traceroutes

---

## 8. HIGH: HTTP Client Recreated On Every Check

**File:** `src/monitor/http.rs:32-47`
**Impact:** Inflated latency measurements, no TLS session reuse

### Problem

A new `reqwest::Client` is created for every HTTP connectivity check:

```rust
let client = match Client::builder()
    .timeout(Duration::from_millis(timeout_ms))
    .build()
```

`reqwest::Client` is designed to be reused — it maintains connection pools, DNS cache, and TLS session cache. Creating a new client each time means every check includes full DNS resolution and TLS handshake overhead, artificially inflating latency measurements.

### Fix

- [ ] Create the `Client` once (e.g., as a `lazy_static` or pass it as a parameter)
- [ ] Option A: Store in `PingMonitor` struct
- [ ] Option B: Use `once_cell::sync::Lazy<Client>` module-level static

---

## 9. MEDIUM: `identify_failing_hop` Naming Confusion

**File:** `src/monitor/traceroute.rs:73-88`
**Impact:** Inconsistent use of `failing_hop_ip` throughout codebase

### Problem

`HopAnalyzer::identify_failing_hop()` returns the **last responding hop** (the hop BEFORE the failure), but the name implies it returns the failing hop itself. This leads to confusion across callers:

- `main.rs:478`: Stores the returned IP as `failing_hop_ip` — but this is the last responding hop's IP
- `outages.rs:102`: Prints `"Hop {hop} (after {ip})"` using `failing_hop_ip` — "after" is also confusing
- `outage_detail.rs:84`: Prints `"Hop {hop} (after {ip})"` — same issue

The `Outage.failing_hop` field sometimes contains the failing hop number (from `analyze_diagnosis` which adds +1) and sometimes the last-responding hop number (from `identify_failing_hop` which doesn't add +1).

### Fix

- [ ] Rename `identify_failing_hop` → `identify_last_responding_hop` and update return type docs
- [ ] Rename `Outage.failing_hop_ip` → `last_responding_hop_ip` (or keep but document clearly)
- [ ] Audit all callers to ensure consistent semantics: `failing_hop` = the hop number that failed, `failing_hop_ip` = the last hop that DID respond

---

## 10. MEDIUM: No Configuration Validation

**File:** `src/config.rs`
**Impact:** Invalid configs cause confusing runtime errors

### Problem

The config is deserialized from TOML but never validated. Invalid configurations include:

- `ping_interval_ms = 0` (busy loop)
- `ping_timeout_ms > ping_interval_ms` (overlapping pings)
- `ping_process_timeout_ms < ping_timeout_ms` (process killed before ping times out)
- `degraded_threshold > offline_threshold` (impossible state transition)
- `recovery_threshold = 0` (instant recovery)
- Empty target IP: `ip = ""`
- Target IP starting with `-` (argument injection, see #4)
- Port out of range: `port = 0` for TCP/HTTP

### Fix

- [ ] Add a `Config::validate(&self) -> Result<(), ConfigError>` method
- [ ] Call it after loading config in `Config::load_for_env()`
- [ ] Validate: `ping_process_timeout_ms >= ping_timeout_ms`
- [ ] Validate: `degraded_threshold < offline_threshold`
- [ ] Validate: `recovery_threshold >= 1`
- [ ] Validate: all target IPs are non-empty and don't start with `-`
- [ ] Validate: TCP/HTTP target ports are 1-65535

---

## 11. MEDIUM: `truncate` Function Can Panic on Multi-Byte UTF-8

**File:** `src/cli/helpers.rs:65-73`
**Impact:** Latent panic if non-ASCII characters appear in IP/hostname display

### Problem

```rust
format!("{}...", &s[..max_len - 3])
```

`s[..n]` uses byte indexing. If `s` contains multi-byte UTF-8 characters and `max_len - 3` falls on a non-character-boundary, this panics. Currently only used with ASCII strings (IPs, numbers) so it doesn't trigger, but it's a latent crash.

### Fix

- [ ] Replace byte slicing with `s.chars().take(max_len - 3).collect::<String>()` or use `s.char_indices()` to find the correct boundary

---

## 12. MEDIUM: `foreground` Flag Ignored

**File:** `src/main.rs:281`
**Impact:** `vigil start` always runs in foreground; `--foreground` is misleading

### Problem

```rust
async fn cmd_start(_foreground: bool, env: &Environment) -> ...
```

The `foreground` parameter is accepted but prefixed with `_` (unused). The monitor always runs in the foreground. Users might expect `vigil start` (without `--foreground`) to daemonize.

### Fix

- [ ] Either implement daemonization for `vigil start` (without `--foreground`)
- [ ] Or remove the `--foreground` flag and document that background execution uses `vigil service install`
- [ ] The launchd plist already uses `--foreground`, so removing the flag requires a plist update

---

## 13. MEDIUM: `config set` Command Is a Stub

**File:** `src/main.rs:271-276`
**Impact:** Accepts user input but does nothing

### Problem

```rust
ConfigAction::Set { key, value } => {
    println!("Setting {} = {}", key, value);
    println!("(Configuration editing not yet implemented - edit config file directly)");
```

This silently discards user input. A user running `vigil config set monitor.ping_interval_ms 500` thinks they've changed the config, but nothing happened.

### Fix

- [ ] Either implement config editing (load TOML, modify key, save)
- [ ] Or remove the `Set` subcommand and update help text to point users to the config file directly

---

## 14. LOW: SQLite WAL Mode Not Enabled

**File:** `src/db.rs:42`
**Impact:** Potential contention if CLI commands run while monitor is active

### Problem

The database is opened without WAL mode. In default journal mode, readers block writers and vice versa. If a user runs `vigil status` or `vigil outages` while the monitor is writing, one operation blocks the other.

### Fix

- [ ] Add `conn.execute_batch("PRAGMA journal_mode=WAL;")` after opening the database
- [ ] WAL mode allows concurrent readers and a single writer without blocking

---

## 15. LOW: Plist Generation Doesn't XML-Escape Binary Path

**File:** `src/cli/service.rs:25-61`
**Impact:** Invalid plist if binary path contains `<`, `>`, or `&`

### Problem

```rust
format!(r#"<string>{}</string>"#, binary_str)
```

If `binary_str` contains XML-special characters (e.g., the path includes `&`), the generated plist is malformed XML.

### Fix

- [ ] XML-escape `binary_str` before inserting into the plist template
- [ ] Replace `&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`

---

## 16. LOW: Ping Results Only Logged on Status Change

**File:** `src/main.rs:688-709`
**Impact:** Sparse `ping_log` table, no continuous latency data

### Problem

Ping results are only inserted into the database when the target's status changes:

```rust
let should_print = last_status.get(&key) != Some(&current);
if should_print {
    // ... insert_ping()
}
```

This means the `ping_log` table has extremely sparse data — only transition points. Continuous latency tracking, jitter analysis, or detailed availability calculations are impossible.

### Fix

- [ ] Add a configurable sampling rate (e.g., `log_every_nth_ping = 10`) to log periodic pings
- [ ] Or always log but add a retention policy for ping_log that is more aggressive than outage retention
- [ ] Consider this for Feature 008 (Latency Quality Metrics) which needs continuous latency data

---

## Summary of Tasks by Priority

### Critical (fix immediately — core logic broken)
1. Fix state machine per-result counter bug (Section 1)
2. Add process timeout to traceroute subprocess (Section 2)
3. Add process timeout to gateway ping (Section 3)

### High (fix soon — reliability/security issues)
4. Add argument injection protection with `--` and input validation (Section 4)
5. Wrap multi-step DB operations in transactions (Section 5)
6. Make migrations transactional (Section 6)
7. Fix cleanup to prevent orphaned records (Section 7)
8. Reuse HTTP client across checks (Section 8)

### Medium (improve correctness)
9. Fix `identify_failing_hop` naming and semantics (Section 9)
10. Add configuration validation (Section 10)
11. Fix `truncate` UTF-8 safety (Section 11)
12. Resolve `--foreground` flag behavior (Section 12)
13. Implement or remove `config set` stub (Section 13)

### Low (polish)
14. Enable SQLite WAL mode (Section 14)
15. XML-escape plist binary path (Section 15)
16. Add configurable ping logging (Section 16)

---

## Cross-References

- **PRD.md Section 2**: Race condition in periodic traceroute → Partially addressed by Section 1 (state machine fix)
- **PRD.md Section 3**: No transaction management → Section 5
- **PRD.md Section 4**: Missing config validation → Section 10
- **PRD.md Section 8**: Command injection risk → Section 4 (argument injection is the actual risk; shell injection is already mitigated by Rust's `Command`)
- **Feature 008** (Latency Quality Metrics): Depends on Section 16 (continuous ping logging)
