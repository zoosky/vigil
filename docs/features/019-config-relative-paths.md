# 019 - Fix Config Relative Paths

**Status:** Pending

## Problem Statement

Vigil currently loads its configuration exclusively from the platform-standard
directory (`~/Library/Application Support/ch.kapptec.vigil/config.toml` on
macOS) determined by the `directories` crate. There is no way to point the CLI
at an alternative config file.

More critically, when a user does specify relative paths inside the config
(e.g. `database.path` or `logging.file`), those paths are resolved relative to
the **current working directory** of the process — wherever the binary happens
to be invoked from — rather than relative to the directory containing the
config file itself. This leads to unpredictable behaviour: the same config
produces different results depending on `pwd`.

### Current Behaviour

```
# Config lives at /opt/vigil/config.toml and contains:
#   [database]
#   path = "data/monitor.db"
#   [logging]
#   file = "logs/monitor.log"

$ cd /opt/vigil && vigil start   # db → /opt/vigil/data/monitor.db  ✓
$ cd /tmp && vigil start         # db → /tmp/data/monitor.db        ✗ (wrong)
```

Without a `--config` flag there is no supported way to use a config file
outside the platform-standard location, making it difficult to:

- Run multiple isolated Vigil instances on one machine.
- Store configuration alongside deployment scripts in version control.
- Run in containers where the platform-standard path may not exist.

### User Impact

- Relative paths in config silently resolve to the wrong location when Vigil is
  started from a different directory (e.g. via cron, systemd, or launchd).
- No `--config` flag means no portable, relocatable deployments.

## Solution

### 1. Add `--config` / `-c` Global CLI Flag

Add a new global argument to the `Cli` struct that accepts an explicit path to
a TOML config file.

```rust
#[derive(Parser)]
#[command(name = "vigil")]
struct Cli {
    /// Path to config file (default: platform-standard location)
    #[arg(long = "config", short = 'c', global = true, value_name = "FILE")]
    config_path: Option<PathBuf>,

    // ... existing fields unchanged
}
```

### 2. Resolve Relative Paths Against Config Directory

After loading the config, resolve every `PathBuf` field that is a relative path
against the **parent directory of the config file**, not against `cwd`. Absolute
paths are left untouched.

```rust
impl Config {
    /// Resolve relative paths in the config against `base_dir`.
    /// Absolute paths are left unchanged.
    pub fn resolve_paths(&mut self, base_dir: &Path) {
        if let Some(ref mut p) = self.database.path {
            if p.is_relative() {
                *p = base_dir.join(&p);
            }
        }
        if let Some(ref mut p) = self.logging.file {
            if p.is_relative() {
                *p = base_dir.join(&p);
            }
        }
    }
}
```

### 3. Loading Logic

Introduce a new `Config::load_from` method that accepts an explicit file path,
and adjust the existing loading flow.

```rust
impl Config {
    /// Load config from an explicit file path.
    /// Relative paths inside the config are resolved against the
    /// config file's parent directory.
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let canonical = path.canonicalize().map_err(|e| {
            ConfigError::ReadError(std::io::Error::new(
                e.kind(),
                format!("config file not found: {}", path.display()),
            ))
        })?;
        let content = std::fs::read_to_string(&canonical)?;
        let mut config: Config = toml::from_str(&content)?;

        if let Some(base_dir) = canonical.parent() {
            config.resolve_paths(base_dir);
        }

        Ok(config)
    }

    /// Load config for a given environment.
    /// When no explicit path is provided, uses the platform-standard
    /// location and resolves relative paths against its directory.
    pub fn load_for_env(env: &Environment) -> Result<Self, ConfigError> {
        let config_path = env.config_path()?;
        if config_path.exists() {
            Self::load_from(&config_path)
        } else {
            Ok(Config::default())
        }
    }
}
```

### 4. Integrate Into CLI Dispatch

Update `main()` to thread the optional `--config` path through the loading
pipeline.

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let env = cli.environment();

    // Load config: explicit path takes precedence over environment default
    let config = match cli.config_path {
        Some(ref path) => Config::load_from(path)?,
        None => Config::load_for_env(&env)?,
    };

    // Build App from the loaded config instead of re-loading inside App::with_env
    let app = App::with_config(config, env)?;

    match cli.command {
        // ... unchanged dispatch
    }
}
```

A new `App::with_config` constructor avoids double-loading:

```rust
impl App {
    /// Create App from an already-loaded Config.
    pub fn with_config(
        config: Config,
        env: Environment,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        init_logging_for_env(&config, &env)?;

        let db_path = config.database_path_for_env(&env)?;
        let db = db::Database::open(&db_path)?;

        tracing::info!("Database opened at {:?}", db_path);

        Ok(App {
            config,
            db,
            environment: env,
        })
    }
}
```

### 5. Update Existing `database_path_for_env` / `log_path_for_env`

After `resolve_paths` runs at load time, the helper methods
`database_path_for_env` and `log_path_for_env` already return the corrected
absolute path — no further changes needed in those methods.

## Path Resolution Rules

| Config value              | `--config` given?      | Resolved against           |
|---------------------------|------------------------|----------------------------|
| Absolute (`/var/db/v.db`) | yes or no              | Used as-is                 |
| Relative (`data/v.db`)    | `--config /opt/v/c.toml` | `/opt/v/data/v.db`       |
| Relative (`data/v.db`)    | no (default location)  | `<platform-data-dir>/data/v.db` |
| Not set (None)            | yes or no              | Environment default        |

## Files Changed

```
src/main.rs      # Add --config/-c flag, update dispatch
src/config.rs    # Add resolve_paths(), load_from()
src/lib.rs       # Add App::with_config()
```

## Backward Compatibility

- **No `--config` flag supplied**: behaviour is identical to today; the
  platform-standard location is used. Relative paths now resolve against the
  config directory instead of `cwd`, which is the correct behaviour and fixes
  the existing bug.
- **`--config` flag supplied**: new capability; no previous behaviour to break.
- Users who relied on relative paths resolving against `cwd` (unlikely, since
  this is undocumented and produces inconsistent results) will see paths resolve
  against the config directory instead. This is intentional.

## Test Plan

### Unit Tests

```rust
#[test]
fn test_resolve_paths_relative() {
    let mut config = Config::default();
    config.database.path = Some(PathBuf::from("data/monitor.db"));
    config.logging.file = Some(PathBuf::from("logs/monitor.log"));

    config.resolve_paths(Path::new("/opt/vigil"));

    assert_eq!(
        config.database.path.unwrap(),
        PathBuf::from("/opt/vigil/data/monitor.db")
    );
    assert_eq!(
        config.logging.file.unwrap(),
        PathBuf::from("/opt/vigil/logs/monitor.log")
    );
}

#[test]
fn test_resolve_paths_absolute_unchanged() {
    let mut config = Config::default();
    config.database.path = Some(PathBuf::from("/var/lib/vigil/monitor.db"));

    config.resolve_paths(Path::new("/opt/vigil"));

    assert_eq!(
        config.database.path.unwrap(),
        PathBuf::from("/var/lib/vigil/monitor.db")
    );
}

#[test]
fn test_resolve_paths_none_unchanged() {
    let mut config = Config::default();
    assert!(config.database.path.is_none());

    config.resolve_paths(Path::new("/opt/vigil"));

    assert!(config.database.path.is_none());
}

#[test]
fn test_load_from_nonexistent_file() {
    let result = Config::load_from(Path::new("/nonexistent/config.toml"));
    assert!(result.is_err());
}
```

### Integration Tests

```rust
#[test]
fn test_config_flag_loads_custom_file() {
    // Create a temp config file
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    std::fs::write(&config_path, r#"
        [database]
        path = "mydata/monitor.db"

        [logging]
        level = "debug"
    "#).unwrap();

    let config = Config::load_from(&config_path).unwrap();

    // Relative path resolved against temp dir
    assert_eq!(
        config.database.path.unwrap(),
        dir.path().join("mydata/monitor.db")
    );
    assert_eq!(config.logging.level, "debug");
}
```

### Manual Tests

```bash
# 1. Default behaviour unchanged
vigil config path
vigil start

# 2. Custom config with relative database path
mkdir -p /tmp/vigil-test
cat > /tmp/vigil-test/config.toml <<'EOF'
[database]
path = "data/monitor.db"

[logging]
level = "debug"
file = "logs/monitor.log"
EOF

vigil --config /tmp/vigil-test/config.toml config show
# Verify paths show /tmp/vigil-test/data/monitor.db
# and /tmp/vigil-test/logs/monitor.log

# 3. Absolute paths left alone
cat > /tmp/vigil-test/config.toml <<'EOF'
[database]
path = "/var/lib/vigil/monitor.db"
EOF

vigil --config /tmp/vigil-test/config.toml config show
# Verify path is still /var/lib/vigil/monitor.db

# 4. Running from different cwd produces same result
cd /tmp && vigil --config /tmp/vigil-test/config.toml config path
cd /    && vigil --config /tmp/vigil-test/config.toml config path
# Both should show identical paths
```

## Acceptance Criteria

1. `vigil --config path/to/config.toml <command>` loads the specified config.
2. Relative `database.path` and `logging.file` values resolve against the
   config file's parent directory, not `cwd`.
3. Absolute paths in the config are used as-is.
4. Omitted path fields (`None`) continue to fall back to environment defaults.
5. Without `--config`, behaviour is unchanged (platform-standard location).
6. Error message is clear when the specified config file does not exist.
7. `vigil config path` reflects the resolved paths when `--config` is used.
8. All existing tests continue to pass.

## Future Enhancements

- Support `VIGIL_CONFIG` environment variable as an alternative to `--config`.
- `vigil init --config <path>` to scaffold a config at a custom location.
- Warn at startup when relative paths are detected and `--config` is not set
  (to nudge users toward explicit configuration).

## Dependencies

No new crate dependencies required. Uses only `std::path` and `std::fs`.

## Next Steps

Proceed to implementation after this spec is approved.
