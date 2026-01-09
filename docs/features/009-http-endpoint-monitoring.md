# 009 - HTTP Endpoint Monitoring

**Status:** Pending

## Overview

Extend monitoring capabilities beyond ICMP ping to include HTTP/HTTPS endpoint checks. This provides a more complete picture of connectivity by testing the full network stack, including DNS resolution, TCP connection, TLS handshake, and HTTP response validation.

## Objectives

- Monitor HTTP/HTTPS endpoints alongside ICMP targets
- Measure DNS resolution time separately
- Track TLS/SSL certificate expiration
- Validate HTTP response codes and content
- Provide timing breakdown (DNS, connect, TLS, transfer)

## Why HTTP Monitoring?

ICMP ping has limitations:

1. **ICMP can succeed while HTTP fails** - Firewall rules, proxy issues, application errors
2. **No application-layer visibility** - Can't detect if a web service is actually responding
3. **DNS issues invisible** - Ping uses IP directly, misses DNS failures
4. **TLS problems undetected** - Certificate issues, protocol mismatches

HTTP monitoring provides end-to-end verification that services are actually reachable.

## Timing Breakdown

```
Total Request Time
├── DNS Resolution    (dns_ms)
├── TCP Connect       (connect_ms)
├── TLS Handshake     (tls_ms)       [HTTPS only]
├── Time to First Byte (ttfb_ms)
└── Content Transfer  (transfer_ms)
```

## Implementation

### File: `src/monitor/http.rs`

```rust
pub struct HttpMonitor {
    client: reqwest::Client,
    targets: Vec<HttpTarget>,
    timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct HttpTarget {
    pub name: String,
    pub url: String,
    pub method: Method,
    pub expected_status: Option<u16>,
    pub expected_body_contains: Option<String>,
    pub check_certificate: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct HttpResult {
    pub target: String,
    pub url: String,
    pub timestamp: DateTime<Utc>,
    pub success: bool,
    pub status_code: Option<u16>,

    // Timing breakdown (milliseconds)
    pub dns_ms: Option<f64>,
    pub connect_ms: Option<f64>,
    pub tls_ms: Option<f64>,
    pub ttfb_ms: Option<f64>,
    pub total_ms: Option<f64>,

    // Certificate info (HTTPS only)
    pub cert_expires_in_days: Option<i64>,
    pub cert_issuer: Option<String>,

    // Error info
    pub error: Option<String>,
    pub error_phase: Option<HttpPhase>,
}

#[derive(Debug, Clone, Serialize)]
pub enum HttpPhase {
    DnsResolution,
    TcpConnect,
    TlsHandshake,
    HttpRequest,
    ResponseValidation,
}

impl HttpMonitor {
    pub fn new(config: &HttpConfig) -> Self;

    /// Check a single HTTP endpoint
    pub async fn check(&self, target: &HttpTarget) -> HttpResult;

    /// Check all configured endpoints
    pub async fn check_all(&self) -> Vec<HttpResult>;
}
```

### Timing Measurement

```rust
async fn check_with_timing(&self, target: &HttpTarget) -> HttpResult {
    let start = Instant::now();

    // DNS timing (manual resolution)
    let dns_start = Instant::now();
    let url: Url = target.url.parse()?;
    let host = url.host_str().unwrap();
    let addrs = tokio::net::lookup_host(format!("{}:80", host)).await;
    let dns_ms = dns_start.elapsed().as_secs_f64() * 1000.0;

    // HTTP request with connection timing
    let response = self.client
        .request(target.method.clone(), &target.url)
        .timeout(self.timeout)
        .send()
        .await;

    let total_ms = start.elapsed().as_secs_f64() * 1000.0;

    match response {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let success = target.expected_status
                .map(|s| s == status)
                .unwrap_or(resp.status().is_success());

            HttpResult {
                success,
                status_code: Some(status),
                dns_ms: Some(dns_ms),
                total_ms: Some(total_ms),
                ..
            }
        }
        Err(e) => {
            HttpResult {
                success: false,
                error: Some(e.to_string()),
                error_phase: Some(classify_error(&e)),
                ..
            }
        }
    }
}
```

### Certificate Checking

```rust
async fn check_certificate(url: &str) -> Option<CertInfo> {
    // Use openssl s_client for detailed cert info
    let output = Command::new("openssl")
        .args([
            "s_client",
            "-connect", &format!("{}:443", host),
            "-servername", host,
        ])
        .stdin(Stdio::null())
        .output()
        .await?;

    // Parse certificate expiration
    let cert_output = Command::new("openssl")
        .args(["x509", "-noout", "-dates"])
        .stdin(Stdio::piped())
        .output()
        .await?;

    parse_cert_dates(&cert_output.stdout)
}
```

## Database Schema Extension

```sql
CREATE TABLE http_log (
    id INTEGER PRIMARY KEY,
    timestamp TEXT NOT NULL,
    target TEXT NOT NULL,
    url TEXT NOT NULL,
    success INTEGER NOT NULL,
    status_code INTEGER,
    dns_ms REAL,
    connect_ms REAL,
    tls_ms REAL,
    ttfb_ms REAL,
    total_ms REAL,
    error TEXT,
    error_phase TEXT
);

CREATE TABLE cert_status (
    id INTEGER PRIMARY KEY,
    target TEXT NOT NULL,
    url TEXT NOT NULL,
    last_checked TEXT NOT NULL,
    expires_at TEXT,
    days_until_expiry INTEGER,
    issuer TEXT,
    is_valid INTEGER
);

CREATE INDEX idx_http_log_timestamp ON http_log(timestamp);
```

## Configuration

```toml
[http]
enabled = true
check_interval_secs = 60    # Less frequent than ping
timeout_secs = 10

[[http.targets]]
name = "Google"
url = "https://www.google.com"
method = "GET"
expected_status = 200

[[http.targets]]
name = "Company API"
url = "https://api.example.com/health"
method = "GET"
expected_status = 200
expected_body_contains = "\"status\":\"ok\""

[[http.targets]]
name = "Internal Service"
url = "http://192.168.1.100:8080/ping"
method = "HEAD"

[http.certificates]
check_enabled = true
warning_days = 30           # Warn when cert expires within 30 days
critical_days = 7           # Critical alert within 7 days
```

## CLI Integration

### `vigil http`

```
HTTP Endpoint Status
═══════════════════════════════════════════════════════════

Target: Google (https://www.google.com)
  Status: 200 OK
  Total: 145ms (DNS: 12ms, Connect: 45ms, TLS: 58ms, TTFB: 30ms)
  Certificate: Valid, expires in 62 days

Target: Company API (https://api.example.com/health)
  Status: 200 OK
  Total: 89ms (DNS: 8ms, Connect: 25ms, TLS: 32ms, TTFB: 24ms)
  Certificate: Valid, expires in 15 days ⚠ (renew soon)

Target: Internal Service (http://192.168.1.100:8080/ping)
  Status: 200 OK
  Total: 12ms (Connect: 8ms, TTFB: 4ms)
```

### `vigil certs`

```
TLS Certificate Status
═══════════════════════════════════════════════════════════

┌──────────────────────────────┬─────────────┬──────────────────┐
│ Endpoint                     │ Expires In  │ Status           │
├──────────────────────────────┼─────────────┼──────────────────┤
│ google.com                   │ 62 days     │ ✓ Valid          │
│ api.example.com              │ 15 days     │ ⚠ Renew Soon     │
│ old-service.example.com      │ 3 days      │ ✗ Critical       │
└──────────────────────────────┴─────────────┴──────────────────┘

1 certificate requires immediate attention!
```

## Integration with State Machine

HTTP failures can contribute to overall connectivity state:

```rust
enum TargetType {
    Icmp(IcmpTarget),
    Http(HttpTarget),
}

// HTTP failures weighted same as ICMP for state machine
impl ConnectivityTracker {
    pub fn process_http(&mut self, result: HttpResult) -> StateEvent {
        // Convert HTTP result to generic connectivity result
        let connectivity_result = ConnectivityResult {
            target: result.target,
            success: result.success,
            latency_ms: result.total_ms,
        };

        self.process(connectivity_result)
    }
}
```

## Integration with Notifications (Feature 007)

Certificate expiration warnings:

```rust
if cert.days_until_expiry <= config.warning_days {
    notifier.notify(&NotificationEvent::CertificateExpiring {
        target: target.clone(),
        expires_in_days: cert.days_until_expiry,
    }).await?;
}
```

## Tasks

- [ ] Define `HttpTarget` and `HttpResult` structs
- [ ] Implement HTTP checking with reqwest
- [ ] Implement timing breakdown measurement
- [ ] Implement certificate expiration checking
- [ ] Add database tables for HTTP logs
- [ ] Add configuration parsing for HTTP targets
- [ ] Implement `vigil http` command
- [ ] Implement `vigil certs` command
- [ ] Integrate HTTP results with state machine
- [ ] Add certificate expiration notifications
- [ ] Add unit tests with mock HTTP server
- [ ] Add integration tests with real endpoints

## Dependencies

```toml
reqwest = { version = "0.11", features = ["json", "rustls-tls"] }
url = "2"
```

## Test Plan

### Unit Tests

```rust
#[test]
fn test_classify_http_error() {
    let dns_error = reqwest::Error::from(...);
    assert_eq!(classify_error(&dns_error), HttpPhase::DnsResolution);
}

#[test]
fn test_response_validation() {
    let target = HttpTarget {
        expected_status: Some(200),
        expected_body_contains: Some("ok".to_string()),
        ..
    };

    assert!(validate_response(&target, 200, "status: ok"));
    assert!(!validate_response(&target, 500, "error"));
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_http_check_google() {
    let monitor = HttpMonitor::new(...);
    let target = HttpTarget {
        url: "https://www.google.com".to_string(),
        ..
    };

    let result = monitor.check(&target).await;
    assert!(result.success);
    assert!(result.total_ms.unwrap() < 5000.0);
}
```

## Error Classification

| Error Type | Phase | User Message |
|------------|-------|--------------|
| DNS lookup failed | DnsResolution | "DNS resolution failed" |
| Connection refused | TcpConnect | "Connection refused" |
| Connection timeout | TcpConnect | "Connection timed out" |
| TLS handshake failed | TlsHandshake | "TLS/SSL error" |
| Certificate expired | TlsHandshake | "Certificate expired" |
| HTTP 4xx | HttpRequest | "Client error (4xx)" |
| HTTP 5xx | HttpRequest | "Server error (5xx)" |
| Body mismatch | ResponseValidation | "Response validation failed" |

## Acceptance Criteria

1. HTTP/HTTPS endpoints can be configured alongside ICMP targets
2. Timing breakdown shows DNS, connect, TLS, TTFB separately
3. Certificate expiration is tracked and warnings issued
4. HTTP failures contribute to overall connectivity state
5. Response status codes and body content can be validated
6. Errors are classified by phase for easier debugging
7. CLI displays HTTP status with timing breakdown
8. Certificate status displayed in dedicated command
