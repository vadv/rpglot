use crate::analysis::advisor::{Advisor, AdvisorContext, Recommendation};
use crate::analysis::{Incident, Severity, find_block};
use crate::storage::model::{DataBlock, PgStatStatementsInfo};

// ============================================================
// Helpers
// ============================================================

fn find_incident<'a>(incidents: &'a [Incident], rule_id: &str) -> Option<&'a Incident> {
    incidents.iter().find(|i| i.rule_id == rule_id)
}

fn find_any_incident<'a>(incidents: &'a [Incident], rule_ids: &[&str]) -> Option<&'a Incident> {
    incidents
        .iter()
        .find(|i| rule_ids.contains(&i.rule_id.as_str()))
}

fn find_all_incidents<'a>(incidents: &'a [Incident], rule_ids: &[&str]) -> Vec<&'a Incident> {
    incidents
        .iter()
        .filter(|i| rule_ids.contains(&i.rule_id.as_str()))
        .collect()
}

fn worst_severity(incidents: &[&Incident]) -> Severity {
    incidents
        .iter()
        .map(|i| i.severity)
        .max()
        .unwrap_or(Severity::Info)
}

fn format_bytes(bytes: i64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.1} GiB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.0} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.0} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

// ============================================================
// 1. ReplicationLagAdvisor
// ============================================================

pub struct ReplicationLagAdvisor;

impl Advisor for ReplicationLagAdvisor {
    fn id(&self) -> &'static str {
        "replication_lag"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let sync = match find_incident(ctx.incidents, "wait_sync_replica") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![sync];
        let mut desc = String::from(
            "The primary is blocked waiting for synchronous replica acknowledgement.\n\
             \n\
             Diagnose:\n\
             \u{2022} Check replica health: replay lag, disk I/O, network latency to standby\n\
             \u{2022} Look at pg_stat_replication: sent_lsn vs replay_lsn gap\n\
             \n\
             If the replica is consistently slow and downtime is acceptable:\n\
             \u{2022} Temporarily switch to async: SET synchronous_standby_names = ''\n\
             \u{2022} Fix the root cause on the replica, then re-enable sync mode",
        );

        if let Some(cpu) = find_incident(ctx.incidents, "cpu_high") {
            related.push(cpu);
            desc.push_str(
                "\n\nHigh CPU on the primary is also present — this compounds the issue \
                 by slowing WAL generation.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Synchronous replication waits".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 2. LockContentionAdvisor
// ============================================================

pub struct LockContentionAdvisor;

impl Advisor for LockContentionAdvisor {
    fn id(&self) -> &'static str {
        "lock_contention"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let blocked = match find_incident(ctx.incidents, "blocked_sessions") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![blocked];
        let mut desc = String::from(
            "Sessions are waiting on locks held by other transactions.\n\
             \n\
             Immediate action:\n\
             \u{2022} Find the root blocker: PGL tab \u{2192} look at the tree root\n\
             \u{2022} Check if it's idle in transaction (forgot COMMIT?) or a long query\n\
             \u{2022} Terminate if safe: SELECT pg_terminate_backend(<pid>)\n\
             \n\
             Prevention:\n\
             \u{2022} SET lock_timeout = '5s' (or per-session) to bound wait time\n\
             \u{2022} Keep transactions as short as possible — do heavy computation outside the TX",
        );

        if let Some(long_q) = find_incident(ctx.incidents, "long_query") {
            related.push(long_q);
            desc.push_str(
                "\n\nLong-running queries detected — likely holding the blocking locks. \
                 Target the longest-held transaction first.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Lock contention — sessions blocked".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 3. HighCpuAdvisor
// ============================================================

pub struct HighCpuAdvisor;

impl Advisor for HighCpuAdvisor {
    fn id(&self) -> &'static str {
        "high_cpu"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let cpu = match find_incident(ctx.incidents, "cpu_high") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cpu];
        let mut desc = String::from(
            "High CPU utilization.\n\
             \n\
             Find the culprit:\n\
             \u{2022} PGS tab: sort by total_exec_time and mean_exec_time\n\
             \u{2022} Run EXPLAIN (ANALYZE, BUFFERS) on the top queries\n\
             \u{2022} Look for: seq scans on large tables, nested loops with high row counts, \
             hash joins spilling to disk\n\
             \n\
             Common fixes:\n\
             \u{2022} Add missing indexes for filtered/joined columns\n\
             \u{2022} Rewrite queries to reduce rows early (push filters down)\n\
             \u{2022} If many connections compete for CPU — use a connection pooler to limit concurrency",
        );

        if let Some(iow) = find_incident(ctx.incidents, "iowait_high") {
            related.push(iow);
            desc.push_str(
                "\n\nI/O wait is also high — the workload is both CPU- and I/O-bound. \
                 Focus on reducing pages read per query (better indexes, query rewrites) \
                 rather than just adding CPU.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "High CPU usage".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 4. MemoryPressureAdvisor
// ============================================================

pub struct MemoryPressureAdvisor;

impl Advisor for MemoryPressureAdvisor {
    fn id(&self) -> &'static str {
        "memory_pressure"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let mem = match find_incident(ctx.incidents, "memory_low") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![mem];
        let mut desc = String::from(
            "Low available memory.\n\
             \n\
             Check PostgreSQL memory allocation:\n\
             \u{2022} shared_buffers should be ~25% of RAM (not more — the OS needs the rest for page cache)\n\
             \u{2022} work_mem \u{00d7} max_connections = worst-case sort/hash memory — can balloon fast\n\
             \u{2022} maintenance_work_mem affects VACUUM, CREATE INDEX (one-off, but large)\n\
             \n\
             If settings are reasonable, the system needs more RAM or fewer concurrent connections.",
        );

        if let Some(ref s) = ctx.settings
            && let (Some(sb), Some(wm)) = (s.get_bytes("shared_buffers"), s.get_bytes("work_mem"))
        {
            desc.push_str(&format!(
                "\n\nCurrent: shared_buffers={}, work_mem={}.",
                format_bytes(sb),
                format_bytes(wm)
            ));
            if let Some(mc) = s.get_i64("max_connections") {
                desc.push_str(&format!(
                    " Worst-case work_mem total: {} (work_mem \u{00d7} {mc}).",
                    format_bytes(wm * mc)
                ));
            }
        }

        if let Some(swap) = find_incident(ctx.incidents, "swap_usage") {
            related.push(swap);
            desc.push_str(
                "\n\n\u{26a0} Swap usage detected — PostgreSQL performance degrades \
                 catastrophically under swap. Urgent: free memory now (reduce connections, \
                 lower work_mem) or add RAM.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Memory pressure".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 5. TableBloatAdvisor
// ============================================================

pub struct TableBloatAdvisor;

impl Advisor for TableBloatAdvisor {
    fn id(&self) -> &'static str {
        "table_bloat"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let dead = match find_incident(ctx.incidents, "dead_tuples_high") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let related = vec![dead];
        let severity = worst_severity(&related);

        let mut desc = String::from(
            "High dead tuple ratio — tables are bloated.\n\
             \n\
             Reclaim space (online, no locks):\n\
             \u{2022} pg_repack — rewrites the table in the background, needs free disk space ~1\u{00d7} table size\n\
             \u{2022} pgcompacttable — moves tuples in-place, slower but needs no extra space\n\
             \u{2022} Do NOT use VACUUM FULL — it takes AccessExclusiveLock and blocks all queries\n\
             \n\
             Prevent future bloat — tune autovacuum:\n\
             \u{2022} autovacuum_vacuum_scale_factor = 0.02\u{2013}0.05 (default 0.2 is too lazy for large tables)\n\
             \u{2022} autovacuum_vacuum_cost_limit = 1000\u{2013}2000 (default 200 is very slow)\n\
             \u{2022} autovacuum_vacuum_cost_delay = 2ms (PG 12+, default 20ms is conservative)\n\
             \u{2022} For large tables: set per-table thresholds via ALTER TABLE ... SET (autovacuum_vacuum_threshold = ...)",
        );

        if let Some(ref s) = ctx.settings
            && let Some(sf) = s.get_f64("autovacuum_vacuum_scale_factor")
        {
            desc.push_str(&format!(
                "\n\nCurrent autovacuum_vacuum_scale_factor = {sf}."
            ));
            if sf >= 0.1 {
                desc.push_str(" This is high for large tables — lower it.");
            }
        }

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Table bloat — dead tuples accumulating".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 6. CheckpointStormAdvisor
// ============================================================

pub struct CheckpointStormAdvisor;

impl Advisor for CheckpointStormAdvisor {
    fn id(&self) -> &'static str {
        "checkpoint_storm"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let cp = match find_incident(ctx.incidents, "checkpoint_spike") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cp];
        let mut desc = String::from(
            "Checkpoint activity spike — forced checkpoints are generating heavy disk I/O.\n\
             \n\
             Spread checkpoint I/O over longer intervals:\n\
             \u{2022} max_wal_size = 4\u{2013}8 GB (reduces forced checkpoint frequency)\n\
             \u{2022} checkpoint_timeout = 15\u{2013}30 min (default 5 min is too frequent for write-heavy loads)\n\
             \u{2022} checkpoint_completion_target = 0.9 (spread writes across the interval)",
        );

        if let Some(ref s) = ctx.settings
            && let (Some(mws), Some(ct)) =
                (s.get_bytes("max_wal_size"), s.get_ms("checkpoint_timeout"))
        {
            desc.push_str(&format!(
                "\n\nCurrent: max_wal_size={}, checkpoint_timeout={}s.",
                format_bytes(mws),
                ct / 1000
            ));
        }

        if let Some(backend) = find_incident(ctx.incidents, "backend_buffers_high") {
            related.push(backend);
            desc.push_str(
                "\n\nBackend processes are also writing dirty buffers directly (bypassing \
                 bgwriter) — this means bgwriter can't keep up. Tune:\n\
                 \u{2022} bgwriter_lru_maxpages = 1000 (default 100)\n\
                 \u{2022} bgwriter_delay = 20ms (default 200ms)",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Checkpoint storm — forced checkpoints".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 7. IoBottleneckAdvisor
// ============================================================

pub struct IoBottleneckAdvisor;

impl Advisor for IoBottleneckAdvisor {
    fn id(&self) -> &'static str {
        "io_bottleneck"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        // Trigger on disk_util_high OR disk_latency_high
        let disk_util = find_incident(ctx.incidents, "disk_util_high");
        let disk_latency = find_incident(ctx.incidents, "disk_latency_high");
        if disk_util.is_none() && disk_latency.is_none() {
            return Vec::new();
        }

        let mut related: Vec<&Incident> = Vec::new();
        let mut desc = String::new();

        if disk_util.is_some() && disk_latency.is_some() {
            desc.push_str("Disk I/O saturated: high utilization + high latency (r_await/w_await).");
            if let Some(u) = disk_util {
                related.push(u);
            }
            if let Some(l) = disk_latency {
                related.push(l);
            }
        } else if let Some(util) = disk_util {
            related.push(util);
            desc.push_str("High disk utilization detected.");
        } else if let Some(latency) = disk_latency {
            related.push(latency);
            desc.push_str(
                "High disk latency (r_await/w_await) — storage is struggling with I/O demands.",
            );
        }

        desc.push_str(
            "\n\n\
             Check PG-level causes first:\n\
             \u{2022} Is shared_buffers too small, causing excessive physical reads? (check PGT HIT%)\n\
             \u{2022} Are seq scans on large tables generating unnecessary I/O? (check PGT SEQ%)\n\
             \u{2022} Is autovacuum or a checkpoint storm generating extra I/O?\n\
             \n\
             If I/O is at hardware limits:\n\
             \u{2022} Migrate to faster storage (NVMe)\n\
             \u{2022} Offload read queries to a replica\n\
             \u{2022} Add RAM — larger page cache = fewer physical reads",
        );

        if let Some(iow) = find_incident(ctx.incidents, "iowait_high") {
            related.push(iow);
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "I/O bottleneck".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 8. ErrorStormAdvisor
// ============================================================

pub struct ErrorStormAdvisor;

impl Advisor for ErrorStormAdvisor {
    fn id(&self) -> &'static str {
        "error_storm"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let error = find_incident(ctx.incidents, "pg_errors");
        let fatal = find_incident(ctx.incidents, "pg_fatal_panic");

        let mut related: Vec<&Incident> = Vec::new();
        if let Some(e) = error {
            related.push(e);
        }
        if let Some(f) = fatal {
            related.push(f);
        }

        if related.is_empty() {
            return Vec::new();
        }

        let severity = worst_severity(&related);

        let desc = if fatal.is_some() {
            String::from(
                "FATAL/PANIC errors in PostgreSQL logs — this is urgent.\n\
                 \n\
                 \u{2022} Check PostgreSQL server logs immediately\n\
                 \u{2022} Common causes: connection exhaustion (max_connections reached), \
                 shared memory issues, data corruption, out of disk space\n\
                 \u{2022} PANIC = PostgreSQL crashed and restarted — check for hardware issues",
            )
        } else {
            String::from(
                "Elevated PostgreSQL error rate.\n\
                 \n\
                 Check PGE tab for error patterns:\n\
                 \u{2022} Connection errors \u{2192} connection pooler misconfiguration or max_connections too low\n\
                 \u{2022} Permission errors \u{2192} recent GRANT/REVOKE changes\n\
                 \u{2022} Constraint violations \u{2192} application logic issue (usually harmless)\n\
                 \u{2022} Lock/serialization errors \u{2192} high contention, may need retry logic in the app",
            )
        };

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: if fatal.is_some() {
                "FATAL/PANIC errors — check logs immediately".to_string()
            } else {
                "Elevated error rate".to_string()
            },
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 9. CgroupThrottleAdvisor
// ============================================================

pub struct CgroupThrottleAdvisor;

impl Advisor for CgroupThrottleAdvisor {
    fn id(&self) -> &'static str {
        "cgroup_throttle"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let throttle = match find_incident(ctx.incidents, "cgroup_throttled") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![throttle];
        let mut desc = String::from(
            "Container is being CPU-throttled by cgroup limits.\n\
             \n\
             Two approaches:\n\
             1. Reduce CPU demand: check PGS tab for expensive queries, add indexes, \
             optimize query plans\n\
             2. Increase the container CPU quota (cpu.max / cpu.cfs_quota_us) if the \
             workload legitimately needs more CPU",
        );

        if let Some(cpu) = find_incident(ctx.incidents, "cpu_high") {
            related.push(cpu);
            desc.push_str(
                "\n\nHigh CPU + throttling confirms the container genuinely needs more \
                 CPU or the workload needs optimization. Start with the queries — \
                 increasing quota without fixing bad queries just delays the problem.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Container CPU throttling".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 10. VacuumBlockedAdvisor
// ============================================================

pub struct VacuumBlockedAdvisor;

impl Advisor for VacuumBlockedAdvisor {
    fn id(&self) -> &'static str {
        "vacuum_blocked"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let dead = match find_incident(ctx.incidents, "dead_tuples_high") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let idle_tx = match find_incident(ctx.incidents, "idle_in_transaction") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![dead, idle_tx];
        let mut desc = String::from(
            "Dead tuples accumulating while idle-in-transaction sessions hold the xmin horizon, \
             preventing VACUUM from reclaiming dead rows. This is the #1 cause of unbounded table bloat.\n\
             \n\
             Fix:\n\
             \u{2022} Find and fix the idle transactions: PGA tab \u{2192} filter idle in transaction\n\
             \u{2022} Set idle_in_transaction_session_timeout = 30s\u{2013}5min to auto-terminate them\n\
             \u{2022} Fix the application: ensure every BEGIN has a matching COMMIT/ROLLBACK",
        );

        if let Some(ref s) = ctx.settings
            && let Some(timeout_ms) = s.get_ms("idle_in_transaction_session_timeout")
        {
            if timeout_ms == 0 {
                desc.push_str(
                    "\n\n\u{26a0} idle_in_transaction_session_timeout = 0 (disabled). \
                     This is dangerous — set it to at least 60s.",
                );
            } else {
                desc.push_str(&format!(
                    "\n\nCurrent idle_in_transaction_session_timeout = {}s.",
                    timeout_ms / 1000
                ));
            }
        }

        if let Some(lq) = find_incident(ctx.incidents, "long_query") {
            related.push(lq);
            desc.push_str(
                "\n\nLong-running queries also hold the xmin horizon. \
                 Check if they can use a read replica instead.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "VACUUM blocked by idle transactions".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 11. LockCascadeAdvisor
// ============================================================

pub struct LockCascadeAdvisor;

impl Advisor for LockCascadeAdvisor {
    fn id(&self) -> &'static str {
        "lock_cascade"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let blocked = match find_incident(ctx.incidents, "blocked_sessions") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let active = match find_incident(ctx.incidents, "high_active_sessions") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![blocked, active];
        let mut desc = String::from(
            "Lock contention is cascading: blocked sessions pile up, appear as \"active\", \
             and create a session storm. One blocker can queue dozens of waiters.\n\
             \n\
             Immediate action:\n\
             \u{2022} PGL tab: find the root of the lock tree, check its query and duration\n\
             \u{2022} Terminate the blocker: SELECT pg_terminate_backend(<pid>)\n\
             \n\
             Prevention:\n\
             \u{2022} SET lock_timeout = '5s'\u{2013}'30s' to prevent unbounded waits\n\
             \u{2022} Use CREATE INDEX CONCURRENTLY instead of CREATE INDEX\n\
             \u{2022} Use pg_repack instead of VACUUM FULL for table maintenance",
        );

        if let Some(ref s) = ctx.settings
            && let Some(lt_ms) = s.get_ms("lock_timeout")
            && lt_ms == 0
        {
            desc.push_str(
                "\n\n\u{26a0} lock_timeout = 0 (disabled) — queries wait for locks \
                 indefinitely. Set it to prevent cascading waits.",
            );
        }

        if let Some(lq) = find_incident(ctx.incidents, "long_query") {
            related.push(lq);
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Lock cascade — session storm".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 12. ConnectionStormAdvisor
// ============================================================

pub struct ConnectionStormAdvisor;

impl Advisor for ConnectionStormAdvisor {
    fn id(&self) -> &'static str {
        "connection_storm"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let active = match find_incident(ctx.incidents, "high_active_sessions") {
            Some(i) => i,
            None => return Vec::new(),
        };

        // Skip if blocked_sessions present — LockCascadeAdvisor covers that case.
        if find_incident(ctx.incidents, "blocked_sessions").is_some() {
            return Vec::new();
        }

        let pressure = find_any_incident(ctx.incidents, &["cpu_high", "wait_lock"]);
        let pressure = match pressure {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![active, pressure];
        let mut desc = String::from(
            "Too many concurrent active queries are saturating resources.\n\
             \n\
             Immediate:\n\
             \u{2022} Check PGA tab: are all sessions running real queries, or is something stuck?\n\
             \u{2022} If the app is opening too many connections: limit at the app/pooler level\n\
             \n\
             Prevention:\n\
             \u{2022} Use a connection pooler (PgBouncer in transaction mode) to limit active backends\n\
             \u{2022} If already using a pooler: lower its pool_size / max_client_conn\n\
             \u{2022} Rule of thumb: active backends \u{2264} 2\u{2013}3\u{00d7} CPU cores",
        );

        if let Some(ref s) = ctx.settings
            && let Some(mc) = s.get_i64("max_connections")
        {
            desc.push_str(&format!("\n\nCurrent max_connections = {mc}."));
        }

        // Add any additional correlated incidents.
        for extra in find_all_incidents(ctx.incidents, &["cpu_high", "wait_lock"]) {
            if !related.iter().any(|r| r.rule_id == extra.rule_id) {
                related.push(extra);
            }
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Connection storm — too many active sessions".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 13. WriteAmplificationAdvisor
// ============================================================

pub struct WriteAmplificationAdvisor;

impl Advisor for WriteAmplificationAdvisor {
    fn id(&self) -> &'static str {
        "write_amplification"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let cp = match find_incident(ctx.incidents, "checkpoint_spike") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let backend = match find_incident(ctx.incidents, "backend_buffers_high") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let disk = match find_any_incident(
            ctx.incidents,
            &["disk_util_high", "disk_io_spike", "disk_latency_high"],
        ) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cp, backend, disk];
        let mut desc = String::from(
            "Write amplification: forced checkpoints + backends writing dirty buffers + \
             disk saturation. The entire write pipeline is overwhelmed.\n\
             \n\
             Tune the write path:\n\
             \u{2022} max_wal_size = 4\u{2013}8 GB (reduces forced checkpoint frequency)\n\
             \u{2022} bgwriter_lru_maxpages = 1000 (proactively flush dirty pages)\n\
             \u{2022} bgwriter_delay = 20ms\n\
             \n\
             Reduce WAL volume:\n\
             \u{2022} Drop unused indexes on write-heavy tables (each index = extra WAL per write)\n\
             \u{2022} Use COPY instead of INSERT for bulk loads\n\
             \u{2022} Consider wal_compression = on (PG 15+: lz4)",
        );

        if let Some(ref s) = ctx.settings {
            let mut settings_info = Vec::new();
            if let Some(v) = s.get_bytes("max_wal_size") {
                settings_info.push(format!("max_wal_size={}", format_bytes(v)));
            }
            if let Some(v) = s.get_i64("bgwriter_lru_maxpages") {
                settings_info.push(format!("bgwriter_lru_maxpages={v}"));
            }
            if let Some(v) = s.get_ms("bgwriter_delay") {
                settings_info.push(format!("bgwriter_delay={v}ms"));
            }
            if !settings_info.is_empty() {
                desc.push_str(&format!("\n\nCurrent: {}.", settings_info.join(", ")));
            }
        }

        // Collect extra disk incidents.
        for extra in find_all_incidents(
            ctx.incidents,
            &["disk_util_high", "disk_io_spike", "disk_latency_high"],
        ) {
            if !related.iter().any(|r| r.rule_id == extra.rule_id) {
                related.push(extra);
            }
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Write amplification storm".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 14. CacheMissAdvisor
// ============================================================

pub struct CacheMissAdvisor;

impl Advisor for CacheMissAdvisor {
    fn id(&self) -> &'static str {
        "cache_miss_io"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let cache =
            match find_any_incident(ctx.incidents, &["cache_hit_ratio_drop", "index_cache_miss"]) {
                Some(i) => i,
                None => return Vec::new(),
            };
        let io = match find_any_incident(
            ctx.incidents,
            &["disk_util_high", "iowait_high", "disk_latency_high"],
        ) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cache, io];
        let mut desc = String::from(
            "Low buffer cache hit ratio is causing excessive physical disk reads.\n\
             \n\
             The working set likely exceeds shared_buffers:\n\
             \u{2022} shared_buffers should be ~25% of RAM\n\
             \u{2022} Check PGT Reads view: which tables have low HIT%?\n\
             \u{2022} Tables with HIT% < 90% and high Disk Read/s are the problem\n\
             \n\
             Note: effective_cache_size is only a planner hint (not a memory allocation) \
             — set it to ~75% of total RAM so the planner knows about OS page cache.",
        );

        if let Some(ref s) = ctx.settings {
            let mut info = Vec::new();
            if let Some(v) = s.get_bytes("shared_buffers") {
                info.push(format!("shared_buffers={}", format_bytes(v)));
            }
            if let Some(v) = s.get_bytes("effective_cache_size") {
                info.push(format!("effective_cache_size={}", format_bytes(v)));
            }
            if !info.is_empty() {
                desc.push_str(&format!("\n\nCurrent: {}.", info.join(", ")));
            }
        }

        // Add all related cache/io incidents.
        for extra in find_all_incidents(
            ctx.incidents,
            &[
                "cache_hit_ratio_drop",
                "index_cache_miss",
                "disk_util_high",
                "disk_latency_high",
                "iowait_high",
            ],
        ) {
            if !related.iter().any(|r| r.rule_id == extra.rule_id) {
                related.push(extra);
            }
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Cache misses \u{2192} disk I/O pressure".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 15. SeqScanCpuAdvisor
// ============================================================

pub struct SeqScanCpuAdvisor;

impl Advisor for SeqScanCpuAdvisor {
    fn id(&self) -> &'static str {
        "seq_scan_pressure"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let seq = match find_incident(ctx.incidents, "seq_scan_dominant") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let pressure = match find_any_incident(
            ctx.incidents,
            &["cpu_high", "iowait_high", "heap_read_spike"],
        ) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![seq, pressure];
        let mut desc = String::from(
            "Sequential scans on large tables are wasting CPU and I/O.\n\
             \n\
             Find and fix:\n\
             \u{2022} PGT Scans view: tables with high SEQ% and large row count need indexes\n\
             \u{2022} PGS tab: find queries with high total_exec_time, run EXPLAIN (ANALYZE, BUFFERS)\n\
             \u{2022} Create B-tree indexes on columns used in WHERE and JOIN clauses\n\
             \u{2022} For partial scans: consider partial indexes (WHERE condition)",
        );

        if let Some(ref s) = ctx.settings
            && let Some(rpc) = s.get_f64("random_page_cost")
            && rpc >= 4.0
        {
            desc.push_str(&format!(
                "\n\nrandom_page_cost = {rpc} (HDD default). If using SSD, set to 1.1 \
                 — this alone can make the planner prefer index scans.",
            ));
        }

        for extra in find_all_incidents(
            ctx.incidents,
            &["cpu_high", "iowait_high", "heap_read_spike"],
        ) {
            if !related.iter().any(|r| r.rule_id == extra.rule_id) {
                related.push(extra);
            }
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Sequential scans causing resource pressure".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 16. AutovacuumPressureAdvisor
// ============================================================

pub struct AutovacuumPressureAdvisor;

impl Advisor for AutovacuumPressureAdvisor {
    fn id(&self) -> &'static str {
        "autovacuum_pressure"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let av = match find_incident(ctx.incidents, "autovacuum_impact") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let io = match find_any_incident(ctx.incidents, &["disk_util_high", "iowait_high"]) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![av, io];
        let mut desc = String::from(
            "Autovacuum I/O is competing with the normal workload for disk bandwidth.\n\
             \n\
             Throttle autovacuum:\n\
             \u{2022} autovacuum_vacuum_cost_delay = 10\u{2013}20ms (slow down each worker)\n\
             \u{2022} autovacuum_vacuum_cost_limit: lower it to reduce burst I/O per worker\n\
             \u{2022} autovacuum_max_workers: if multiple workers run simultaneously, reduce to 1\u{2013}2\n\
             \n\
             For very large tables: set per-table cost_delay via ALTER TABLE to throttle \
             independently from other tables.",
        );

        if let Some(ref s) = ctx.settings {
            let mut info = Vec::new();
            if let Some(v) = s.get_i64("autovacuum_vacuum_cost_limit") {
                info.push(format!("autovacuum_vacuum_cost_limit={v}"));
            }
            if let Some(v) = s.get_ms("autovacuum_vacuum_cost_delay") {
                info.push(format!("autovacuum_vacuum_cost_delay={v}ms"));
            }
            if let Some(v) = s.get_i64("autovacuum_max_workers") {
                info.push(format!("autovacuum_max_workers={v}"));
            }
            if !info.is_empty() {
                desc.push_str(&format!("\n\nCurrent: {}.", info.join(", ")));
            }
        }

        for extra in find_all_incidents(ctx.incidents, &["disk_util_high", "iowait_high"]) {
            if !related.iter().any(|r| r.rule_id == extra.rule_id) {
                related.push(extra);
            }
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Autovacuum saturating disk I/O".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 17. OomRiskAdvisor
// ============================================================

pub struct OomRiskAdvisor;

impl Advisor for OomRiskAdvisor {
    fn id(&self) -> &'static str {
        "oom_risk"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let mem = match find_incident(ctx.incidents, "memory_low") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let swap = match find_incident(ctx.incidents, "swap_usage") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![mem, swap];
        let has_oom = find_incident(ctx.incidents, "cgroup_oom_kill").is_some();

        let mut desc = if has_oom {
            if let Some(oom) = find_incident(ctx.incidents, "cgroup_oom_kill") {
                related.push(oom);
            }
            String::from(
                "OOM kills have occurred. PostgreSQL processes are being killed by the OS.\n\
                 \n\
                 Urgent — reduce memory footprint:\n\
                 \u{2022} Lower work_mem (each sort/hash uses this much per operation per connection)\n\
                 \u{2022} Lower shared_buffers if over-allocated\n\
                 \u{2022} Reduce max_connections (or use a pooler)\n\
                 \u{2022} Check maintenance_work_mem — a single CREATE INDEX can use this much\n\
                 \n\
                 If settings are already minimal: add RAM or increase container memory limit.",
            )
        } else {
            String::from(
                "Approaching OOM: low memory + active swap usage. PostgreSQL under swap \
                 pressure = severe performance degradation.\n\
                 \n\
                 Reduce memory:\n\
                 \u{2022} Lower work_mem — worst case = work_mem \u{00d7} max_connections \u{00d7} \
                 operations_per_query\n\
                 \u{2022} Check shared_buffers: should be ~25% of RAM, not more\n\
                 \u{2022} Reduce connections if possible\n\
                 \n\
                 If settings are reasonable: add RAM.",
            )
        };

        if let Some(ref s) = ctx.settings {
            let mut info = Vec::new();
            if let Some(v) = s.get_bytes("shared_buffers") {
                info.push(format!("shared_buffers={}", format_bytes(v)));
            }
            if let Some(v) = s.get_bytes("work_mem") {
                info.push(format!("work_mem={}", format_bytes(v)));
            }
            if let Some(mc) = s.get_i64("max_connections") {
                info.push(format!("max_connections={mc}"));
                if let Some(wm) = s.get_bytes("work_mem") {
                    let worst_case = wm * mc;
                    info.push(format!(
                        "worst-case work_mem total={}",
                        format_bytes(worst_case)
                    ));
                }
            }
            if !info.is_empty() {
                desc.push_str(&format!("\n\nCurrent: {}.", info.join(", ")));
            }
        }

        let severity = if has_oom {
            Severity::Critical
        } else {
            worst_severity(&related)
        };

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: if has_oom {
                "OOM kills — memory exhausted".to_string()
            } else {
                "Approaching OOM — swap pressure".to_string()
            },
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 18. QueryRegressionAdvisor
// ============================================================

pub struct QueryRegressionAdvisor;

impl Advisor for QueryRegressionAdvisor {
    fn id(&self) -> &'static str {
        "query_regression"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let stmt = match find_incident(ctx.incidents, "stmt_mean_time_spike") {
            Some(i) => i,
            None => return Vec::new(),
        };

        // Only fire for persistent regressions (6+ snapshots ≈ 1+ minute).
        if stmt.snapshot_count < 6 {
            return Vec::new();
        }

        let has_resource_pressure = find_any_incident(
            ctx.incidents,
            &["cpu_high", "disk_util_high", "iowait_high"],
        )
        .is_some();

        let mut related = vec![stmt];
        let mut desc = if has_resource_pressure {
            for extra in find_all_incidents(
                ctx.incidents,
                &["cpu_high", "disk_util_high", "iowait_high"],
            ) {
                related.push(extra);
            }
            String::from(
                "Query slowdown + resource pressure. The slow query may be both a symptom \
                 (degraded by contention) and a cause (consuming resources).\n\
                 \n\
                 Diagnose:\n\
                 \u{2022} PGS tab: find the query with highest mean_exec_time spike\n\
                 \u{2022} Run EXPLAIN (ANALYZE, BUFFERS) — compare with the expected plan\n\
                 \u{2022} Look for: plan flip (seq scan instead of index scan), buffer bloat, \
                 lock waits in the plan",
            )
        } else {
            String::from(
                "Query slowdown without resource saturation — likely a plan regression.\n\
                 \n\
                 Common causes:\n\
                 \u{2022} Stale statistics after bulk INSERT/UPDATE/DELETE \u{2192} run ANALYZE on affected tables\n\
                 \u{2022} Table grew past a planner threshold \u{2192} plan flipped from index scan to seq scan\n\
                 \u{2022} Schema change (dropped index, new column) \u{2192} check recent DDL\n\
                 \n\
                 Diagnose: EXPLAIN (ANALYZE, BUFFERS) on the slow query, compare with previous plan.",
            )
        };

        if let Some(ref s) = ctx.settings
            && let Some(rpc) = s.get_f64("random_page_cost")
            && rpc >= 4.0
        {
            desc.push_str(&format!(
                "\n\nrandom_page_cost = {rpc} (HDD default). For SSD, set to 1.1 \
                 — may fix the plan regression.",
            ));
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Query plan regression".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 19. TempFileSpillAdvisor
// ============================================================

pub struct TempFileSpillAdvisor;

/// Compute total temp blocks written delta between two snapshots.
/// Returns (delta_written, delta_read) in blocks (each block = 8 KiB).
fn temp_blks_delta(ctx: &AdvisorContext<'_>) -> Option<(i64, i64)> {
    let snap = ctx.snapshot?;
    let prev = ctx.prev_snapshot?;

    let stmts = find_block(snap, |b| match b {
        DataBlock::PgStatStatements(v) => Some(v.as_slice()),
        _ => None,
    })?;
    let prev_stmts: &[PgStatStatementsInfo] = find_block(prev, |b| match b {
        DataBlock::PgStatStatements(v) => Some(v.as_slice()),
        _ => None,
    })?;

    let mut delta_written: i64 = 0;
    let mut delta_read: i64 = 0;

    for s in stmts {
        if let Some(ps) = prev_stmts.iter().find(|p| p.queryid == s.queryid) {
            let dw = s.temp_blks_written.saturating_sub(ps.temp_blks_written);
            let dr = s.temp_blks_read.saturating_sub(ps.temp_blks_read);
            if dw > 0 {
                delta_written += dw;
            }
            if dr > 0 {
                delta_read += dr;
            }
        } else {
            // New statement — count its totals as delta
            delta_written += s.temp_blks_written;
            delta_read += s.temp_blks_read;
        }
    }

    Some((delta_written, delta_read))
}

impl Advisor for TempFileSpillAdvisor {
    fn id(&self) -> &'static str {
        "temp_file_spill"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let stmt = match find_incident(ctx.incidents, "stmt_mean_time_spike") {
            Some(i) => i,
            None => return Vec::new(),
        };
        // Accept disk_io_spike OR disk_latency_high as disk evidence.
        let disk = match find_any_incident(ctx.incidents, &["disk_io_spike", "disk_latency_high"]) {
            Some(i) => i,
            None => return Vec::new(),
        };

        // Skip if checkpoint_spike or blocked_sessions — other advisors cover those.
        if find_incident(ctx.incidents, "checkpoint_spike").is_some()
            || find_incident(ctx.incidents, "blocked_sessions").is_some()
        {
            return Vec::new();
        }

        let related = vec![stmt, disk];

        // Check actual temp_blks data from pg_stat_statements.
        let (confirmed, delta_written, delta_read) = match temp_blks_delta(ctx) {
            Some((dw, dr)) if dw > 0 || dr > 0 => (true, dw, dr),
            Some(_) => {
                // temp_blks did not grow — correlation is false positive, skip.
                return Vec::new();
            }
            // No snapshot data available — fall back to correlation-based advisory.
            None => (false, 0, 0),
        };

        let mut desc = if confirmed {
            let bytes_written = delta_written * 8192;
            let bytes_read = delta_read * 8192;
            format!(
                "Confirmed: PostgreSQL is spilling sort/hash operations to temp files on disk.\n\
                 Temp written: {}, temp read: {} (between snapshots).\n\
                 \n\
                 work_mem is too small for the current queries.",
                format_bytes(bytes_written),
                format_bytes(bytes_read),
            )
        } else {
            String::from(
                "Likely temp file spill: query slowdown coincides with disk I/O spikes.\n\
                 When work_mem is too small, sorts and hash joins spill to disk.",
            )
        };

        desc.push_str(
            "\n\n\
             Fix:\n\
             \u{2022} Increase work_mem — but carefully: work_mem \u{00d7} connections \u{00d7} sorts_per_query = total\n\
             \u{2022} Start with SET work_mem = '64MB' per session for the problem query\n\
             \u{2022} Don't blindly set it globally to 256MB+ — that can cause OOM under load\n\
             \u{2022} Better fix: optimize the query to reduce the sort/hash size",
        );

        if let Some(ref s) = ctx.settings {
            if let Some(wm) = s.get_bytes("work_mem") {
                desc.push_str(&format!("\n\nCurrent work_mem = {}.", format_bytes(wm)));
            }
            if let Some(ltf) = s.get_i64("log_temp_files")
                && ltf < 0
            {
                desc.push_str(" log_temp_files is disabled — set to 0 to log all temp file usage.");
            }
        }

        let severity = if confirmed {
            // Confirmed spill is at least Warning, use worst of related incidents.
            worst_severity(&related).max(Severity::Warning)
        } else {
            worst_severity(&related)
        };

        let title = if confirmed {
            "Temp file spill confirmed — increase work_mem"
        } else {
            "Likely temp file spill — check work_mem"
        };

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: title.to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 20. PlanRegressionAdvisor
// ============================================================

pub struct PlanRegressionAdvisor;

impl Advisor for PlanRegressionAdvisor {
    fn id(&self) -> &'static str {
        "plan_regression"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let plan_inc = match find_incident(ctx.incidents, "plan_regression") {
            Some(i) => i,
            None => return Vec::new(),
        };

        // pg_store_plans updates every ~5 min; require 3+ snapshots for confidence.
        if plan_inc.snapshot_count < 3 {
            return Vec::new();
        }

        let ratio = plan_inc.peak_value;
        let mut related = vec![plan_inc];

        let has_stmt_spike = find_incident(ctx.incidents, "stmt_mean_time_spike").is_some();
        if let Some(spike) = find_incident(ctx.incidents, "stmt_mean_time_spike") {
            related.push(spike);
        }

        let mut desc = if has_stmt_spike {
            format!(
                "Plan flip confirmed via pg_store_plans — same query uses multiple execution \
                 plans with {ratio:.1}x difference in mean_time. The slow plan is actively \
                 degrading performance.\n\
                 \n\
                 Diagnose:\n\
                 \u{2022} PGP tab \u{2192} Regression view: see all affected plans\n\
                 \u{2022} EXPLAIN (ANALYZE, BUFFERS) on the query — confirm plan identity\n\
                 \u{2022} If plan flipped to seq scan: check ANALYZE freshness, missing indexes\n\
                 \u{2022} To fix immediately: pg_hint_plan or plan_cache_mode = force_generic_plan"
            )
        } else {
            format!(
                "pg_store_plans shows multiple plans for the same query with {ratio:.1}x \
                 mean_time difference. The regression has not yet caused overall slowdown, \
                 but the inefficient plan is in use.\n\
                 \n\
                 Diagnose:\n\
                 \u{2022} PGP tab \u{2192} Regression view: see all affected plans\n\
                 \u{2022} EXPLAIN (ANALYZE, BUFFERS) on the query — confirm plan identity\n\
                 \u{2022} If plan flipped to seq scan: check ANALYZE freshness, missing indexes\n\
                 \u{2022} To fix immediately: pg_hint_plan or plan_cache_mode = force_generic_plan"
            )
        };

        if let Some(ref s) = ctx.settings
            && let Some(rpc) = s.get_f64("random_page_cost")
            && rpc >= 4.0
        {
            desc.push_str(&format!(
                "\n\nrandom_page_cost = {rpc} (HDD default). If using SSD, set to 1.1 \
                 — this alone can fix plan regressions caused by seq scan preference."
            ));
        }

        let severity = if has_stmt_spike {
            worst_severity(&related)
        } else {
            Severity::Warning
        };

        let title = if has_stmt_spike {
            format!("Plan regression confirmed — {ratio:.1}x slowdown")
        } else {
            format!("Latent plan regression — {ratio:.1}x difference")
        };

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title,
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// Tests
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::advisor::{AdvisorContext, PgSettings};
    use crate::analysis::{Category, Incident, Severity};
    use crate::storage::model::PgSettingEntry;
    use crate::storage::model::{DataBlock, PgStatStatementsInfo, Snapshot};

    fn make_incident(rule_id: &str, severity: Severity) -> Incident {
        Incident {
            rule_id: rule_id.to_string(),
            category: Category::Cpu,
            severity,
            first_ts: 1000,
            last_ts: 2000,
            merge_key: None,
            peak_ts: 1500,
            peak_value: 90.0,
            title: format!("Test {rule_id}"),
            detail: None,
            snapshot_count: 10,
            entity_id: None,
        }
    }

    fn make_ctx(incidents: &[Incident]) -> AdvisorContext<'_> {
        AdvisorContext {
            incidents,
            settings: None,
            snapshot: None,
            prev_snapshot: None,
        }
    }

    fn make_setting(name: &str, setting: &str, unit: &str) -> PgSettingEntry {
        PgSettingEntry {
            name: name.to_string(),
            setting: setting.to_string(),
            unit: unit.to_string(),
        }
    }

    // --- PgSettings wrapper tests ---

    #[test]
    fn pg_settings_get_bytes_8kb() {
        let entries = vec![make_setting("shared_buffers", "16384", "8kB")];
        let s = PgSettings::new(&entries);
        assert_eq!(s.get_bytes("shared_buffers"), Some(16384 * 8192)); // 128 MiB
    }

    #[test]
    fn pg_settings_get_bytes_kb() {
        let entries = vec![make_setting("work_mem", "4096", "kB")];
        let s = PgSettings::new(&entries);
        assert_eq!(s.get_bytes("work_mem"), Some(4096 * 1024)); // 4 MiB
    }

    #[test]
    fn pg_settings_get_ms_seconds() {
        let entries = vec![make_setting("checkpoint_timeout", "300", "s")];
        let s = PgSettings::new(&entries);
        assert_eq!(s.get_ms("checkpoint_timeout"), Some(300_000));
    }

    #[test]
    fn pg_settings_get_f64() {
        let entries = vec![make_setting("random_page_cost", "4", "")];
        let s = PgSettings::new(&entries);
        assert_eq!(s.get_f64("random_page_cost"), Some(4.0));
    }

    #[test]
    fn pg_settings_missing_key() {
        let entries = vec![make_setting("shared_buffers", "16384", "8kB")];
        let s = PgSettings::new(&entries);
        assert_eq!(s.get("no_such_setting"), None);
        assert_eq!(s.get_bytes("no_such_setting"), None);
    }

    // --- Existing advisors basic tests ---

    #[test]
    fn replication_lag_fires() {
        let incidents = vec![make_incident("wait_sync_replica", Severity::Warning)];
        let recs = ReplicationLagAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "replication_lag");
    }

    #[test]
    fn replication_lag_no_trigger() {
        let incidents = vec![make_incident("cpu_high", Severity::Warning)];
        let recs = ReplicationLagAdvisor.evaluate(&make_ctx(&incidents));
        assert!(recs.is_empty());
    }

    // --- New advisors tests ---

    #[test]
    fn vacuum_blocked_fires() {
        let incidents = vec![
            make_incident("dead_tuples_high", Severity::Warning),
            make_incident("idle_in_transaction", Severity::Warning),
        ];
        let recs = VacuumBlockedAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "vacuum_blocked");
        assert!(
            recs[0]
                .related_incidents
                .contains(&"dead_tuples_high".to_string())
        );
        assert!(
            recs[0]
                .related_incidents
                .contains(&"idle_in_transaction".to_string())
        );
    }

    #[test]
    fn vacuum_blocked_needs_both() {
        let incidents = vec![make_incident("dead_tuples_high", Severity::Warning)];
        assert!(
            VacuumBlockedAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );

        let incidents = vec![make_incident("idle_in_transaction", Severity::Warning)];
        assert!(
            VacuumBlockedAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    #[test]
    fn vacuum_blocked_with_settings() {
        let incidents = vec![
            make_incident("dead_tuples_high", Severity::Warning),
            make_incident("idle_in_transaction", Severity::Warning),
        ];
        let settings = vec![make_setting(
            "idle_in_transaction_session_timeout",
            "0",
            "ms",
        )];
        let ctx = AdvisorContext {
            incidents: &incidents,
            settings: Some(PgSettings::new(&settings)),
            snapshot: None,
            prev_snapshot: None,
        };
        let recs = VacuumBlockedAdvisor.evaluate(&ctx);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].description.contains("disabled"));
    }

    #[test]
    fn lock_cascade_fires() {
        let incidents = vec![
            make_incident("blocked_sessions", Severity::Warning),
            make_incident("high_active_sessions", Severity::Warning),
        ];
        let recs = LockCascadeAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "lock_cascade");
    }

    #[test]
    fn lock_cascade_needs_both() {
        let incidents = vec![make_incident("blocked_sessions", Severity::Warning)];
        assert!(
            LockCascadeAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    #[test]
    fn connection_storm_fires() {
        let incidents = vec![
            make_incident("high_active_sessions", Severity::Warning),
            make_incident("cpu_high", Severity::Warning),
        ];
        let recs = ConnectionStormAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "connection_storm");
    }

    #[test]
    fn connection_storm_skips_when_blocked() {
        let incidents = vec![
            make_incident("high_active_sessions", Severity::Warning),
            make_incident("cpu_high", Severity::Warning),
            make_incident("blocked_sessions", Severity::Warning),
        ];
        assert!(
            ConnectionStormAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    #[test]
    fn write_amplification_fires() {
        let incidents = vec![
            make_incident("checkpoint_spike", Severity::Warning),
            make_incident("backend_buffers_high", Severity::Warning),
            make_incident("disk_util_high", Severity::Warning),
        ];
        let recs = WriteAmplificationAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "write_amplification");
    }

    #[test]
    fn write_amplification_needs_all_three() {
        let incidents = vec![
            make_incident("checkpoint_spike", Severity::Warning),
            make_incident("backend_buffers_high", Severity::Warning),
        ];
        assert!(
            WriteAmplificationAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    #[test]
    fn cache_miss_fires() {
        let incidents = vec![
            make_incident("cache_hit_ratio_drop", Severity::Warning),
            make_incident("disk_util_high", Severity::Warning),
        ];
        let recs = CacheMissAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "cache_miss_io");
    }

    #[test]
    fn seq_scan_cpu_fires() {
        let incidents = vec![
            make_incident("seq_scan_dominant", Severity::Warning),
            make_incident("cpu_high", Severity::Warning),
        ];
        let recs = SeqScanCpuAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "seq_scan_pressure");
    }

    #[test]
    fn autovacuum_pressure_fires() {
        let incidents = vec![
            make_incident("autovacuum_impact", Severity::Warning),
            make_incident("disk_util_high", Severity::Warning),
        ];
        let recs = AutovacuumPressureAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "autovacuum_pressure");
    }

    #[test]
    fn oom_risk_fires() {
        let incidents = vec![
            make_incident("memory_low", Severity::Warning),
            make_incident("swap_usage", Severity::Warning),
        ];
        let recs = OomRiskAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "oom_risk");
    }

    #[test]
    fn oom_risk_critical_with_oom_kill() {
        let incidents = vec![
            make_incident("memory_low", Severity::Warning),
            make_incident("swap_usage", Severity::Warning),
            make_incident("cgroup_oom_kill", Severity::Warning),
        ];
        let recs = OomRiskAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].severity, Severity::Critical);
        assert!(recs[0].title.contains("OOM kills"));
    }

    #[test]
    fn query_regression_fires_persistent() {
        let incidents = vec![make_incident("stmt_mean_time_spike", Severity::Warning)];
        // snapshot_count=10 in make_incident, >= 6 threshold
        let recs = QueryRegressionAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "query_regression");
    }

    #[test]
    fn query_regression_skips_transient() {
        let mut inc = make_incident("stmt_mean_time_spike", Severity::Warning);
        inc.snapshot_count = 3; // < 6
        let incidents = vec![inc];
        assert!(
            QueryRegressionAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    #[test]
    fn temp_file_spill_fires() {
        let incidents = vec![
            make_incident("stmt_mean_time_spike", Severity::Warning),
            make_incident("disk_io_spike", Severity::Warning),
        ];
        let recs = TempFileSpillAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "temp_file_spill");
    }

    #[test]
    fn temp_file_spill_skips_checkpoint() {
        let incidents = vec![
            make_incident("stmt_mean_time_spike", Severity::Warning),
            make_incident("disk_io_spike", Severity::Warning),
            make_incident("checkpoint_spike", Severity::Warning),
        ];
        assert!(
            TempFileSpillAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    #[test]
    fn temp_file_spill_skips_blocked() {
        let incidents = vec![
            make_incident("stmt_mean_time_spike", Severity::Warning),
            make_incident("disk_io_spike", Severity::Warning),
            make_incident("blocked_sessions", Severity::Warning),
        ];
        assert!(
            TempFileSpillAdvisor
                .evaluate(&make_ctx(&incidents))
                .is_empty()
        );
    }

    fn make_stmt(
        queryid: i64,
        temp_blks_written: i64,
        temp_blks_read: i64,
    ) -> PgStatStatementsInfo {
        PgStatStatementsInfo {
            queryid,
            temp_blks_written,
            temp_blks_read,
            ..Default::default()
        }
    }

    fn make_snapshot_with_stmts(stmts: Vec<PgStatStatementsInfo>) -> Snapshot {
        Snapshot {
            timestamp: 1000,
            blocks: vec![DataBlock::PgStatStatements(stmts)],
        }
    }

    #[test]
    fn temp_file_spill_confirmed_with_real_data() {
        let incidents = vec![
            make_incident("stmt_mean_time_spike", Severity::Warning),
            make_incident("disk_io_spike", Severity::Warning),
        ];
        let prev_snap = make_snapshot_with_stmts(vec![make_stmt(1, 0, 0), make_stmt(2, 100, 50)]);
        let snap = make_snapshot_with_stmts(vec![make_stmt(1, 500, 200), make_stmt(2, 300, 150)]);
        let ctx = AdvisorContext {
            incidents: &incidents,
            settings: None,
            snapshot: Some(&snap),
            prev_snapshot: Some(&prev_snap),
        };
        let recs = TempFileSpillAdvisor.evaluate(&ctx);
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "temp_file_spill");
        assert!(recs[0].title.contains("confirmed"));
        assert!(recs[0].description.contains("Confirmed"));
    }

    #[test]
    fn temp_file_spill_false_positive_no_temp_growth() {
        let incidents = vec![
            make_incident("stmt_mean_time_spike", Severity::Warning),
            make_incident("disk_io_spike", Severity::Warning),
        ];
        // Same temp_blks in both snapshots — no growth
        let prev_snap = make_snapshot_with_stmts(vec![make_stmt(1, 100, 50)]);
        let snap = make_snapshot_with_stmts(vec![make_stmt(1, 100, 50)]);
        let ctx = AdvisorContext {
            incidents: &incidents,
            settings: None,
            snapshot: Some(&snap),
            prev_snapshot: Some(&prev_snap),
        };
        let recs = TempFileSpillAdvisor.evaluate(&ctx);
        assert!(recs.is_empty(), "should skip when temp_blks did not grow");
    }

    // --- PlanRegressionAdvisor tests ---

    #[test]
    fn plan_regression_advisor_fires() {
        let mut inc = make_incident("plan_regression", Severity::Warning);
        inc.snapshot_count = 10;
        inc.peak_value = 5.0;
        let incidents = vec![inc];
        let recs = PlanRegressionAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0].id, "plan_regression");
    }

    #[test]
    fn plan_regression_advisor_skips_transient() {
        let mut inc = make_incident("plan_regression", Severity::Warning);
        inc.snapshot_count = 1;
        let incidents = vec![inc];
        let recs = PlanRegressionAdvisor.evaluate(&make_ctx(&incidents));
        assert!(recs.is_empty());
    }

    #[test]
    fn plan_regression_advisor_with_stmt_spike() {
        let mut plan_inc = make_incident("plan_regression", Severity::Warning);
        plan_inc.snapshot_count = 10;
        plan_inc.peak_value = 8.0;
        let incidents = vec![
            plan_inc,
            make_incident("stmt_mean_time_spike", Severity::Warning),
        ];
        let recs = PlanRegressionAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert!(recs[0].title.contains("confirmed"));
        assert!(recs[0].description.contains("confirmed"));
    }

    #[test]
    fn plan_regression_advisor_without_stmt_spike() {
        let mut plan_inc = make_incident("plan_regression", Severity::Warning);
        plan_inc.snapshot_count = 10;
        plan_inc.peak_value = 3.0;
        let incidents = vec![plan_inc];
        let recs = PlanRegressionAdvisor.evaluate(&make_ctx(&incidents));
        assert_eq!(recs.len(), 1);
        assert!(recs[0].title.contains("Latent"));
        assert!(recs[0].description.contains("not yet caused"));
    }
}
