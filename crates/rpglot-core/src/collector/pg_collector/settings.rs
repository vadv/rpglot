//! PostgreSQL settings collector.
//!
//! Collects all entries from `pg_settings` view. Cached for 1 hour since
//! settings rarely change at runtime.

use std::time::{Duration, Instant};

use tracing::warn;

use super::PostgresCollector;
use super::format_postgres_error;
use crate::storage::model::PgSettingEntry;

/// Interval between pg_settings re-collection.
const SETTINGS_COLLECT_INTERVAL: Duration = Duration::from_secs(3600);

const SETTINGS_QUERY: &str =
    "SELECT name, setting, COALESCE(unit, '') AS unit FROM pg_settings ORDER BY name";

impl PostgresCollector {
    /// Collects all PostgreSQL settings from `pg_settings` view.
    ///
    /// Results are cached for 1 hour. Returns cached data if fresh.
    pub fn collect_settings(&mut self) -> Vec<PgSettingEntry> {
        if let Some(ref cache_time) = self.settings_cache_time
            && cache_time.elapsed() < SETTINGS_COLLECT_INTERVAL
            && !self.settings_cache.is_empty()
        {
            return self.settings_cache.clone();
        }

        let Some(ref mut client) = self.client else {
            return Vec::new();
        };

        match client.query(SETTINGS_QUERY, &[]) {
            Ok(rows) => {
                let entries: Vec<PgSettingEntry> = rows
                    .iter()
                    .map(|row| PgSettingEntry {
                        name: row.get(0),
                        setting: row.get(1),
                        unit: row.get(2),
                    })
                    .collect();
                self.settings_cache = entries.clone();
                self.settings_cache_time = Some(Instant::now());
                entries
            }
            Err(e) => {
                warn!(error = %format_postgres_error(&e), "failed to collect pg_settings");
                // Return stale cache if available
                self.settings_cache.clone()
            }
        }
    }
}
