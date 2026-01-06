# 003 - Outage Detection

**Status:** Pending

## Overview

Implement a state machine to track connectivity state and detect outages with hysteresis to prevent flapping.

## Objectives

- Track per-target connectivity state
- Aggregate to overall connectivity state
- Emit events on state transitions
- Create outage records on entering OFFLINE state
- End outage records on recovery

## State Machine

```
                    ┌─────────────────┐
                    │     ONLINE      │
                    └────────┬────────┘
                             │
                             │ degraded_threshold consecutive failures
                             ▼
                    ┌─────────────────┐
              ┌─────│    DEGRADED     │─────┐
              │     └────────┬────────┘     │
              │              │              │
    recovery_threshold       │ offline_threshold
    consecutive successes    │ consecutive failures
              │              │              │
              │              ▼              │
              │     ┌─────────────────┐     │
              └─────│    OFFLINE      │─────┘
                    └─────────────────┘
                             │
                             │ recovery_threshold consecutive successes
                             ▼
                    ┌─────────────────┐
                    │     ONLINE      │
                    └─────────────────┘
```

## Implementation

### File: `src/monitor/state.rs`

```rust
pub struct ConnectivityTracker {
    state: ConnectivityState,
    failure_count: u32,
    success_count: u32,
    config: MonitorConfig,
    current_outage: Option<Outage>,

    // Per-target state
    target_states: HashMap<String, TargetState>,
}

pub struct TargetState {
    pub target: Target,
    pub last_result: Option<PingResult>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
}

pub enum StateEvent {
    /// Entered DEGRADED state
    Degraded { failing_targets: Vec<String> },
    /// Entered OFFLINE state, outage started
    Offline { outage: Outage },
    /// Recovered to ONLINE state, outage ended
    Recovered { outage: Outage },
    /// State unchanged
    NoChange,
}

impl ConnectivityTracker {
    pub fn new(config: &MonitorConfig, targets: &[Target]) -> Self;

    /// Process a ping result, returns any state change event
    pub fn process(&mut self, result: PingResult) -> StateEvent;

    /// Get current state
    pub fn state(&self) -> ConnectivityState;

    /// Get current outage (if any)
    pub fn current_outage(&self) -> Option<&Outage>;
}
```

### State Transition Logic

```rust
fn process(&mut self, result: PingResult) -> StateEvent {
    // Update target state
    let target_state = self.target_states.get_mut(&result.target);

    if result.success {
        target_state.consecutive_failures = 0;
        target_state.consecutive_successes += 1;
    } else {
        target_state.consecutive_successes = 0;
        target_state.consecutive_failures += 1;
    }

    // Aggregate: count targets with failures
    let failing_targets: Vec<_> = self.target_states
        .values()
        .filter(|t| t.consecutive_failures >= 1)
        .collect();

    // Check for state transitions
    match self.state {
        Online => {
            if failing_targets.len() >= threshold {
                self.state = Degraded;
                return StateEvent::Degraded { ... };
            }
        }
        Degraded => {
            if all_healthy {
                self.state = Online;
            } else if failures_continue {
                self.state = Offline;
                self.start_outage();
                return StateEvent::Offline { ... };
            }
        }
        Offline => {
            if recovery_count >= recovery_threshold {
                self.end_outage();
                self.state = Online;
                return StateEvent::Recovered { ... };
            }
        }
    }

    StateEvent::NoChange
}
```

## Tasks

- [ ] Define `ConnectivityTracker` struct
- [ ] Implement per-target state tracking
- [ ] Implement state transition logic
- [ ] Implement `StateEvent` enum
- [ ] Integrate with outage creation/completion
- [ ] Add unit tests for all state transitions
- [ ] Test edge cases (rapid flapping, partial failures)

## Test Plan

### Unit Tests

```rust
#[test]
fn test_online_to_degraded() {
    let mut tracker = ConnectivityTracker::new(...);

    // Send 3 failures
    for _ in 0..3 {
        let event = tracker.process(failing_result());
    }

    assert_eq!(tracker.state(), ConnectivityState::Degraded);
}

#[test]
fn test_degraded_to_offline() {
    let mut tracker = /* already degraded */;

    // Send 2 more failures
    for _ in 0..2 {
        tracker.process(failing_result());
    }

    assert_eq!(tracker.state(), ConnectivityState::Offline);
    assert!(tracker.current_outage().is_some());
}

#[test]
fn test_recovery() {
    let mut tracker = /* in offline state */;

    // Send 2 successes
    for _ in 0..2 {
        tracker.process(success_result());
    }

    assert_eq!(tracker.state(), ConnectivityState::Online);
    assert!(tracker.current_outage().is_none());
}

#[test]
fn test_flap_prevention() {
    // Single success during DEGRADED should not recover immediately
}
```

## Configuration Used

```toml
[monitor]
degraded_threshold = 3    # Failures to enter DEGRADED
offline_threshold = 5     # Failures to enter OFFLINE
recovery_threshold = 2    # Successes to recover
```

## Integration with Other Features

### From Feature 002 (Ping Monitor)

Receives `PingResult` stream:
```rust
while let Some(result) = ping_results.recv().await {
    let event = tracker.process(result);
    match event {
        StateEvent::Offline { outage } => {
            db.insert_outage(&outage);
            // Trigger traceroute (Feature 004)
        }
        StateEvent::Recovered { outage } => {
            db.update_outage(&outage);
        }
        _ => {}
    }
}
```

### To Feature 004 (Hop Analysis)

When entering OFFLINE state, triggers traceroute to identify failing hop.

## Acceptance Criteria

1. State transitions follow the defined thresholds
2. Outage records created when entering OFFLINE
3. Outage records updated (end_time, duration) on recovery
4. No false positives from single packet loss
5. Recovers correctly after extended outages
