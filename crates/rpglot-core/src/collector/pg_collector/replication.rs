//! Replication status collector.
//!
//! Determines whether the PostgreSQL instance is a primary or standby,
//! collects replay lag (standby) or replica details (primary).
//! Results are cached for 30 seconds (same interval as statements).

use tracing::debug;

use super::PostgresCollector;
use crate::storage::model::{ReplicaInfo, ReplicationStatus};

impl PostgresCollector {
    /// Collects replication status with 30-second caching.
    ///
    /// Returns cached result if less than 30 seconds old.
    /// On error, returns None (graceful degradation).
    pub fn collect_replication_status(&mut self) -> Option<ReplicationStatus> {
        // Check cache freshness
        if let Some(cache_time) = self.replication_cache_time
            && cache_time.elapsed() < self.statements_collect_interval
        {
            return self.replication_cache.clone();
        }

        let client = self.client.as_mut()?;

        // Determine role
        let is_in_recovery = client
            .query_one("SELECT pg_is_in_recovery()", &[])
            .ok()
            .and_then(|row| row.try_get::<_, bool>(0).ok())
            .unwrap_or(false);

        let status = if is_in_recovery {
            // Standby: get replay lag
            let replay_lag_s = client
                .query_one(
                    "SELECT EXTRACT(EPOCH FROM (now() - pg_last_xact_replay_timestamp()))::bigint",
                    &[],
                )
                .ok()
                .and_then(|row| row.try_get::<_, i64>(0).ok());

            ReplicationStatus {
                is_in_recovery: true,
                replay_lag_s,
                connected_replicas: 0,
                replicas: Vec::new(),
            }
        } else {
            // Primary: get connected replicas
            let replicas = client
                .query(
                    "SELECT \
                         coalesce(client_addr::text, '') as client_addr, \
                         coalesce(application_name, '') as application_name, \
                         coalesce(state, '') as state, \
                         coalesce(sync_state, '') as sync_state, \
                         pg_wal_lsn_diff(sent_lsn, replay_lsn)::bigint as replay_lag_bytes \
                     FROM pg_stat_replication",
                    &[],
                )
                .ok()
                .map(|rows| {
                    rows.iter()
                        .map(|row| ReplicaInfo {
                            client_addr: row.get(0),
                            application_name: row.get(1),
                            state: row.get(2),
                            sync_state: row.get(3),
                            replay_lag_bytes: row.try_get::<_, i64>(4).ok(),
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let connected_replicas = replicas.len() as u32;

            ReplicationStatus {
                is_in_recovery: false,
                replay_lag_s: None,
                connected_replicas,
                replicas,
            }
        };

        debug!(
            is_standby = status.is_in_recovery,
            replicas = status.connected_replicas,
            replay_lag_s = ?status.replay_lag_s,
            "replication status collected"
        );

        self.replication_cache = Some(status.clone());
        self.replication_cache_time = Some(std::time::Instant::now());

        Some(status)
    }
}
