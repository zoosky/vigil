# 015 - Version Information Command

**Status:** Done

## Overview

Add a `vigil version` command that displays comprehensive version and build information, following CLI best practices. This helps users report bugs, verify installations, and troubleshoot database compatibility issues.

## Current State

- `vigil --version` shows only the semver version (e.g., "vigil 0.1.0")
- No way to see database schema version
- No git commit or build information

## Objectives

- Display build version (semver from Cargo.toml)
- Display git commit hash and build timestamp
- Display database schema version (expected and actual)
- Show database file location
- Follow CLI best practices (machine-readable option)

## Implementation

### 1. Build-time Information

**File: `build.rs`** (new)

```rust
use std::process::Command;

fn main() {
    // Git commit hash
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok();

    let git_hash = output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);

    // Build timestamp (ISO 8601)
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    println!("cargo:rustc-env=BUILD_TIME={}", now);

    // Rebuild if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
}
```

**File: `Cargo.toml`** (addition)

```toml
[package]
# ... existing ...
build = "build.rs"

[build-dependencies]
chrono = "0.4"
```

### 2. Version Constants

**File: `src/lib.rs`** (update)

```rust
/// Software version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Git commit hash (short)
pub const GIT_HASH: &str = env!("GIT_HASH");

/// Build timestamp (ISO 8601)
pub const BUILD_TIME: &str = env!("BUILD_TIME");

/// Database schema version - increment when adding migrations
pub const DB_SCHEMA_VERSION: u32 = 3;

/// Build information struct
pub struct BuildInfo {
    pub version: &'static str,
    pub git_hash: &'static str,
    pub build_time: &'static str,
    pub db_schema_version: u32,
    pub target: &'static str,
    pub rustc_version: &'static str,
}

impl BuildInfo {
    pub fn current() -> Self {
        Self {
            version: VERSION,
            git_hash: GIT_HASH,
            build_time: BUILD_TIME,
            db_schema_version: DB_SCHEMA_VERSION,
            target: env!("TARGET"),
            rustc_version: env!("RUSTC_VERSION"),
        }
    }
}
```

### 3. CLI Command

**File: `src/main.rs`** (addition)

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Show version and build information
    Version {
        /// Output in JSON format for scripting
        #[arg(long)]
        json: bool,
    },
}
```

**File: `src/cli/version.rs`** (new)

```rust
use crate::config::Environment;
use crate::{BuildInfo, DB_SCHEMA_VERSION};
use serde::Serialize;

#[derive(Serialize)]
struct VersionInfo {
    version: String,
    git_hash: String,
    build_time: String,
    target: String,
    db_schema: DbSchemaInfo,
    paths: PathInfo,
}

#[derive(Serialize)]
struct DbSchemaInfo {
    expected: u32,
    actual: Option<u32>,
    status: String,
}

#[derive(Serialize)]
struct PathInfo {
    config: Option<String>,
    database: Option<String>,
    log: Option<String>,
}

pub fn run(env: &Environment, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let build = BuildInfo::current();

    // Get actual database schema version
    let db_actual = get_db_schema_version(env);
    let db_status = match db_actual {
        Some(v) if v == DB_SCHEMA_VERSION => "up-to-date",
        Some(v) if v < DB_SCHEMA_VERSION => "needs-migration",
        Some(_) => "newer-than-binary",
        None => "not-initialized",
    };

    if json {
        let info = VersionInfo {
            version: build.version.to_string(),
            git_hash: build.git_hash.to_string(),
            build_time: build.build_time.to_string(),
            target: build.target.to_string(),
            db_schema: DbSchemaInfo {
                expected: DB_SCHEMA_VERSION,
                actual: db_actual,
                status: db_status.to_string(),
            },
            paths: PathInfo {
                config: env.config_path().ok().map(|p| p.display().to_string()),
                database: env.database_path().ok().map(|p| p.display().to_string()),
                log: env.log_path().ok().map(|p| p.display().to_string()),
            },
        };
        println!("{}", serde_json::to_string_pretty(&info)?);
    } else {
        println!("Vigil Network Monitor");
        println!("═══════════════════════════════════════════════════════════\n");

        println!("Build Information:");
        println!("  Version:      {}", build.version);
        println!("  Git commit:   {}", build.git_hash);
        println!("  Built:        {}", build.build_time);
        println!("  Target:       {}", build.target);
        println!();

        println!("Database Schema:");
        println!("  Expected:     v{}", DB_SCHEMA_VERSION);
        if let Some(actual) = db_actual {
            println!("  Actual:       v{}", actual);
            println!("  Status:       {}", match db_status {
                "up-to-date" => "✓ Up to date",
                "needs-migration" => "⚠ Needs migration (run any command to upgrade)",
                "newer-than-binary" => "✗ Database newer than binary (upgrade vigil)",
                _ => db_status,
            });
        } else {
            println!("  Actual:       (not initialized)");
            println!("  Status:       Run 'vigil init' to create database");
        }
        println!();

        println!("Paths ({} environment):", env);
        if let Ok(path) = env.config_path() {
            let exists = path.exists();
            println!("  Config:       {} {}", path.display(),
                if exists { "" } else { "(not found)" });
        }
        if let Ok(path) = env.database_path() {
            let exists = path.exists();
            println!("  Database:     {} {}", path.display(),
                if exists { "" } else { "(not found)" });
        }
        if let Ok(path) = env.log_path() {
            let exists = path.exists();
            println!("  Log:          {} {}", path.display(),
                if exists { "" } else { "(not found)" });
        }
    }

    Ok(())
}

fn get_db_schema_version(env: &Environment) -> Option<u32> {
    let db_path = env.database_path().ok()?;
    if !db_path.exists() {
        return None;
    }

    let conn = rusqlite::Connection::open(&db_path).ok()?;
    conn.query_row(
        "SELECT MAX(version) FROM schema_version",
        [],
        |row| row.get(0),
    ).ok()
}
```

### 4. Update Cargo.toml Version

**File: `Cargo.toml`**

Add build script and set meaningful version:

```toml
[package]
name = "vigil"
version = "0.2.0"
edition = "2021"
build = "build.rs"
# ...

[build-dependencies]
chrono = "0.4"
```

## Output Examples

### Human-readable (default)

```
$ vigil version

Vigil Network Monitor
═══════════════════════════════════════════════════════════

Build Information:
  Version:      0.2.0
  Git commit:   a1b2c3d
  Built:        2024-01-15T10:30:00Z
  Target:       aarch64-apple-darwin

Database Schema:
  Expected:     v3
  Actual:       v3
  Status:       ✓ Up to date

Paths (production environment):
  Config:       /Users/user/Library/Application Support/ch.kapptec.vigil/config.toml
  Database:     /Users/user/Library/Application Support/ch.kapptec.vigil/monitor.db
  Log:          /Users/user/Library/Application Support/ch.kapptec.vigil/monitor.log
```

### JSON (for scripting)

```
$ vigil version --json
{
  "version": "0.2.0",
  "git_hash": "a1b2c3d",
  "build_time": "2024-01-15T10:30:00Z",
  "target": "aarch64-apple-darwin",
  "db_schema": {
    "expected": 3,
    "actual": 3,
    "status": "up-to-date"
  },
  "paths": {
    "config": "/Users/user/Library/Application Support/ch.kapptec.vigil/config.toml",
    "database": "/Users/user/Library/Application Support/ch.kapptec.vigil/monitor.db",
    "log": "/Users/user/Library/Application Support/ch.kapptec.vigil/monitor.log"
  }
}
```

### Development environment

```
$ vigil --dev version

Vigil Network Monitor
═══════════════════════════════════════════════════════════

Build Information:
  Version:      0.2.0
  Git commit:   a1b2c3d
  Built:        2024-01-15T10:30:00Z
  Target:       aarch64-apple-darwin

Database Schema:
  Expected:     v3
  Actual:       v2
  Status:       ⚠ Needs migration (run any command to upgrade)

Paths (development environment):
  Config:       /Users/user/Library/Application Support/ch.kapptec.vigil/dev/config.toml
  Database:     /Users/user/Library/Application Support/ch.kapptec.vigil/dev/monitor.db
  Log:          /Users/user/Library/Application Support/ch.kapptec.vigil/dev/monitor.log
```

## Tasks

- [x] Create `build.rs` with git hash and build time
- [x] Add build dependencies to Cargo.toml
- [x] Update lib.rs with GIT_HASH, BUILD_TIME constants
- [x] Update DB_SCHEMA_VERSION to 3
- [x] Add `Version` command to CLI
- [x] Create `src/cli/version.rs` module
- [x] Add JSON output support
- [x] Update `--version` flag to include git hash
- [x] Add tests for version info
- [x] Update documentation

## Best Practices Followed

1. **Semantic Versioning**: Use semver for version numbers
2. **Build Reproducibility**: Include git hash for exact source identification
3. **Machine-Readable Output**: JSON option for scripting/automation
4. **Database Compatibility**: Show schema version mismatch warnings
5. **Path Visibility**: Show file locations for debugging
6. **Environment Awareness**: Respect --dev flag in output

## Acceptance Criteria

1. `vigil version` shows comprehensive build and database info
2. `vigil version --json` outputs valid JSON
3. `vigil --version` shows version with git hash (e.g., "vigil 0.2.0 (a1b2c3d)")
4. Database schema mismatch is clearly indicated
5. File paths shown with existence status
6. Works correctly in all environments (production, dev, test)

## Dependencies

No new runtime dependencies. Build dependency:
- `chrono` (already a runtime dependency)

## Future Considerations

- Add `vigil doctor` command for full system health check
- Include OS version in build info
- Check for available updates (requires network)
