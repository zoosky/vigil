use crate::config::MonitorConfig;
use crate::models::{ConnectivityState, DegradedEvent, Outage, PingResult, Target};
use std::collections::HashMap;

/// Event emitted when state changes
#[derive(Debug, Clone)]
pub enum StateEvent {
    /// Entered DEGRADED state - some targets failing
    Degraded {
        failing_targets: Vec<String>,
        degraded_event: DegradedEvent,
    },
    /// Entered OFFLINE state - outage started (escalated from degraded)
    Offline {
        outage: Outage,
        degraded_event: Option<DegradedEvent>,
    },
    /// Recovered to ONLINE state - outage ended
    Recovered { outage: Outage },
    /// Recovered from DEGRADED to ONLINE (no outage)
    DegradedRecovered { degraded_event: DegradedEvent },
    /// State unchanged
    NoChange,
}

/// Per-target connectivity state
#[derive(Debug, Clone)]
pub struct TargetState {
    pub target: Target,
    pub last_result: Option<PingResult>,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
}

impl TargetState {
    pub fn new(target: Target) -> Self {
        Self {
            target,
            last_result: None,
            consecutive_failures: 0,
            consecutive_successes: 0,
        }
    }

    /// Update state with a new ping result
    pub fn update(&mut self, result: &PingResult) {
        if result.success {
            self.consecutive_failures = 0;
            self.consecutive_successes += 1;
        } else {
            self.consecutive_successes = 0;
            self.consecutive_failures += 1;
        }
        self.last_result = Some(result.clone());
    }

    /// Check if this target is currently failing
    pub fn is_failing(&self) -> bool {
        self.consecutive_failures > 0
    }
}

/// Tracks connectivity state across multiple targets
pub struct ConnectivityTracker {
    state: ConnectivityState,
    config: MonitorConfig,
    target_states: HashMap<String, TargetState>,
    current_outage: Option<Outage>,
    current_degraded_event: Option<DegradedEvent>,

    // Aggregate counters for state transitions (incremented once per round)
    aggregate_failures: u32,
    aggregate_successes: u32,

    // Round tracking: only evaluate state after all targets report
    expected_targets: usize,
    results_this_round: usize,
}

impl ConnectivityTracker {
    /// Create a new connectivity tracker
    pub fn new(config: &MonitorConfig, targets: &[Target]) -> Self {
        let target_states = targets
            .iter()
            .map(|t| (t.ip.clone(), TargetState::new(t.clone())))
            .collect();

        Self {
            state: ConnectivityState::Online,
            config: config.clone(),
            target_states,
            current_outage: None,
            current_degraded_event: None,
            aggregate_failures: 0,
            aggregate_successes: 0,
            expected_targets: targets.len().max(1),
            results_this_round: 0,
        }
    }

    /// Process a batch of ping results from a single monitoring round.
    /// All results from one round should be collected and passed together
    /// so that aggregate counters increment once per round, not once per target.
    pub fn process_round(&mut self, results: &[PingResult]) -> StateEvent {
        // Update per-target state for each result in the round
        for result in results {
            if let Some(target_state) = self.target_states.get_mut(&result.target) {
                target_state.update(result);
            }
        }

        self.evaluate_state()
    }

    /// Process a single ping result (for backwards compatibility).
    /// Tracks how many results have arrived this round and only evaluates
    /// state transitions once all targets have reported.
    pub fn process(&mut self, result: &PingResult) -> StateEvent {
        // Update target-specific state
        if let Some(target_state) = self.target_states.get_mut(&result.target) {
            target_state.update(result);
        }

        self.results_this_round += 1;

        // Only evaluate state once all targets have reported for this round
        if self.results_this_round >= self.expected_targets {
            self.results_this_round = 0;
            self.evaluate_state()
        } else {
            StateEvent::NoChange
        }
    }

    /// Evaluate aggregate state and perform state machine transitions.
    /// Called once per complete round of results.
    fn evaluate_state(&mut self) -> StateEvent {
        // Count currently failing targets
        let failing_targets: Vec<String> = self
            .target_states
            .values()
            .filter(|t| t.is_failing())
            .map(|t| t.target.ip.clone())
            .collect();

        let any_failing = !failing_targets.is_empty();
        let all_healthy = failing_targets.is_empty();

        // Update aggregate counters (once per round)
        if any_failing {
            self.aggregate_successes = 0;
            self.aggregate_failures += 1;
        } else {
            self.aggregate_failures = 0;
            self.aggregate_successes += 1;
        }

        // State machine transitions
        match self.state {
            ConnectivityState::Online => {
                if self.aggregate_failures >= self.config.degraded_threshold {
                    self.state = ConnectivityState::Degraded;
                    let degraded_event = self.start_degraded_event(failing_targets.clone());
                    tracing::warn!(
                        "State: ONLINE -> DEGRADED ({} consecutive failures)",
                        self.aggregate_failures
                    );
                    return StateEvent::Degraded {
                        failing_targets: failing_targets.clone(),
                        degraded_event,
                    };
                }
            }
            ConnectivityState::Degraded => {
                if all_healthy && self.aggregate_successes >= self.config.recovery_threshold {
                    self.state = ConnectivityState::Online;
                    self.aggregate_failures = 0;
                    let degraded_event = self.end_degraded_event();
                    tracing::info!(
                        "State: DEGRADED -> ONLINE ({} consecutive successes)",
                        self.aggregate_successes
                    );
                    if let Some(event) = degraded_event {
                        return StateEvent::DegradedRecovered {
                            degraded_event: event,
                        };
                    }
                    return StateEvent::NoChange;
                }
                if self.aggregate_failures >= self.config.offline_threshold {
                    self.state = ConnectivityState::Offline;
                    let degraded_event = self.escalate_degraded_event();
                    let outage = self.start_outage(failing_targets.clone());
                    tracing::error!(
                        "State: DEGRADED -> OFFLINE ({} consecutive failures) - Outage started",
                        self.aggregate_failures
                    );
                    return StateEvent::Offline {
                        outage,
                        degraded_event,
                    };
                }
            }
            ConnectivityState::Offline => {
                if all_healthy && self.aggregate_successes >= self.config.recovery_threshold {
                    if let Some(outage) = self.end_outage() {
                        self.state = ConnectivityState::Online;
                        self.aggregate_failures = 0;
                        tracing::info!(
                            "State: OFFLINE -> ONLINE ({} consecutive successes) - Outage ended, duration: {:.1}s",
                            self.aggregate_successes,
                            outage.duration_secs.unwrap_or(0.0)
                        );
                        return StateEvent::Recovered { outage };
                    }
                }
            }
        }

        StateEvent::NoChange
    }

    /// Start a new outage
    fn start_outage(&mut self, affected_targets: Vec<String>) -> Outage {
        let outage = Outage::new(affected_targets);
        self.current_outage = Some(outage.clone());
        outage
    }

    /// End the current outage
    fn end_outage(&mut self) -> Option<Outage> {
        if let Some(mut outage) = self.current_outage.take() {
            outage.end();
            Some(outage)
        } else {
            None
        }
    }

    /// Start a new degraded event
    fn start_degraded_event(&mut self, affected_targets: Vec<String>) -> DegradedEvent {
        let event = DegradedEvent::new(affected_targets);
        self.current_degraded_event = Some(event.clone());
        event
    }

    /// End the current degraded event (recovered without escalation)
    fn end_degraded_event(&mut self) -> Option<DegradedEvent> {
        if let Some(mut event) = self.current_degraded_event.take() {
            event.end();
            Some(event)
        } else {
            None
        }
    }

    /// Escalate the current degraded event to an outage
    fn escalate_degraded_event(&mut self) -> Option<DegradedEvent> {
        if let Some(mut event) = self.current_degraded_event.take() {
            // Note: The outage ID will be set after the outage is inserted in the database
            // For now we just end the event - the caller should link it to the outage
            event.end();
            Some(event)
        } else {
            None
        }
    }

    /// Get current connectivity state
    pub fn state(&self) -> ConnectivityState {
        self.state
    }

    /// Get current outage (if any)
    pub fn current_outage(&self) -> Option<&Outage> {
        self.current_outage.as_ref()
    }

    /// Get mutable reference to current outage (for updating)
    pub fn current_outage_mut(&mut self) -> Option<&mut Outage> {
        self.current_outage.as_mut()
    }

    /// Get current degraded event (if any)
    pub fn current_degraded_event(&self) -> Option<&DegradedEvent> {
        self.current_degraded_event.as_ref()
    }

    /// Get all target states
    pub fn target_states(&self) -> &HashMap<String, TargetState> {
        &self.target_states
    }

    /// Get failing targets
    pub fn failing_targets(&self) -> Vec<&TargetState> {
        self.target_states
            .values()
            .filter(|t| t.is_failing())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_config() -> MonitorConfig {
        MonitorConfig {
            ping_interval_ms: 1000,
            ping_timeout_ms: 2000,
            degraded_threshold: 3,
            offline_threshold: 5,
            recovery_threshold: 2,
            traceroute_interval_secs: 60,
            max_traceroutes_per_outage: 10,
            ping_process_timeout_ms: 6000,
            degraded_ping_interval_ms: 500,
        }
    }

    fn make_targets() -> Vec<Target> {
        vec![
            Target::new("Google DNS", "8.8.8.8"),
            Target::new("Cloudflare", "1.1.1.1"),
        ]
    }

    fn success_ping(target: &str) -> PingResult {
        PingResult {
            target: target.to_string(),
            target_name: "Test".to_string(),
            timestamp: Utc::now(),
            success: true,
            latency_ms: Some(10.0),
            error: None,
        }
    }

    fn failure_ping(target: &str) -> PingResult {
        PingResult {
            target: target.to_string(),
            target_name: "Test".to_string(),
            timestamp: Utc::now(),
            success: false,
            latency_ms: None,
            error: Some("timeout".to_string()),
        }
    }

    #[test]
    fn test_initial_state_online() {
        let config = make_config();
        let targets = make_targets();
        let tracker = ConnectivityTracker::new(&config, &targets);

        assert_eq!(tracker.state(), ConnectivityState::Online);
        assert!(tracker.current_outage().is_none());
    }

    #[test]
    fn test_online_to_degraded() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // With 2 targets, need to send both results per round to complete a round.
        // degraded_threshold = 3 rounds of failure.
        for round in 0..3 {
            // Send failure for first target — round not yet complete
            let event = tracker.process(&failure_ping("8.8.8.8"));
            assert!(
                matches!(event, StateEvent::NoChange),
                "mid-round should be NoChange"
            );

            // Send failure for second target — completes the round
            let event = tracker.process(&failure_ping("1.1.1.1"));
            if round < 2 {
                assert!(matches!(event, StateEvent::NoChange));
                assert_eq!(tracker.state(), ConnectivityState::Online);
            } else {
                assert!(matches!(event, StateEvent::Degraded { .. }));
                assert_eq!(tracker.state(), ConnectivityState::Degraded);
            }
        }
    }

    #[test]
    fn test_degraded_to_offline() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Get to degraded state (3 rounds of failure with 2 targets each)
        for _ in 0..3 {
            tracker.process(&failure_ping("8.8.8.8"));
            tracker.process(&failure_ping("1.1.1.1"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // Continue failing until offline threshold (5 total rounds)
        for round in 3..5 {
            tracker.process(&failure_ping("8.8.8.8"));
            let event = tracker.process(&failure_ping("1.1.1.1"));
            if round < 4 {
                assert!(matches!(event, StateEvent::NoChange));
            } else {
                assert!(matches!(event, StateEvent::Offline { .. }));
                assert_eq!(tracker.state(), ConnectivityState::Offline);
            }
        }

        assert!(tracker.current_outage().is_some());
    }

    #[test]
    fn test_offline_to_online_recovery() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Get to offline state (5 rounds of failure with 2 targets)
        for _ in 0..5 {
            tracker.process(&failure_ping("8.8.8.8"));
            tracker.process(&failure_ping("1.1.1.1"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Offline);
        assert!(tracker.current_outage().is_some());

        // Recovery requires recovery_threshold (2) consecutive successful rounds
        // Round 1: both succeed
        tracker.process(&success_ping("8.8.8.8"));
        tracker.process(&success_ping("1.1.1.1"));
        assert_eq!(tracker.state(), ConnectivityState::Offline);

        // Round 2: both succeed — should recover
        tracker.process(&success_ping("8.8.8.8"));
        let event = tracker.process(&success_ping("1.1.1.1"));
        assert!(matches!(event, StateEvent::Recovered { .. }));
        assert_eq!(tracker.state(), ConnectivityState::Online);
        assert!(tracker.current_outage().is_none());

        // Verify outage has duration
        if let StateEvent::Recovered { outage } = event {
            assert!(outage.duration_secs.is_some());
            assert!(outage.end_time.is_some());
        }
    }

    #[test]
    fn test_degraded_recovery_without_outage() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Get to degraded state (3 rounds)
        for _ in 0..3 {
            tracker.process(&failure_ping("8.8.8.8"));
            tracker.process(&failure_ping("1.1.1.1"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // Recover before going offline (2 rounds of success)
        for _ in 0..2 {
            tracker.process(&success_ping("8.8.8.8"));
            tracker.process(&success_ping("1.1.1.1"));
        }

        // Should be back online, no outage recorded
        assert_eq!(tracker.state(), ConnectivityState::Online);
        assert!(tracker.current_outage().is_none());
    }

    #[test]
    fn test_single_failure_no_state_change() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // One round: one target fails, one succeeds — any_failing so aggregate_failures=1
        tracker.process(&failure_ping("8.8.8.8"));
        let event = tracker.process(&success_ping("1.1.1.1"));
        assert!(matches!(event, StateEvent::NoChange));
        assert_eq!(tracker.state(), ConnectivityState::Online);

        // Next round: all succeed — should reset counters
        tracker.process(&success_ping("8.8.8.8"));
        tracker.process(&success_ping("1.1.1.1"));
        assert_eq!(tracker.state(), ConnectivityState::Online);
    }

    #[test]
    fn test_flap_prevention() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Get to degraded (3 rounds)
        for _ in 0..3 {
            tracker.process(&failure_ping("8.8.8.8"));
            tracker.process(&failure_ping("1.1.1.1"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // One round of success — not enough (need recovery_threshold=2)
        tracker.process(&success_ping("8.8.8.8"));
        tracker.process(&success_ping("1.1.1.1"));

        // Then another failure round — resets success counter
        tracker.process(&failure_ping("8.8.8.8"));
        tracker.process(&failure_ping("1.1.1.1"));

        // Should still be degraded due to flapping
        assert_eq!(tracker.state(), ConnectivityState::Degraded);
    }

    #[test]
    fn test_target_state_tracking() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Complete one round: one fails, one succeeds
        tracker.process(&failure_ping("8.8.8.8"));
        tracker.process(&success_ping("1.1.1.1"));

        let failing = tracker.failing_targets();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].target.ip, "8.8.8.8");

        // Other target still healthy
        let states = tracker.target_states();
        let cloudflare = states.get("1.1.1.1").unwrap();
        assert!(!cloudflare.is_failing());
    }

    #[test]
    fn test_multi_target_round_counting() {
        // This test verifies the critical fix: with 2 targets, a single round
        // of failures should only increment aggregate_failures by 1, not 2.
        let config = make_config(); // degraded_threshold=3, offline_threshold=5
        let targets = make_targets(); // 2 targets
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Round 1: both targets fail
        tracker.process(&failure_ping("8.8.8.8"));
        tracker.process(&failure_ping("1.1.1.1"));
        assert_eq!(
            tracker.state(),
            ConnectivityState::Online,
            "1 round < threshold 3"
        );

        // Round 2: both fail again
        tracker.process(&failure_ping("8.8.8.8"));
        tracker.process(&failure_ping("1.1.1.1"));
        assert_eq!(
            tracker.state(),
            ConnectivityState::Online,
            "2 rounds < threshold 3"
        );

        // Round 3: both fail — now hits degraded_threshold=3
        tracker.process(&failure_ping("8.8.8.8"));
        tracker.process(&failure_ping("1.1.1.1"));
        assert_eq!(
            tracker.state(),
            ConnectivityState::Degraded,
            "3 rounds = threshold 3"
        );
        // Must NOT be Offline — old bug would have reached offline here
        assert!(
            tracker.current_outage().is_none(),
            "should not have jumped to Offline"
        );
    }

    #[test]
    fn test_process_round_batch() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Use process_round to send a full batch at once
        for _ in 0..3 {
            let results = vec![failure_ping("8.8.8.8"), failure_ping("1.1.1.1")];
            tracker.process_round(&results);
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // Two more rounds to go offline
        for _ in 0..2 {
            tracker.process_round(&[failure_ping("8.8.8.8"), failure_ping("1.1.1.1")]);
        }
        assert_eq!(tracker.state(), ConnectivityState::Offline);
    }
}
