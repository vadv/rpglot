//! pg_store_plans collection with caching.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tracing::debug;

use crate::storage::interner::StringInterner;
use crate::storage::model::PgStorePlansInfo;

use super::PostgresCollector;
use super::queries::{StorePlansFork, build_store_plans_query};

pub(super) const STORE_PLANS_EXT_CHECK_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub(super) const STORE_PLANS_COLLECT_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub(super) const MAX_CACHED_STORE_PLANS: usize = 1000;

pub(super) fn store_plans_collect_due(last_collect: Option<Instant>, now: Instant) -> bool {
    match last_collect {
        Some(last) => now.duration_since(last) >= STORE_PLANS_COLLECT_INTERVAL,
        None => true,
    }
}

pub(super) fn store_plans_ext_check_due(last_check: Option<Instant>, now: Instant) -> bool {
    match last_check {
        Some(last) => now.duration_since(last) >= STORE_PLANS_EXT_CHECK_INTERVAL,
        None => true,
    }
}

#[derive(Clone)]
pub(crate) struct PgStorePlansCacheEntry {
    pub info: PgStorePlansInfo,
    pub plan_text: String,
    pub datname: String,
    pub usename: String,
}

impl PostgresCollector {
    pub(super) fn store_plans_extension_available(&mut self) -> bool {
        let now = Instant::now();
        if !store_plans_ext_check_due(self.store_plans_last_check, now) {
            return self.store_plans_ext_version.is_some();
        }

        self.store_plans_last_check = Some(now);

        let ext_query = "SELECT extversion FROM pg_extension WHERE extname = 'pg_store_plans'";

        // Check main client first.
        if let Some(ref mut client) = self.client {
            match client.query_opt(ext_query, &[]) {
                Ok(Some(row)) => {
                    let v: String = row.get(0);
                    // Detect fork: check for vadv-specific column.
                    let fork = detect_fork(client);
                    self.store_plans_fork = Some(fork);
                    self.store_plans_ext_version = Some(v);
                    self.store_plans_client_idx = None;
                    return true;
                }
                Ok(None) => {}
                Err(e) => {
                    let msg = super::format_postgres_error(&e);
                    self.last_error = Some(msg);
                }
            }
        }

        // Search through per-database clients.
        for (idx, db_client) in self.db_clients.iter_mut().enumerate() {
            match db_client.client.query_opt(ext_query, &[]) {
                Ok(Some(row)) => {
                    let v: String = row.get(0);
                    let fork = detect_fork(&mut db_client.client);
                    debug!(
                        database = %db_client.datname,
                        version = %v,
                        fork = ?fork,
                        "pg_store_plans found via per-database client"
                    );
                    self.store_plans_fork = Some(fork);
                    self.store_plans_ext_version = Some(v);
                    self.store_plans_client_idx = Some(idx);
                    return true;
                }
                Ok(None) => {}
                Err(_) => {}
            }
        }

        self.store_plans_ext_version = None;
        self.store_plans_client_idx = None;
        self.store_plans_fork = None;
        false
    }

    /// Collects pg_store_plans data.
    ///
    /// Requires `pg_store_plans` extension to be installed. Extension presence is checked
    /// at most once per 5 minutes.
    pub fn collect_store_plans(&mut self, interner: &mut StringInterner) -> Vec<PgStorePlansInfo> {
        let now = Instant::now();
        if !store_plans_collect_due(self.store_plans_cache_time, now) {
            return self.return_pgp_cached(interner);
        }

        self.store_plans_cache_time = Some(now);

        if let Err(e) = self.ensure_connected() {
            self.last_error = Some(e.to_string());
            return self.return_pgp_cached(interner);
        }

        if !self.store_plans_extension_available() {
            self.store_plans_cache.clear();
            return Vec::new();
        }

        let using_db_client = if let Some(idx) = self.store_plans_client_idx {
            if idx >= self.db_clients.len() {
                self.store_plans_client_idx = None;
                self.store_plans_ext_version = None;
                self.store_plans_last_check = None;
                self.store_plans_fork = None;
                return self.return_pgp_cached(interner);
            }
            true
        } else {
            false
        };

        let fork = self.store_plans_fork.unwrap_or(StorePlansFork::OsscDb);
        let query = build_store_plans_query(fork);

        let result = if using_db_client {
            let idx = self.store_plans_client_idx.unwrap();
            self.db_clients[idx].client.query(&query, &[])
        } else {
            self.client.as_mut().unwrap().query(&query, &[])
        };

        match result {
            Ok(rows) => {
                self.last_error = None;

                let mut entries = Vec::with_capacity(rows.len());
                let mut out = Vec::with_capacity(rows.len());
                let mut plan_lens_sample: Vec<usize> = Vec::new();
                for row in rows {
                    let plan_text: String = row.get("plan");
                    if plan_lens_sample.len() < 5 {
                        plan_lens_sample.push(plan_text.len());
                    }
                    let datname: String = row.get("datname");
                    let usename: String = row.get("usename");
                    let collected_at = SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let info = PgStorePlansInfo {
                        stmt_queryid: row.get("stmt_queryid"),
                        planid: row.get("planid"),
                        plan_hash: interner.intern(&plan_text),
                        userid: row.get("userid"),
                        dbid: row.get("dbid"),
                        datname_hash: interner.intern(&datname),
                        usename_hash: interner.intern(&usename),
                        calls: row.get("calls"),
                        total_time: row.get("total_time"),
                        mean_time: row.get("mean_time"),
                        min_time: row.get("min_time"),
                        max_time: row.get("max_time"),
                        stddev_time: row.get("stddev_time"),
                        rows: row.get("rows"),
                        shared_blks_hit: row.get("shared_blks_hit"),
                        shared_blks_read: row.get("shared_blks_read"),
                        shared_blks_dirtied: row.get("shared_blks_dirtied"),
                        shared_blks_written: row.get("shared_blks_written"),
                        local_blks_read: row.get("local_blks_read"),
                        local_blks_written: row.get("local_blks_written"),
                        temp_blks_read: row.get("temp_blks_read"),
                        temp_blks_written: row.get("temp_blks_written"),
                        blk_read_time: row.get("blk_read_time"),
                        blk_write_time: row.get("blk_write_time"),
                        first_call: row.get("first_call"),
                        last_call: row.get("last_call"),
                        collected_at,
                    };
                    entries.push(PgStorePlansCacheEntry {
                        info: info.clone(),
                        plan_text,
                        datname,
                        usename,
                    });
                    out.push(info);
                }

                debug!(
                    rows = out.len(),
                    plan_text_lens = ?plan_lens_sample,
                    "pg_store_plans collected"
                );

                if entries.len() > MAX_CACHED_STORE_PLANS {
                    entries.truncate(MAX_CACHED_STORE_PLANS);
                    out.truncate(MAX_CACHED_STORE_PLANS);
                }

                self.store_plans_cache = entries;
                self.pgp_filtered_cache = out.clone();

                out
            }
            Err(e) => {
                let msg = super::format_postgres_error(&e);
                self.last_error = Some(msg);

                if using_db_client {
                    self.store_plans_client_idx = None;
                    self.store_plans_ext_version = None;
                    self.store_plans_last_check = None;
                    self.store_plans_fork = None;
                } else {
                    self.client = None;
                    self.server_version_num = None;
                    self.store_plans_ext_version = None;
                    self.store_plans_last_check = None;
                    self.store_plans_fork = None;
                }

                self.return_pgp_cached(interner)
            }
        }
    }

    fn return_pgp_cached(&self, interner: &mut StringInterner) -> Vec<PgStorePlansInfo> {
        self.store_plans_cache
            .iter()
            .map(|entry| {
                let mut out = entry.info.clone();
                out.plan_hash = interner.intern(&entry.plan_text);
                out.datname_hash = interner.intern(&entry.datname);
                out.usename_hash = interner.intern(&entry.usename);
                out
            })
            .collect()
    }
}

/// Detects the pg_store_plans fork by checking for vadv-specific column.
fn detect_fork(client: &mut postgres::Client) -> StorePlansFork {
    let check_query = "SELECT 1 FROM information_schema.columns \
                        WHERE table_name = 'pg_store_plans' \
                          AND column_name = 'queryid_stat_statements'";
    match client.query_opt(check_query, &[]) {
        Ok(Some(_)) => StorePlansFork::Vadv,
        _ => StorePlansFork::OsscDb,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_plans_ext_check_due_respects_interval() {
        let now = Instant::now();
        assert!(store_plans_ext_check_due(None, now));

        let recent = now - Duration::from_secs(10);
        assert!(!store_plans_ext_check_due(Some(recent), now));

        let old = now - STORE_PLANS_EXT_CHECK_INTERVAL;
        assert!(store_plans_ext_check_due(Some(old), now));
    }

    #[test]
    fn store_plans_collect_due_respects_interval() {
        let now = Instant::now();
        assert!(store_plans_collect_due(None, now));

        let recent = now - Duration::from_secs(10);
        assert!(!store_plans_collect_due(Some(recent), now));

        let old = now - STORE_PLANS_COLLECT_INTERVAL;
        assert!(store_plans_collect_due(Some(old), now));
    }
}
