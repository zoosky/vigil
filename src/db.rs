use crate::models::{Outage, PingResult, Stats, TracerouteResult};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("Database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Failed to create database directory: {0}")]
    CreateDir(#[from] std::io::Error),
    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open or create a database at the given path
    pub fn open(path: &Path) -> Result<Self, DbError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Create an in-memory database (useful for testing)
    #[allow(dead_code)]
    pub fn in_memory() -> Result<Self, DbError> {
        let conn = Connection::open_in_memory()?;
        let db = Database { conn };
        db.init_schema()?;
        Ok(db)
    }

    /// Initialize the database schema
    fn init_schema(&self) -> Result<(), DbError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS outages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                start_time TEXT NOT NULL,
                end_time TEXT,
                duration_secs REAL,
                affected_targets TEXT NOT NULL,
                failing_hop INTEGER,
                failing_hop_ip TEXT,
                notes TEXT
            );

            CREATE TABLE IF NOT EXISTS ping_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                target TEXT NOT NULL,
                target_name TEXT NOT NULL,
                latency_ms REAL,
                success INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS traceroutes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                outage_id INTEGER REFERENCES outages(id),
                timestamp TEXT NOT NULL,
                target TEXT NOT NULL,
                hops TEXT NOT NULL,
                success INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_outages_start_time ON outages(start_time);
            CREATE INDEX IF NOT EXISTS idx_ping_log_timestamp ON ping_log(timestamp);
            CREATE INDEX IF NOT EXISTS idx_ping_log_target ON ping_log(target);
            "#,
        )?;
        Ok(())
    }

    /// Insert a new outage (returns the outage ID)
    pub fn insert_outage(&self, outage: &Outage) -> Result<i64, DbError> {
        let affected_targets_json = serde_json::to_string(&outage.affected_targets)?;

        self.conn.execute(
            r#"
            INSERT INTO outages (start_time, end_time, duration_secs, affected_targets, failing_hop, failing_hop_ip, notes)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                outage.start_time.to_rfc3339(),
                outage.end_time.map(|t| t.to_rfc3339()),
                outage.duration_secs,
                affected_targets_json,
                outage.failing_hop,
                outage.failing_hop_ip,
                outage.notes,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Update an existing outage (e.g., when it ends)
    pub fn update_outage(&self, outage: &Outage) -> Result<(), DbError> {
        let affected_targets_json = serde_json::to_string(&outage.affected_targets)?;

        self.conn.execute(
            r#"
            UPDATE outages
            SET end_time = ?2, duration_secs = ?3, affected_targets = ?4, failing_hop = ?5, failing_hop_ip = ?6, notes = ?7
            WHERE id = ?1
            "#,
            params![
                outage.id,
                outage.end_time.map(|t| t.to_rfc3339()),
                outage.duration_secs,
                affected_targets_json,
                outage.failing_hop,
                outage.failing_hop_ip,
                outage.notes,
            ],
        )?;

        Ok(())
    }

    /// Get the most recent ongoing outage (if any)
    pub fn get_ongoing_outage(&self) -> Result<Option<Outage>, DbError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, start_time, end_time, duration_secs, affected_targets, failing_hop, failing_hop_ip, notes
            FROM outages
            WHERE end_time IS NULL
            ORDER BY start_time DESC
            LIMIT 1
            "#,
        )?;

        let mut rows = stmt.query([])?;

        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_outage(row)?))
        } else {
            Ok(None)
        }
    }

    /// Get outages within a time range
    pub fn get_outages(&self, since: DateTime<Utc>, until: DateTime<Utc>) -> Result<Vec<Outage>, DbError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT id, start_time, end_time, duration_secs, affected_targets, failing_hop, failing_hop_ip, notes
            FROM outages
            WHERE start_time >= ?1 AND start_time <= ?2
            ORDER BY start_time DESC
            "#,
        )?;

        let mut outages = Vec::new();
        let mut rows = stmt.query(params![since.to_rfc3339(), until.to_rfc3339()])?;

        while let Some(row) = rows.next()? {
            outages.push(self.row_to_outage(row)?);
        }

        Ok(outages)
    }

    fn row_to_outage(&self, row: &rusqlite::Row) -> Result<Outage, DbError> {
        let start_time_str: String = row.get(1)?;
        let end_time_str: Option<String> = row.get(2)?;
        let affected_targets_json: String = row.get(4)?;

        Ok(Outage {
            id: Some(row.get(0)?),
            start_time: DateTime::parse_from_rfc3339(&start_time_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            end_time: end_time_str.and_then(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
            }),
            duration_secs: row.get(3)?,
            affected_targets: serde_json::from_str(&affected_targets_json).unwrap_or_default(),
            failing_hop: row.get(5)?,
            failing_hop_ip: row.get(6)?,
            notes: row.get(7)?,
        })
    }

    /// Insert a ping result
    pub fn insert_ping(&self, ping: &PingResult) -> Result<(), DbError> {
        self.conn.execute(
            r#"
            INSERT INTO ping_log (timestamp, target, target_name, latency_ms, success)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                ping.timestamp.to_rfc3339(),
                ping.target,
                ping.target_name,
                ping.latency_ms,
                ping.success as i32,
            ],
        )?;
        Ok(())
    }

    /// Insert a traceroute result
    pub fn insert_traceroute(&self, outage_id: Option<i64>, trace: &TracerouteResult) -> Result<(), DbError> {
        let hops_json = serde_json::to_string(&trace.hops)?;

        self.conn.execute(
            r#"
            INSERT INTO traceroutes (outage_id, timestamp, target, hops, success)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                outage_id,
                trace.timestamp.to_rfc3339(),
                trace.target,
                hops_json,
                trace.success as i32,
            ],
        )?;
        Ok(())
    }

    /// Calculate statistics for a time period
    pub fn get_stats(&self, since: DateTime<Utc>, until: DateTime<Utc>) -> Result<Stats, DbError> {
        let outages = self.get_outages(since, until)?;

        let total_outages = outages.len() as u32;
        let total_downtime_secs: f64 = outages
            .iter()
            .filter_map(|o| o.duration_secs)
            .sum();

        let period_secs = (until - since).num_seconds() as f64;
        let availability_percent = if period_secs > 0.0 {
            ((period_secs - total_downtime_secs) / period_secs) * 100.0
        } else {
            100.0
        };

        let avg_outage_duration_secs = if total_outages > 0 {
            Some(total_downtime_secs / total_outages as f64)
        } else {
            None
        };

        // Find most common failing hop
        let mut hop_counts: std::collections::HashMap<u8, u32> = std::collections::HashMap::new();
        for outage in &outages {
            if let Some(hop) = outage.failing_hop {
                *hop_counts.entry(hop).or_insert(0) += 1;
            }
        }
        let most_common_failing_hop = hop_counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(hop, _)| hop);

        Ok(Stats {
            period_start: since,
            period_end: until,
            total_outages,
            total_downtime_secs,
            availability_percent,
            avg_outage_duration_secs,
            most_common_failing_hop,
        })
    }

    /// Delete old data based on retention policy
    pub fn cleanup(&self, retention_days: u32) -> Result<u64, DbError> {
        let cutoff = Utc::now() - Duration::days(retention_days as i64);
        let cutoff_str = cutoff.to_rfc3339();

        let deleted_pings = self.conn.execute(
            "DELETE FROM ping_log WHERE timestamp < ?1",
            params![cutoff_str],
        )?;

        let deleted_traceroutes = self.conn.execute(
            "DELETE FROM traceroutes WHERE timestamp < ?1",
            params![cutoff_str],
        )?;

        let deleted_outages = self.conn.execute(
            "DELETE FROM outages WHERE start_time < ?1",
            params![cutoff_str],
        )?;

        Ok((deleted_pings + deleted_traceroutes + deleted_outages) as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_database() {
        let db = Database::in_memory().unwrap();
        assert!(db.get_ongoing_outage().unwrap().is_none());
    }

    #[test]
    fn test_insert_and_get_outage() {
        let db = Database::in_memory().unwrap();

        let mut outage = Outage::new(vec!["8.8.8.8".to_string()]);
        let id = db.insert_outage(&outage).unwrap();
        outage.id = Some(id);

        let ongoing = db.get_ongoing_outage().unwrap();
        assert!(ongoing.is_some());
        assert_eq!(ongoing.unwrap().id, Some(id));

        // End the outage
        outage.end();
        db.update_outage(&outage).unwrap();

        let ongoing = db.get_ongoing_outage().unwrap();
        assert!(ongoing.is_none());
    }

    #[test]
    fn test_insert_ping() {
        let db = Database::in_memory().unwrap();

        let ping = PingResult {
            target: "8.8.8.8".to_string(),
            target_name: "Google DNS".to_string(),
            timestamp: Utc::now(),
            success: true,
            latency_ms: Some(15.5),
            error: None,
        };

        db.insert_ping(&ping).unwrap();
    }

    #[test]
    fn test_stats() {
        let db = Database::in_memory().unwrap();

        let stats = db
            .get_stats(Utc::now() - Duration::hours(24), Utc::now())
            .unwrap();

        assert_eq!(stats.total_outages, 0);
        assert_eq!(stats.availability_percent, 100.0);
    }
}
