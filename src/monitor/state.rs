use crate::config::MonitorConfig;
use crate::models::{ConnectivityState, Outage, PingResult, Target};
use std::collections::HashMap;

/// Event emitted when state changes
#[derive(Debug, Clone)]
pub enum StateEvent {
    /// Entered DEGRADED state - some targets failing
    Degraded { failing_targets: Vec<String> },
    /// Entered OFFLINE state - outage started
    Offline { outage: Outage },
    /// Recovered to ONLINE state - outage ended
    Recovered { outage: Outage },
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

    // Aggregate counters for state transitions
    aggregate_failures: u32,
    aggregate_successes: u32,
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
            aggregate_failures: 0,
            aggregate_successes: 0,
        }
    }

    /// Process a ping result, returns any state change event
    pub fn process(&mut self, result: &PingResult) -> StateEvent {
        // Update target-specific state
        if let Some(target_state) = self.target_states.get_mut(&result.target) {
            target_state.update(result);
        }

        // Count currently failing targets
        let failing_targets: Vec<String> = self
            .target_states
            .values()
            .filter(|t| t.is_failing())
            .map(|t| t.target.ip.clone())
            .collect();

        let any_failing = !failing_targets.is_empty();
        let all_healthy = failing_targets.is_empty();

        // Update aggregate counters
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
                    tracing::warn!(
                        "State: ONLINE -> DEGRADED ({} consecutive failures)",
                        self.aggregate_failures
                    );
                    return StateEvent::Degraded {
                        failing_targets: failing_targets.clone(),
                    };
                }
            }
            ConnectivityState::Degraded => {
                if all_healthy && self.aggregate_successes >= self.config.recovery_threshold {
                    self.state = ConnectivityState::Online;
                    self.aggregate_failures = 0;
                    tracing::info!(
                        "State: DEGRADED -> ONLINE ({} consecutive successes)",
                        self.aggregate_successes
                    );
                    return StateEvent::NoChange; // No outage to report
                }
                if self.aggregate_failures >= self.config.offline_threshold {
                    self.state = ConnectivityState::Offline;
                    let outage = self.start_outage(failing_targets.clone());
                    tracing::error!(
                        "State: DEGRADED -> OFFLINE ({} consecutive failures) - Outage started",
                        self.aggregate_failures
                    );
                    return StateEvent::Offline { outage };
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

        // Send failures until degraded threshold (3)
        for i in 0..3 {
            let event = tracker.process(&failure_ping("8.8.8.8"));
            if i < 2 {
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

        // Get to degraded state
        for _ in 0..3 {
            tracker.process(&failure_ping("8.8.8.8"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // Continue failing until offline threshold (5 total)
        for i in 3..5 {
            let event = tracker.process(&failure_ping("8.8.8.8"));
            if i < 4 {
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

        // Get to offline state
        for _ in 0..5 {
            tracker.process(&failure_ping("8.8.8.8"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Offline);
        assert!(tracker.current_outage().is_some());

        // Recovery requires successes from ALL targets
        // First success
        tracker.process(&success_ping("8.8.8.8"));
        assert_eq!(tracker.state(), ConnectivityState::Offline);

        // Second success - should recover
        let event = tracker.process(&success_ping("8.8.8.8"));
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

        // Get to degraded state
        for _ in 0..3 {
            tracker.process(&failure_ping("8.8.8.8"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // Recover before going offline
        for _ in 0..2 {
            tracker.process(&success_ping("8.8.8.8"));
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

        // Single failure should not change state
        let event = tracker.process(&failure_ping("8.8.8.8"));
        assert!(matches!(event, StateEvent::NoChange));
        assert_eq!(tracker.state(), ConnectivityState::Online);

        // Success should reset
        tracker.process(&success_ping("8.8.8.8"));
        assert_eq!(tracker.state(), ConnectivityState::Online);
    }

    #[test]
    fn test_flap_prevention() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Get to degraded
        for _ in 0..3 {
            tracker.process(&failure_ping("8.8.8.8"));
        }
        assert_eq!(tracker.state(), ConnectivityState::Degraded);

        // Single success should not recover
        tracker.process(&success_ping("8.8.8.8"));
        // This resets aggregate_failures but we need 2 consecutive successes

        // Another failure
        tracker.process(&failure_ping("8.8.8.8"));

        // Should still be degraded due to flapping
        assert_eq!(tracker.state(), ConnectivityState::Degraded);
    }

    #[test]
    fn test_target_state_tracking() {
        let config = make_config();
        let targets = make_targets();
        let mut tracker = ConnectivityTracker::new(&config, &targets);

        // Fail one target
        tracker.process(&failure_ping("8.8.8.8"));

        let failing = tracker.failing_targets();
        assert_eq!(failing.len(), 1);
        assert_eq!(failing[0].target.ip, "8.8.8.8");

        // Other target still healthy
        let states = tracker.target_states();
        let cloudflare = states.get("1.1.1.1").unwrap();
        assert!(!cloudflare.is_failing());
    }
}
