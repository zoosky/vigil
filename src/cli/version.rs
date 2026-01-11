//! Version information command

use crate::config::Environment;
use crate::{BUILD_TARGET, BUILD_TIME, DB_SCHEMA_VERSION, GIT_HASH, VERSION};
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

/// Run the version command
pub fn run(env: &Environment, json: bool) -> Result<(), Box<dyn std::error::Error>> {
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
            version: VERSION.to_string(),
            git_hash: GIT_HASH.to_string(),
            build_time: BUILD_TIME.to_string(),
            target: BUILD_TARGET.to_string(),
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
        println!(
            "===============================================================================\n"
        );

        println!("Build Information:");
        println!("  Version:      {}", VERSION);
        println!("  Git commit:   {}", GIT_HASH);
        println!("  Built:        {}", BUILD_TIME);
        println!("  Target:       {}", BUILD_TARGET);
        println!();

        println!("Database Schema:");
        println!("  Expected:     v{}", DB_SCHEMA_VERSION);
        if let Some(actual) = db_actual {
            println!("  Actual:       v{}", actual);
            let status_display = match db_status {
                "up-to-date" => "Up to date",
                "needs-migration" => "Needs migration (run any command to upgrade)",
                "newer-than-binary" => "Database newer than binary (upgrade vigil)",
                _ => db_status,
            };
            println!("  Status:       {}", status_display);
        } else {
            println!("  Actual:       (not initialized)");
            println!("  Status:       Run 'vigil init' to create database");
        }
        println!();

        println!("Paths ({} environment):", env);
        if let Ok(path) = env.config_path() {
            let exists = if path.exists() { "" } else { " (not found)" };
            println!("  Config:       {}{}", path.display(), exists);
        }
        if let Ok(path) = env.database_path() {
            let exists = if path.exists() { "" } else { " (not found)" };
            println!("  Database:     {}{}", path.display(), exists);
        }
        if let Ok(path) = env.log_path() {
            // Log files use daily rotation, so check for any monitor.log* files
            let log_exists = if let Some(log_dir) = path.parent() {
                log_dir
                    .read_dir()
                    .map(|entries| {
                        entries
                            .filter_map(|e| e.ok())
                            .any(|e| e.file_name().to_string_lossy().starts_with("monitor.log"))
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            let exists = if log_exists { "" } else { " (not found)" };
            println!("  Log:          {}{}", path.display(), exists);
        }
    }

    Ok(())
}

/// Get the current database schema version by reading directly from the database
fn get_db_schema_version(env: &Environment) -> Option<u32> {
    let db_path = env.database_path().ok()?;
    if !db_path.exists() {
        return None;
    }

    let conn = rusqlite::Connection::open(&db_path).ok()?;

    // Check if schema_version table exists
    let table_exists: bool = conn
        .query_row(
            "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false);

    if !table_exists {
        return Some(0); // Pre-migration database
    }

    conn.query_row("SELECT MAX(version) FROM schema_version", [], |row| {
        row.get(0)
    })
    .ok()
}

/// Get a short version string for --version flag
pub fn short_version() -> String {
    format!("{} ({})", VERSION, GIT_HASH)
}
