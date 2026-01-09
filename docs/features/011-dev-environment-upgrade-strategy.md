# 011 - Development Environment & Upgrade Strategy

**Status:** Implemented

## Problem Statement

Current development workflow has critical issues:

1. **No environment separation** - Running `cargo run` uses the same config/database as production, risking data corruption or interfering with ongoing monitoring
2. **No version tracking** - Database schema changes require manual migration; no way to detect version mismatch
3. **Risky upgrades** - No strategy for upgrading a running monitor service without data loss
4. **Single data path** - Hardcoded `~/Library/Application Support/ch.kapptec.vigil/` for all environments

**Current state:**

```bash
$ vigil config path
/Users/andreas/Library/Application Support/ch.kapptec.vigil/config.toml

$ vigil stats -p 7d
INFO Database opened at "/Users/andreas/Library/Application Support/ch.kapptec.vigil/monitor.db"
# ^ Development runs hit the same database as production service
```

## Objectives

- Enable isolated development without affecting production monitoring
- Introduce software and database versioning
- Provide safe upgrade path for running services
- Support fast switching between environments
- Automatic database migration with backup

## Implementation

### 1. Software Version Tracking

**File: `src/lib.rs`**

```rust
/// Software version from Cargo.toml
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Database schema version (increment on schema changes)
pub const DB_SCHEMA_VERSION: u32 = 1;

pub struct VersionInfo {
    pub software: String,
    pub db_schema: u32,
    pub db_schema_actual: Option<u32>,
}

impl VersionInfo {
    pub fn current() -> Self {
        Self {
            software: VERSION.to_string(),
            db_schema: DB_SCHEMA_VERSION,
            db_schema_actual: None,
        }
    }

    pub fn needs_migration(&self) -> bool {
        self.db_schema_actual.map(|v| v < self.db_schema).unwrap_or(false)
    }
}
```

**Database metadata table:**

```sql
CREATE TABLE IF NOT EXISTS _meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Initial values
INSERT INTO _meta (key, value) VALUES ('schema_version', '1');
INSERT INTO _meta (key, value) VALUES ('created_at', '2024-01-15T10:00:00Z');
INSERT INTO _meta (key, value) VALUES ('last_migration', '2024-01-15T10:00:00Z');
```

### 2. Environment-Based Data Paths

**File: `src/config.rs`**

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Environment {
    Production,
    Development,
    Test,
}

impl Environment {
    /// Determine environment from vigil_ENV or default
    pub fn from_env() -> Self {
        match std::env::var("vigil_ENV").as_deref() {
            Ok("development") | Ok("dev") => Environment::Development,
            Ok("test") => Environment::Test,
            _ => Environment::Production,
        }
    }

    /// Get data directory for this environment
    pub fn data_dir(&self) -> PathBuf {
        let base = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("ch.kapptec.vigil");

        match self {
            Environment::Production => base,
            Environment::Development => base.join("dev"),
            Environment::Test => base.join("test"),
        }
    }

    pub fn config_path(&self) -> PathBuf {
        self.data_dir().join("config.toml")
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir().join("monitor.db")
    }

    pub fn log_path(&self) -> PathBuf {
        self.data_dir().join("monitor.log")
    }
}
```

**Directory structure:**

```
~/Library/Application Support/ch.kapptec.vigil/
├── config.toml          # Production config
├── monitor.db           # Production database
├── monitor.log          # Production logs
├── dev/
│   ├── config.toml      # Development config
│   ├── monitor.db       # Development database (isolated)
│   └── monitor.log      # Development logs
└── test/
    ├── config.toml      # Test config
    └── monitor.db       # Test database (can be ephemeral)
```

### 3. CLI Environment Support

**File: `src/main.rs`**

Add global flag and environment display:

```rust
#[derive(Parser)]
#[command(name = "vigil")]
#[command(version = VERSION)]
struct Cli {
    /// Environment: production, development, test
    #[arg(long, short = 'e', global = true, env = "vigil_ENV")]
    env: Option<String>,

    /// Use development environment (shorthand for --env=development)
    #[arg(long, global = true)]
    dev: bool,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    fn environment(&self) -> Environment {
        if self.dev {
            return Environment::Development;
        }
        match self.env.as_deref() {
            Some("dev") | Some("development") => Environment::Development,
            Some("test") => Environment::Test,
            _ => Environment::from_env(),
        }
    }
}
```

**Usage:**

```bash
# Production (default)
$ vigil start

# Development via flag
$ vigil --dev start
$ vigil -e dev start

# Development via environment variable
$ vigil_ENV=dev cargo run -- start

# Show current environment
$ vigil --dev config path
Environment: development
Config: /Users/andreas/Library/Application Support/ch.kapptec.vigil/dev/config.toml
Database: /Users/andreas/Library/Application Support/ch.kapptec.vigil/dev/monitor.db
```

### 4. Version Command

**File: `src/cli/version.rs`**

```rust
pub fn run(app: &App, verbose: bool) -> Result<()> {
    println!("vigil {}", VERSION);

    if verbose {
        println!();
        println!("Environment:     {}", app.environment);
        println!("Config:          {}", app.config_path.display());
        println!("Database:        {}", app.db_path.display());
        println!();
        println!("Schema version:  {} (current: {})",
            app.db_schema_version.unwrap_or(0),
            DB_SCHEMA_VERSION
        );

        if app.needs_migration() {
            println!("Status:          Migration required");
        } else {
            println!("Status:          Up to date");
        }
    }

    Ok(())
}
```

**Usage:**

```bash
$ vigil version
vigil 0.1.0

$ vigil version -v
vigil 0.1.0

Environment:     production
Config:          /Users/andreas/Library/Application Support/ch.kapptec.vigil/config.toml
Database:        /Users/andreas/Library/Application Support/ch.kapptec.vigil/monitor.db

Schema version:  1 (current: 1)
Status:          Up to date
```

### 5. Database Migration System

**File: `src/db/migrations.rs`**

```rust
pub struct Migration {
    pub version: u32,
    pub description: &'static str,
    pub up: &'static str,    // SQL to apply
    pub down: &'static str,  // SQL to rollback (optional, for dev)
}

/// All migrations in order
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        description: "Initial schema",
        up: include_str!("migrations/001_initial.sql"),
        down: "",
    },
    Migration {
        version: 2,
        description: "Add degraded_events and traceroute columns",
        up: include_str!("migrations/002_enhanced_culprit.sql"),
        down: include_str!("migrations/002_enhanced_culprit_down.sql"),
    },
];

impl Database {
    /// Check if migration needed
    pub fn needs_migration(&self) -> Result<bool> {
        let current = self.get_schema_version()?;
        Ok(current < DB_SCHEMA_VERSION)
    }

    /// Get current schema version from database
    pub fn get_schema_version(&self) -> Result<u32> {
        let version: Result<u32, _> = self.conn.query_row(
            "SELECT value FROM _meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0)?.parse().map_err(|_| rusqlite::Error::InvalidQuery),
        );
        Ok(version.unwrap_or(0))
    }

    /// Run all pending migrations
    pub fn migrate(&mut self) -> Result<MigrationResult> {
        let current = self.get_schema_version()?;
        let mut applied = Vec::new();

        for migration in MIGRATIONS {
            if migration.version > current {
                info!("Applying migration {}: {}", migration.version, migration.description);
                self.conn.execute_batch(migration.up)?;
                self.set_schema_version(migration.version)?;
                applied.push(migration.version);
            }
        }

        Ok(MigrationResult {
            from_version: current,
            to_version: self.get_schema_version()?,
            applied,
        })
    }
}

pub struct MigrationResult {
    pub from_version: u32,
    pub to_version: u32,
    pub applied: Vec<u32>,
}
```

### 6. Upgrade Command with Backup

**File: `src/cli/upgrade.rs`**

```rust
#[derive(Args)]
pub struct UpgradeArgs {
    /// Skip backup (not recommended)
    #[arg(long)]
    no_backup: bool,

    /// Dry run - show what would be done
    #[arg(long)]
    dry_run: bool,

    /// Force upgrade even if service is running
    #[arg(long)]
    force: bool,
}

pub fn run(app: &mut App, args: &UpgradeArgs) -> Result<()> {
    // Check if service is running
    if !args.force && is_service_running()? {
        return Err(anyhow!(
            "Monitor service is running. Stop it first with:\n\
             $ vigil service stop\n\n\
             Or use --force to upgrade anyway (may cause issues)"
        ));
    }

    // Check if migration needed
    if !app.db.needs_migration()? {
        println!("Database is already up to date (schema version {})", DB_SCHEMA_VERSION);
        return Ok(());
    }

    let current_version = app.db.get_schema_version()?;
    println!("Current schema version: {}", current_version);
    println!("Target schema version:  {}", DB_SCHEMA_VERSION);

    if args.dry_run {
        println!("\nMigrations that would be applied:");
        for m in MIGRATIONS {
            if m.version > current_version {
                println!("  - v{}: {}", m.version, m.description);
            }
        }
        return Ok(());
    }

    // Create backup
    if !args.no_backup {
        let backup_path = create_backup(&app.db_path)?;
        println!("Backup created: {}", backup_path.display());
    }

    // Run migrations
    println!("\nApplying migrations...");
    let result = app.db.migrate()?;

    println!("\nUpgrade complete!");
    println!("  Schema version: {} → {}", result.from_version, result.to_version);
    println!("  Migrations applied: {}", result.applied.len());

    Ok(())
}

fn create_backup(db_path: &Path) -> Result<PathBuf> {
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
    let backup_name = format!("monitor.db.backup_{}", timestamp);
    let backup_path = db_path.parent().unwrap().join(backup_name);

    std::fs::copy(db_path, &backup_path)?;
    Ok(backup_path)
}

fn is_service_running() -> Result<bool> {
    let output = Command::new("launchctl")
        .args(["list", "ch.kapptec.vigil"])
        .output()?;
    Ok(output.status.success())
}
```

**Usage:**

```bash
# Check what would be upgraded
$ vigil upgrade --dry-run
Current schema version: 1
Target schema version:  2

Migrations that would be applied:
  - v2: Add degraded_events and traceroute columns

# Perform upgrade (creates backup automatically)
$ vigil upgrade
Current schema version: 1
Target schema version:  2
Backup created: /Users/andreas/.../monitor.db.backup_20240115_143022

Applying migrations...
Applying migration 2: Add degraded_events and traceroute columns

Upgrade complete!
  Schema version: 1 → 2
  Migrations applied: 1

# Service running protection
$ vigil upgrade
Error: Monitor service is running. Stop it first with:
  $ vigil service stop

Or use --force to upgrade anyway (may cause issues)
```

### 7. Init Command Enhancement

**File: `src/cli/init.rs`**

Update to handle environment and show paths:

```rust
pub fn run(env: Environment, force: bool) -> Result<()> {
    let data_dir = env.data_dir();
    let config_path = env.config_path();
    let db_path = env.database_path();

    println!("Initializing vigil ({})", env);
    println!();

    // Create directory
    if !data_dir.exists() {
        std::fs::create_dir_all(&data_dir)?;
        println!("Created directory: {}", data_dir.display());
    }

    // Create config
    if !config_path.exists() || force {
        create_default_config(&config_path)?;
        println!("Created config:    {}", config_path.display());
    } else {
        println!("Config exists:     {}", config_path.display());
    }

    // Create/migrate database
    let db = Database::open(&db_path)?;
    if db.needs_migration()? {
        db.migrate()?;
        println!("Database migrated: {}", db_path.display());
    } else {
        println!("Database ready:    {}", db_path.display());
    }

    println!();
    println!("Schema version: {}", DB_SCHEMA_VERSION);
    println!();

    if env == Environment::Development {
        println!("Development environment initialized!");
        println!("Run with: cargo run -- --dev <command>");
    }

    Ok(())
}
```

### 8. Development Workflow Helpers

**File: `Makefile` or `justfile`**

```makefile
# Development commands
.PHONY: dev dev-init dev-start dev-status dev-reset

# Initialize development environment
dev-init:
 cargo run -- --dev init

# Start monitoring in development
dev-start:
 cargo run -- --dev start --foreground

# Check development status
dev-status:
 cargo run -- --dev status

# View development stats
dev-stats:
 cargo run -- --dev stats -p 1d

# Reset development database (destructive!)
dev-reset:
 rm -f "$(HOME)/Library/Application Support/ch.kapptec.vigil/dev/monitor.db"
 cargo run -- --dev init

# Run tests with isolated test environment
test:
 vigil_ENV=test cargo test

# Production commands (require confirmation)
prod-upgrade:
 vigil service stop
 vigil upgrade
 vigil service start
```

### 9. Cargo Aliases

**File: `.cargo/config.toml`**

```toml
[alias]
dev = "run -- --dev"
dev-start = "run -- --dev start --foreground"
dev-status = "run -- --dev status"
```

**Usage:**

```bash
cargo dev status        # Same as: cargo run -- --dev status
cargo dev-start         # Same as: cargo run -- --dev start --foreground
```

### 10. Startup Version Check

**File: `src/main.rs`**

Add version check on startup:

```rust
fn main() -> Result<()> {
    let cli = Cli::parse();
    let env = cli.environment();

    // Open database
    let db = Database::open(&env.database_path())?;

    // Version check
    if db.needs_migration()? {
        let current = db.get_schema_version()?;
        eprintln!(
            "Warning: Database schema version {} is older than software version {}",
            current, DB_SCHEMA_VERSION
        );
        eprintln!("Run 'vigil upgrade' to migrate the database.");
        eprintln!();

        // Allow read-only commands, block write commands
        if cli.command.requires_current_schema() {
            return Err(anyhow!("Cannot run this command until database is upgraded"));
        }
    }

    // ... rest of main
}
```

## Tasks

- [ ] Add `_meta` table to database schema
- [ ] Implement `Environment` enum with path resolution
- [ ] Add `--dev` and `--env` CLI flags
- [ ] Create migration system in `src/db/migrations.rs`
- [ ] Implement `upgrade` command with backup
- [ ] Update `init` command for environment support
- [ ] Add `version` command with verbose output
- [ ] Add startup version check with warning
- [ ] Create development helper scripts (Makefile/justfile)
- [ ] Add `.cargo/config.toml` aliases
- [ ] Write migration for existing databases (add `_meta` table)
- [ ] Update documentation with development workflow
- [ ] Add tests for migration system
- [ ] Add tests for environment detection

## Test Plan

### Unit Tests

```rust
#[test]
fn test_environment_detection() {
    std::env::set_var("vigil_ENV", "dev");
    assert_eq!(Environment::from_env(), Environment::Development);

    std::env::set_var("vigil_ENV", "production");
    assert_eq!(Environment::from_env(), Environment::Production);

    std::env::remove_var("vigil_ENV");
    assert_eq!(Environment::from_env(), Environment::Production);
}

#[test]
fn test_environment_paths() {
    let dev = Environment::Development;
    assert!(dev.data_dir().to_string_lossy().contains("/dev"));

    let prod = Environment::Production;
    assert!(!prod.data_dir().to_string_lossy().contains("/dev"));
}

#[test]
fn test_migration_order() {
    for (i, migration) in MIGRATIONS.iter().enumerate() {
        assert_eq!(migration.version, (i + 1) as u32, "Migration version mismatch");
    }
}

#[test]
fn test_migration_applies_in_order() {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut db = Database::open(&db_path).unwrap();
    assert_eq!(db.get_schema_version().unwrap(), 0);

    db.migrate().unwrap();
    assert_eq!(db.get_schema_version().unwrap(), DB_SCHEMA_VERSION);
}
```

### Integration Tests

```rust
#[test]
fn test_dev_environment_isolation() {
    // Initialize both environments
    // Verify they have separate databases
    // Verify operations in one don't affect the other
}

#[test]
fn test_upgrade_creates_backup() {
    // Create database at version 1
    // Run upgrade
    // Verify backup file exists with correct naming
}

#[test]
fn test_upgrade_blocked_when_service_running() {
    // Mock launchctl to report service running
    // Verify upgrade command fails with helpful message
}
```

## Migration for Existing Installations

First migration adds `_meta` table to existing databases:

**File: `src/db/migrations/001_add_meta.sql`**

```sql
-- Add metadata table if not exists
CREATE TABLE IF NOT EXISTS _meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Set initial version (this is migration 1)
INSERT OR REPLACE INTO _meta (key, value) VALUES ('schema_version', '1');
INSERT OR REPLACE INTO _meta (key, value) VALUES ('created_at', datetime('now'));
INSERT OR REPLACE INTO _meta (key, value) VALUES ('last_migration', datetime('now'));
```

## Acceptance Criteria

1. `cargo run -- --dev start` uses isolated dev database
2. `vigil_ENV=dev` works as alternative to `--dev` flag
3. `vigil version -v` shows environment and schema version
4. `vigil upgrade` creates backup before migration
5. `vigil upgrade` fails if service is running (without `--force`)
6. `vigil upgrade --dry-run` shows pending migrations
7. Development database can be reset without affecting production
8. Startup warns if database schema is outdated
9. All existing databases get `_meta` table on first run
10. `cargo dev status` alias works

## Example Workflow

```bash
# One-time: Initialize development environment
$ cargo run -- --dev init
Initializing vigil (development)

Created directory: /Users/andreas/.../ch.kapptec.vigil/dev
Created config:    /Users/andreas/.../ch.kapptec.vigil/dev/config.toml
Database ready:    /Users/andreas/.../ch.kapptec.vigil/dev/monitor.db

Schema version: 1

Development environment initialized!
Run with: cargo run -- --dev <command>

# Daily development
$ cargo dev-start
INFO Database opened at ".../dev/monitor.db"
INFO Starting monitor (development mode)...

# Meanwhile, production keeps running undisturbed
$ vigil status
Status: ONLINE (production)
...

# After schema changes: upgrade production
$ vigil service stop
$ vigil upgrade
Backup created: .../monitor.db.backup_20240115_143022
Applying migration 2: Add degraded_events...
Upgrade complete!

$ vigil service start
```
