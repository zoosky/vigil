use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Represents the current connectivity state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectivityState {
    Online,
    Degraded,
    Offline,
}

impl std::fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectivityState::Online => write!(f, "ONLINE"),
            ConnectivityState::Degraded => write!(f, "DEGRADED"),
            ConnectivityState::Offline => write!(f, "OFFLINE"),
        }
    }
}

/// Result of a single ping attempt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub target: String,
    pub target_name: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub latency_ms: Option<f64>,
    pub error: Option<String>,
}

/// A network hop from traceroute
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerouteHop {
    pub hop_number: u8,
    pub ip: Option<String>,
    pub hostname: Option<String>,
    pub latency_ms: Option<f64>,
    pub timeout: bool,
}

/// Full traceroute result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracerouteResult {
    pub target: String,
    pub timestamp: DateTime<Utc>,
    pub hops: Vec<TracerouteHop>,
    pub success: bool,
}

/// An outage event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outage {
    pub id: Option<i64>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_secs: Option<f64>,
    pub affected_targets: Vec<String>,
    pub failing_hop: Option<u8>,
    pub failing_hop_ip: Option<String>,
    pub notes: Option<String>,
}

impl Outage {
    pub fn new(affected_targets: Vec<String>) -> Self {
        Self {
            id: None,
            start_time: Utc::now(),
            end_time: None,
            duration_secs: None,
            affected_targets,
            failing_hop: None,
            failing_hop_ip: None,
            notes: None,
        }
    }

    pub fn end(&mut self) {
        let now = Utc::now();
        self.end_time = Some(now);
        self.duration_secs = Some((now - self.start_time).num_milliseconds() as f64 / 1000.0);
    }
}

/// A monitoring target
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub ip: String,
}

impl Target {
    pub fn new(name: impl Into<String>, ip: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ip: ip.into(),
        }
    }
}

/// A degraded event (before escalating to outage)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DegradedEvent {
    pub id: Option<i64>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_secs: Option<f64>,
    pub escalated_to_outage_id: Option<i64>,
    pub affected_targets: Vec<String>,
    pub notes: Option<String>,
}

impl DegradedEvent {
    pub fn new(affected_targets: Vec<String>) -> Self {
        Self {
            id: None,
            start_time: Utc::now(),
            end_time: None,
            duration_secs: None,
            escalated_to_outage_id: None,
            affected_targets,
            notes: None,
        }
    }

    pub fn end(&mut self) {
        let now = Utc::now();
        self.end_time = Some(now);
        self.duration_secs = Some((now - self.start_time).num_milliseconds() as f64 / 1000.0);
    }

    pub fn escalate(&mut self, outage_id: i64) {
        self.escalated_to_outage_id = Some(outage_id);
        self.end();
    }
}

/// Traceroute trigger type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceTrigger {
    StateChange,
    Periodic,
    Manual,
}

impl std::fmt::Display for TraceTrigger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceTrigger::StateChange => write!(f, "state_change"),
            TraceTrigger::Periodic => write!(f, "periodic"),
            TraceTrigger::Manual => write!(f, "manual"),
        }
    }
}

impl std::str::FromStr for TraceTrigger {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "state_change" => Ok(TraceTrigger::StateChange),
            "periodic" => Ok(TraceTrigger::Periodic),
            "manual" => Ok(TraceTrigger::Manual),
            _ => Err(format!("Unknown trace trigger: {}", s)),
        }
    }
}

/// Statistics summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stats {
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_outages: u32,
    pub total_downtime_secs: f64,
    pub availability_percent: f64,
    pub avg_outage_duration_secs: Option<f64>,
    pub most_common_failing_hop: Option<u8>,
}

/// Interpret hop number to human-readable name
pub fn interpret_hop(hop: u8) -> &'static str {
    match hop {
        1 => "Gateway",
        2 => "ISP Modem",
        3 => "ISP Router",
        _ => "ISP Backbone",
    }
}
