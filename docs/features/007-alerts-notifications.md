# 007 - Alerts & Notifications

**Status:** Pending

## Overview

Implement a notification system to alert users when network state changes occur, enabling real-time awareness of outages without requiring active monitoring of the CLI.

## Objectives

- Send alerts on state transitions (ONLINE → DEGRADED → OFFLINE)
- Support multiple notification channels (desktop, webhook, command)
- Configurable alert thresholds and rate limiting
- Include relevant context in notifications (failing hop, affected targets)

## Notification Channels

### 1. Desktop Notifications (macOS)

Using `osascript` to trigger native macOS notifications:

```bash
osascript -e 'display notification "Network offline - Hop 3 (ISP) failing" with title "Network Monitor" sound name "Basso"'
```

### 2. Webhook (HTTP POST)

For integration with Slack, Discord, PagerDuty, or custom services:

```json
{
  "event": "offline",
  "timestamp": "2024-01-15T14:23:05Z",
  "state": "offline",
  "previous_state": "degraded",
  "failing_hop": 3,
  "failing_hop_ip": "10.0.0.1",
  "affected_targets": ["8.8.8.8", "1.1.1.1"],
  "duration_so_far": null
}
```

Recovery notification:
```json
{
  "event": "recovered",
  "timestamp": "2024-01-15T14:23:47Z",
  "state": "online",
  "previous_state": "offline",
  "outage_duration_secs": 42,
  "failing_hop": 3,
  "failing_hop_ip": "10.0.0.1"
}
```

### 3. Command Execution

Run arbitrary command with environment variables:

```bash
NETMON_EVENT=offline \
NETMON_STATE=offline \
NETMON_FAILING_HOP=3 \
NETMON_FAILING_IP=10.0.0.1 \
/path/to/custom-script.sh
```

## Implementation

### File: `src/notifications.rs`

```rust
pub struct NotificationManager {
    config: NotificationConfig,
    last_notification: HashMap<NotificationEvent, Instant>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub enum NotificationEvent {
    Degraded,
    Offline,
    Recovered,
}

impl NotificationManager {
    pub fn new(config: NotificationConfig) -> Self;

    /// Send notification for state change
    pub async fn notify(&mut self, event: &StateEvent) -> Result<()>;

    /// Check rate limiting
    fn should_notify(&self, event: &NotificationEvent) -> bool;
}
```

### Desktop Notification Implementation

```rust
async fn send_desktop_notification(title: &str, message: &str, sound: bool) -> Result<()> {
    let sound_part = if sound {
        " sound name \"Basso\""
    } else {
        ""
    };

    let script = format!(
        "display notification \"{}\" with title \"{}\"{}",
        message.replace("\"", "\\\""),
        title.replace("\"", "\\\""),
        sound_part
    );

    Command::new("osascript")
        .args(["-e", &script])
        .status()
        .await?;

    Ok(())
}
```

### Webhook Implementation

```rust
async fn send_webhook(url: &str, payload: &WebhookPayload) -> Result<()> {
    let client = reqwest::Client::new();

    let response = client
        .post(url)
        .json(payload)
        .timeout(Duration::from_secs(10))
        .send()
        .await?;

    if !response.status().is_success() {
        tracing::warn!("Webhook returned {}: {}", response.status(), url);
    }

    Ok(())
}
```

## Configuration

```toml
[notifications]
enabled = true

# Desktop notifications (macOS)
[notifications.desktop]
enabled = true
on_degraded = false      # Only notify on offline by default
on_offline = true
on_recovered = true
sound = true

# Webhook notifications
[notifications.webhook]
enabled = false
url = "https://hooks.slack.com/services/..."
on_degraded = true
on_offline = true
on_recovered = true

# Custom command
[notifications.command]
enabled = false
path = "/path/to/script.sh"
on_degraded = false
on_offline = true
on_recovered = true

# Rate limiting
[notifications.rate_limit]
min_interval_secs = 60   # Don't spam notifications
```

## Integration with State Machine

```rust
// In monitor loop
match tracker.process(result) {
    StateEvent::Degraded { .. } => {
        notifier.notify(&event).await?;
    }
    StateEvent::Offline { outage } => {
        db.insert_outage(&outage);
        notifier.notify(&event).await?;
    }
    StateEvent::Recovered { outage } => {
        db.update_outage(&outage);
        notifier.notify(&event).await?;
    }
    _ => {}
}
```

## Tasks

- [ ] Define `NotificationConfig` struct
- [ ] Implement `NotificationManager`
- [ ] Implement desktop notifications via osascript
- [ ] Implement webhook notifications with reqwest
- [ ] Implement command execution with env vars
- [ ] Add rate limiting logic
- [ ] Add configuration parsing
- [ ] Add unit tests for notification formatting
- [ ] Add integration tests with mock webhook server

## Dependencies

```toml
reqwest = { version = "0.11", features = ["json"] }
```

## Test Plan

### Unit Tests

```rust
#[test]
fn test_webhook_payload_serialization() {
    let payload = WebhookPayload::offline(...);
    let json = serde_json::to_string(&payload).unwrap();
    assert!(json.contains("\"event\":\"offline\""));
}

#[test]
fn test_rate_limiting() {
    let mut manager = NotificationManager::new(...);
    manager.notify(&event).await;

    // Second notification within rate limit should be skipped
    let result = manager.notify(&event).await;
    assert!(result.is_ok()); // Doesn't error, just skips
}
```

### Manual Testing

```bash
# Test desktop notification
networkmonitor test-notify desktop "Test message"

# Test webhook (with httpbin or similar)
networkmonitor test-notify webhook

# Test command execution
networkmonitor test-notify command
```

## Error Handling

| Error | Handling |
|-------|----------|
| Webhook timeout | Log warning, continue monitoring |
| Webhook 4xx/5xx | Log warning, continue monitoring |
| Command execution failure | Log error, continue monitoring |
| osascript not found | Log warning on startup, disable desktop |

Notifications must never block or crash the main monitoring loop.

## Acceptance Criteria

1. Desktop notifications appear on macOS when outages occur
2. Webhook sends valid JSON POST to configured URL
3. Command executed with correct environment variables
4. Rate limiting prevents notification spam
5. Notification failures don't affect monitoring
6. Configuration allows selective enable/disable per channel
