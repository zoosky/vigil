# Changelog

All notable changes to Vigil are documented in this file.

## [Unreleased]

### Added
- Feature 018: Diagnostics command for troubleshooting
- Feature 017: Code audit bug fixes and hardening plan

## [0.2.0] - 2026-02-06

### Added
- **TCP/HTTP connectivity monitoring** (Feature 014) — TCP and HTTP methods alongside ICMP ping; TCP is now the default monitoring method to avoid ICMP rate-limiting false positives
- **Gateway-first diagnosis** (Feature 013) — Pings gateway before running traceroute to quickly classify local vs ISP issues
- **Enhanced culprit tracking** (Feature 010) — Degraded events table, periodic traceroutes during outages, traceroute trigger types
- **Development environment** (Feature 011) — Isolated dev/test databases via `--dev` flag or `VIGIL_ENV`
- **Version information** (Feature 015) — `vigil version` with git hash, build time, schema status, JSON output
- **Process timeout** (Feature 016) — Hard subprocess timeout prevents monitor blocking on hung ping/traceroute
- **Database upgrade command** — `vigil upgrade` with backup, dry-run, and schema migration (v1→v3)
- **Adaptive polling** — 4x faster polling during DEGRADED state (500ms default)

### Changed
- Default monitoring method changed from ICMP ping to TCP:443
- Default thresholds tuned for faster detection: degraded=2, offline=3, recovery=3
- Default ping interval changed to 2000ms to reduce router load

### Fixed
- CI pipeline simplified and fixed (Feature 012)

## [0.1.0] - 2025-01-15

### Added
- **Core infrastructure** (Feature 001) — Config management, SQLite database, logging, CLI skeleton
- **Ping monitor** (Feature 002) — Continuous multi-target ICMP ping monitoring via macOS `ping` command
- **Outage detection** (Feature 003) — State machine with ONLINE/DEGRADED/OFFLINE states and hysteresis
- **Hop analysis** (Feature 004) — Traceroute integration for fault isolation on outage detection
- **CLI reporting** (Feature 005) — `vigil status`, `vigil outages`, `vigil stats` commands with table formatting
- **Service management** (Feature 006) — macOS launchd integration, graceful shutdown, log rotation
- Gateway auto-detection via `route -n get default`
- Data retention with configurable cleanup (`vigil cleanup`)
