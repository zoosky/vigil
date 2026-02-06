 # PRD REVIEW
 
 ## Error Analysis of Vigil Network Monitor Design

  ### 1. Error Handling & Resilience Issues

  #### Critical: Notification Failures Can Block Monitoring (Feature 007)

  - Issue: Documentation states "Notifications must never block or crash the main monitoring loop" but doesn't specify timeout implementation
  - Risk: Webhook timeouts (10s) could accumulate and delay monitoring cycles
  - Recommendation: Implement non-blocking notification dispatch with bounded channels

  #### Process Timeout Implementation (Feature 016) ✓

  - Status: Correctly addressed - subprocess timeouts prevent monitor blocking
  - Good design: Hard timeout wraps system command execution

  #### Missing Retry Logic

  - Issue: No retry strategy for transient failures in TCP/HTTP checks
  - Location: Features 009, 014 - single attempt per check
  - Recommendation: Add configurable retry with exponential backoff for connection failures

  ### 2. State Machine & Detection Logic

  #### Threshold Configuration Conflicts

  - Issue: State transition thresholds could create impossible states
  - Example: If recovery_threshold > degraded_threshold, recovery requires fewer successes than entering degraded
  - Recommendation: Add validation that recovery_threshold >= degraded_threshold >= 1

  #### Race Condition in Periodic Traceroute (Feature 010)

  - Issue: last_traceroute tracking with Instant but no mutex protection
  - Location: Monitor loop checks and updates last_traceroute across async tasks
  - Recommendation: Use Arc<Mutex<Option<Instant>>> or atomic timestamp

  #### Flapping Detection Missing

  - Issue: No protection against rapid state oscillations
  - Scenario: Network flaps between ONLINE/DEGRADED every few seconds
  - Recommendation: Add minimum state duration before allowing transitions (e.g., 30s)

  ### 3. Database & Persistence

  #### No Transaction Management in State Transitions

  - Issue: Outage creation + traceroute insertion not wrapped in transaction
  - Location: Features 003, 004, 010 - separate insert operations
  - Risk: Orphaned traceroutes if outage insert fails
  - Recommendation: Use SQLite transactions for atomic state persistence

  #### Unbounded Traceroute Storage (Feature 010)

  - Issue: max_traceroutes_per_outage config mentioned but no enforcement in DB operations
  - Risk: Database growth if long outages trigger many periodic traces
  - Recommendation: Implement FIFO deletion when limit reached

  #### Database Lock Contention

  - Issue: No connection pooling mentioned; single SQLite connection shared across async tasks
  - Risk: database is locked errors under concurrent read/write
  - Recommendation: Use rusqlite with WAL mode and connection pool

  ### 4. Configuration Validation

  #### Missing Validation Rules

  Missing validations for:

  - ping_interval_ms < ping_timeout_ms (interval should be >= timeout)
  - ping_process_timeout_ms < ping_interval_ms (could accumulate tasks)
  - Port ranges (1-65535) for TCP/HTTP targets
  - URL format validation for HTTP targets
  - Circular gateway detection (gateway can't be itself)

  #### Default Gateway Auto-detection Failure Handling

  - Issue: Feature 001 mentions gateway auto-detection but no fallback if detection fails
  - Risk: Monitor starts without gateway target, reducing diagnostic value
  - Recommendation: Warn user and continue with external targets only

  ### 5. Monitoring Method Issues

  #### ICMP vs TCP Method Mismatch (Feature 014)

  - Issue: Traceroute still uses ICMP even when monitoring uses TCP
  - Problem: TCP check fails → trigger traceroute → ICMP traceroute succeeds → false diagnosis
  - Recommendation: Use TCP-based probing (like tcptraceroute) when primary method is TCP

  #### HTTP Method Latency Misleading (Feature 009)

  - Issue: HTTP latency includes DNS, TLS, transfer - not comparable to ICMP
  - Risk: State machine thresholds tuned for ICMP may not work for HTTP
  - Recommendation: Separate threshold configs per method or normalize latency metrics

  ### 6. Resource Management

  #### No Rate Limiting on Concurrent Operations

  - Issue: All targets pinged concurrently with no semaphore
  - Risk: Burst of 100+ concurrent TCP connections if misconfigured
  - Recommendation: Add max concurrent checks limit (e.g., 10)

  #### Log Rotation Timing Window (Feature 006)

  - Issue: Daily rotation at fixed time not specified
  - Risk: Rotation during critical outage could lose data
  - Recommendation: Use size-based rotation or ensure buffered writes flush

  #### Memory Leak Risk in Quality Metrics (Feature 008)

  - Issue: VecDeque in QualityMetrics with fixed window_size but no explicit cleanup
  - Risk: Long-running process with many targets accumulates memory
  - Recommendation: Verify deque properly drops old entries when full

  ### 7. Platform-Specific Issues

  #### macOS Command Parsing Fragility

  - Issue: Ping/traceroute parsing uses string matching, no version detection
  - Risk: macOS updates change output format → parsing breaks
  - Example: Feature 002 expects exact format "time=14.123 ms"
  - Recommendation: Use multiple regex patterns with fallbacks

  #### launchd Service Race Condition (Feature 006)

  - Issue: vigil service install immediately loads service
  - Risk: Service starts before config file is reviewed by user
  - Recommendation: Install plist but require manual launchctl load or separate start command

  ### 8. Security Concerns

  #### Command Injection Risk

  - Issue: User-provided IPs/hostnames passed to shell commands
  - Location: ping, traceroute, openssl commands use string interpolation
  - Risk: IP like "8.8.8.8; rm -rf /" could execute arbitrary commands
  - Status: Needs audit - ensure all inputs are validated/sanitized

  #### TLS Certificate Checking Uses openssl Command (Feature 009)

  - Issue: Shells out to openssl s_client instead of using Rust TLS library
  - Risk: Parsing complexity, no timeout specified
  - Recommendation: Use rustls or native-tls crate for certificate inspection

  ### 9. Migration & Upgrade Path

  #### No Rollback Strategy

  - Issue: Feature 011 migrations have down SQL but no rollback command
  - Risk: Failed migration requires manual database recovery
  - Recommendation: Add vigil downgrade command or automated rollback on failure

  #### Backup Creation Without Verification (Feature 011)

  - Issue: create_backup() copies file but doesn't verify integrity
  - Risk: Corrupted backup discovered only when needed
  - Recommendation: Verify backup is readable SQLite database

  ### 10. Documentation Gaps

  #### Error Code Standards Missing

  - No documented error codes for different failure types
  - Difficult to programmatically detect specific issues
  - Recommendation: Define error taxonomy (ERR_001: Gateway unreachable, etc.)

  #### No Disaster Recovery Documentation

  - What if database corrupts?
  - What if config file is malformed?
  - No documented recovery procedures

  ## Resolution Tracking

  | Issue | Section | Priority | Status | Addressed By |
  |-------|---------|----------|--------|-------------|
  | Notification blocking monitoring | 1 | Critical | Open | Feature 007 (pending) |
  | Process timeout | 1 | Critical | **Resolved** | Feature 016 |
  | Missing retry logic (TCP/HTTP) | 1 | Medium | Open | — |
  | Threshold config conflicts | 2 | Medium | Open | Needs config validation |
  | Race condition in periodic traceroute | 2 | High | Open | Needs audit in Feature 010 code |
  | Flapping detection | 2 | Medium | Open | — |
  | No transaction management | 3 | High | Open | — |
  | Unbounded traceroute storage | 3 | Medium | Partial | Config exists, enforcement needs audit |
  | Database lock contention | 3 | Medium | Open | — |
  | Missing config validation | 4 | Medium | Open | — |
  | Gateway auto-detection fallback | 4 | Low | Open | — |
  | ICMP vs TCP method mismatch | 5 | Medium | Open | — |
  | HTTP latency thresholds | 5 | Medium | Open | Feature 009 (pending) |
  | No rate limiting on concurrency | 6 | Medium | Open | — |
  | Log rotation timing | 6 | Low | Partial | Feature 006 (daily rotation) |
  | Memory leak in quality metrics | 6 | Low | N/A | Feature 008 not implemented |
  | macOS command parsing fragility | 7 | Medium | Open | — |
  | launchd service race condition | 7 | Low | Open | Feature 006 |
  | Command injection risk | 8 | **Highest** | Open | Needs audit of ping/traceroute shell-out |
  | openssl shell-out for certs | 8 | Medium | N/A | Feature 009 not implemented |
  | No rollback strategy | 9 | Low | Open | — |
  | Backup without verification | 9 | Low | Open | Feature 011 |
  | Error code standards | 10 | Low | Open | — |
  | Disaster recovery docs | 10 | Low | Open | — |

  ## Summary of Critical Issues

  1. **Highest Priority**: Command injection vulnerability needs immediate audit
  2. High Priority: Database transaction safety, state machine race conditions
  3. Medium Priority: Configuration validation, retry logic, method-specific thresholds
  4. Low Priority: Documentation gaps, error codes, rollback strategy

  ## Recommendations for Next Steps

  1. Add comprehensive input validation and sanitization
  2. Implement database transactions for state changes
  3. Add configuration validation on load
  4. Implement method-aware traceroute (TCP tracing for TCP monitors)
  5. Add integration tests for state machine edge cases
  6. Document error handling and recovery procedures
