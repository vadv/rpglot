use crate::analysis::advisor::{Advisor, AdvisorContext, Recommendation};
use crate::analysis::{Incident, Severity};

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
            "Synchronous replication waits detected. Check pg_stat_replication for replica \
             lag and verify synchronous_standby_names configuration. Consider switching to \
             asynchronous replication if latency is acceptable.",
        );

        if let Some(cpu) = find_incident(ctx.incidents, "cpu_high") {
            related.push(cpu);
            desc.push_str(
                " High CPU usage on the primary may indicate the replica is overloaded \
                 and cannot keep up with WAL replay.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Investigate replication lag".to_string(),
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
            "Blocked sessions detected due to lock contention. Identify blocking queries \
             using pg_locks and pg_stat_activity. Review transaction isolation levels and \
             consider adding lock_timeout to prevent indefinite waits.",
        );

        if let Some(long_q) = find_incident(ctx.incidents, "long_query") {
            related.push(long_q);
            desc.push_str(
                " Long-running queries are also present, which may be holding locks \
                 for extended periods. Consider optimizing or breaking up these queries.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Resolve lock contention".to_string(),
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
            "High CPU utilization detected. Check top queries in pg_stat_statements \
             ordered by total_exec_time. Look for missing indexes causing sequential scans \
             and consider query optimization.",
        );

        if let Some(iow) = find_incident(ctx.incidents, "iowait_high") {
            related.push(iow);
            desc.push_str(
                " High I/O wait is also present, indicating the workload may be I/O bound. \
                 Verify storage performance and consider increasing shared_buffers or \
                 effective_cache_size.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Investigate high CPU usage".to_string(),
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
            "Low available memory detected. Consider increasing system RAM or tuning \
             PostgreSQL memory settings: reduce shared_buffers if over-provisioned, \
             lower work_mem for high-concurrency workloads, and review \
             maintenance_work_mem.",
        );

        if let Some(ref s) = ctx.settings
            && let (Some(sb), Some(wm)) = (s.get_bytes("shared_buffers"), s.get_bytes("work_mem"))
        {
            desc.push_str(&format!(
                " Current settings: shared_buffers={}, work_mem={}.",
                format_bytes(sb),
                format_bytes(wm)
            ));
        }

        if let Some(swap) = find_incident(ctx.incidents, "swap_usage") {
            related.push(swap);
            desc.push_str(
                " Swap usage is also detected, which severely degrades database \
                 performance. This is urgent: either increase RAM or reduce PostgreSQL \
                 memory consumption to eliminate swap usage.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Address memory pressure".to_string(),
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
            "High dead tuple ratio detected, indicating table bloat. Run VACUUM FULL \
             on affected tables during a maintenance window to reclaim space. For \
             long-term prevention, adjust autovacuum settings: lower \
             autovacuum_vacuum_scale_factor (e.g. 0.05), increase \
             autovacuum_vacuum_cost_limit, and reduce autovacuum_vacuum_cost_delay.",
        );

        if let Some(ref s) = ctx.settings
            && let Some(sf) = s.get_f64("autovacuum_vacuum_scale_factor")
        {
            desc.push_str(&format!(" Current autovacuum_vacuum_scale_factor={sf}."));
        }

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Reduce table bloat".to_string(),
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
            "Checkpoint activity spike detected. Consider increasing checkpoint_timeout \
             (e.g. 15min) and max_wal_size (e.g. 4GB+) to spread checkpoint I/O over \
             longer intervals and reduce write amplification.",
        );

        if let Some(ref s) = ctx.settings
            && let (Some(mws), Some(ct)) =
                (s.get_bytes("max_wal_size"), s.get_ms("checkpoint_timeout"))
        {
            desc.push_str(&format!(
                " Current settings: max_wal_size={}, checkpoint_timeout={}s.",
                format_bytes(mws),
                ct / 1000
            ));
        }

        if let Some(backend) = find_incident(ctx.incidents, "backend_buffers_high") {
            related.push(backend);
            desc.push_str(
                " High backend buffer writes indicate the background writer cannot keep up. \
                 Tune bgwriter_lru_maxpages and bgwriter_delay to write dirty buffers \
                 more aggressively before checkpoints.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Mitigate checkpoint storms".to_string(),
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
        let disk = match find_incident(ctx.incidents, "disk_util_high") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![disk];
        let mut desc = String::from(
            "High disk utilization detected. Check the storage subsystem for performance \
             bottlenecks. Consider upgrading to faster storage (NVMe), distributing I/O \
             across multiple disks, or reducing write-heavy operations during peak hours.",
        );

        if let Some(iow) = find_incident(ctx.incidents, "iowait_high") {
            related.push(iow);
            desc.push_str(
                " High I/O wait correlates with disk saturation, confirming the storage \
                 subsystem is the bottleneck. Prioritize storage improvements.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Resolve I/O bottleneck".to_string(),
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
                "PostgreSQL FATAL or PANIC errors detected. This is urgent: check \
                 PostgreSQL server logs immediately for crash details, connection \
                 exhaustion, or data corruption. Also check for ERROR-level messages \
                 that may indicate the root cause.",
            )
        } else {
            String::from(
                "Elevated PostgreSQL error rate detected. Check PostgreSQL server logs \
                 for recurring error patterns. Common causes include connection limits, \
                 permission issues, query syntax errors, and constraint violations.",
            )
        };

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Investigate PostgreSQL errors".to_string(),
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
            "Container CPU throttling detected. The container is hitting its CPU quota \
             limit. Increase the CPU limit (cpu.max quota) in the container resource \
             configuration to allow more CPU time.",
        );

        if let Some(cpu) = find_incident(ctx.incidents, "cpu_high") {
            related.push(cpu);
            desc.push_str(
                " High CPU usage combined with throttling confirms the container needs \
                 more CPU resources. Consider both increasing the CPU limit and \
                 optimizing CPU-intensive queries.",
            );
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Increase container CPU limit".to_string(),
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
            "Dead tuples are accumulating while idle-in-transaction sessions are active. \
             Idle transactions hold the xmin horizon, preventing VACUUM from reclaiming \
             dead rows — this is the most common cause of unbounded table bloat.",
        );

        if let Some(ref s) = ctx.settings
            && let Some(timeout_ms) = s.get_ms("idle_in_transaction_session_timeout")
        {
            if timeout_ms == 0 {
                desc.push_str(
                    " CRITICAL: idle_in_transaction_session_timeout is disabled (0). \
                     Set it to 30s-5min to auto-terminate idle transactions.",
                );
            } else {
                desc.push_str(&format!(
                    " Current idle_in_transaction_session_timeout={}s.",
                    timeout_ms / 1000
                ));
            }
        }

        if let Some(lq) = find_incident(ctx.incidents, "long_query") {
            related.push(lq);
            desc.push_str(
                " Long-running queries also hold the xmin horizon. Review and optimize \
                 long transactions.",
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
            "Lock contention is causing a cascade: blocked sessions pile up and appear \
             as active, creating a session storm. A single blocking transaction is likely \
             holding locks that queue multiple waiters. Find and terminate the blocking \
             query using pg_locks. Use CONCURRENTLY variants for DDL operations.",
        );

        if let Some(ref s) = ctx.settings
            && let Some(lt_ms) = s.get_ms("lock_timeout")
            && lt_ms == 0
        {
            desc.push_str(
                " lock_timeout is disabled — queries wait for locks indefinitely. \
                 Set lock_timeout (e.g. 5s-30s) to prevent unbounded waits.",
            );
        }

        if let Some(lq) = find_incident(ctx.incidents, "long_query") {
            related.push(lq);
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Lock cascade causing session storm".to_string(),
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
            "High active session count combined with resource pressure indicates \
             connection overload. Too many concurrent queries cause CPU context switching \
             overhead and lock contention. Deploy a connection pooler (PgBouncer in \
             transaction mode) to limit active backends.",
        );

        if let Some(ref s) = ctx.settings
            && let Some(mc) = s.get_i64("max_connections")
        {
            desc.push_str(&format!(" Current max_connections={mc}."));
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
            title: "Connection storm detected".to_string(),
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
        let disk = match find_any_incident(ctx.incidents, &["disk_util_high", "disk_io_spike"]) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cp, backend, disk];
        let mut desc = String::from(
            "Write amplification detected: forced checkpoints + backends writing dirty \
             buffers directly + disk saturation. The write pipeline is overwhelmed. \
             Increase max_wal_size (e.g. 4-8 GB) to reduce forced checkpoint frequency. \
             Tune bgwriter (bgwriter_lru_maxpages, bgwriter_delay) to write dirty pages \
             proactively. Review whether unused indexes on write-heavy tables can be dropped \
             to reduce WAL volume.",
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
                desc.push_str(&format!(" Current settings: {}.", settings_info.join(", ")));
            }
        }

        // Collect extra disk incidents.
        for extra in find_all_incidents(ctx.incidents, &["disk_util_high", "disk_io_spike"]) {
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
        let io = match find_any_incident(ctx.incidents, &["disk_util_high", "iowait_high"]) {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cache, io];
        let mut desc = String::from(
            "Low cache hit ratio is causing excessive physical disk reads, resulting in \
             I/O saturation. The working set likely exceeds shared_buffers. Increase \
             shared_buffers to 25% of available RAM. Verify effective_cache_size reflects \
             total available memory (RAM minus OS overhead).",
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
                desc.push_str(&format!(" Current settings: {}.", info.join(", ")));
            }
        }

        // Add all related cache/io incidents.
        for extra in find_all_incidents(
            ctx.incidents,
            &[
                "cache_hit_ratio_drop",
                "index_cache_miss",
                "disk_util_high",
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
            title: "Cache misses causing I/O pressure".to_string(),
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
            "Sequential scans on large tables are causing resource pressure. Missing \
             indexes force PostgreSQL to read entire tables. Run EXPLAIN ANALYZE on top \
             queries from pg_stat_statements to identify missing indexes. Create targeted \
             B-tree indexes on frequently filtered columns.",
        );

        if let Some(ref s) = ctx.settings
            && let Some(rpc) = s.get_f64("random_page_cost")
            && rpc >= 4.0
        {
            desc.push_str(&format!(
                " random_page_cost={rpc} (default for HDD). If using SSD, set \
                 it to 1.1 to help the planner prefer index scans."
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
            "Autovacuum I/O is saturating the storage subsystem, competing with the \
             normal workload for disk bandwidth. Tune autovacuum_vacuum_cost_limit \
             (lower it) and increase autovacuum_vacuum_cost_delay to throttle vacuum I/O. \
             Consider scheduling manual VACUUM on large tables during off-peak hours.",
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
                desc.push_str(&format!(" Current settings: {}.", info.join(", ")));
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
            title: "Autovacuum causing I/O pressure".to_string(),
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
                "OOM kills have occurred alongside memory pressure and swap usage. \
                 URGENT: reduce PostgreSQL memory footprint immediately — lower \
                 shared_buffers, work_mem, and maintenance_work_mem. Reduce \
                 max_connections. Consider adding RAM or increasing container memory limits.",
            )
        } else {
            String::from(
                "Memory exhaustion trajectory: low available memory combined with active \
                 swap usage. PostgreSQL performance degrades severely under swap pressure. \
                 Reduce shared_buffers or work_mem if over-allocated. If PostgreSQL \
                 settings are appropriate, add physical RAM.",
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
                desc.push_str(&format!(" Current settings: {}.", info.join(", ")));
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
                "OOM kills detected — memory exhaustion".to_string()
            } else {
                "Memory exhaustion risk — approaching OOM".to_string()
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
                "Persistent query slowdown detected alongside resource pressure. The \
                 slow query may be both a symptom (degraded by resource contention) and \
                 a cause (consuming resources). Identify the specific query via \
                 pg_stat_statements (highest mean_exec_time). Run EXPLAIN ANALYZE and \
                 optimize the query plan.",
            )
        } else {
            String::from(
                "Persistent query slowdown detected without corresponding resource \
                 saturation. This pattern indicates a query plan regression — often \
                 caused by stale statistics after bulk data changes, schema modifications, \
                 or PostgreSQL upgrades. Run ANALYZE on affected tables. Compare EXPLAIN \
                 plans for the slow query.",
            )
        };

        if let Some(ref s) = ctx.settings
            && let Some(rpc) = s.get_f64("random_page_cost")
            && rpc >= 4.0
        {
            desc.push_str(&format!(
                " random_page_cost={rpc} (HDD default). For SSD storage, set \
                 to 1.1 to improve index scan preference."
            ));
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Query plan regression detected".to_string(),
            description: desc,
            related_incidents: related.iter().map(|i| i.rule_id.clone()).collect(),
        }]
    }
}

// ============================================================
// 19. TempFileSpillAdvisor
// ============================================================

pub struct TempFileSpillAdvisor;

impl Advisor for TempFileSpillAdvisor {
    fn id(&self) -> &'static str {
        "temp_file_spill"
    }

    fn evaluate(&self, ctx: &AdvisorContext<'_>) -> Vec<Recommendation> {
        let stmt = match find_incident(ctx.incidents, "stmt_mean_time_spike") {
            Some(i) => i,
            None => return Vec::new(),
        };
        let disk = match find_incident(ctx.incidents, "disk_io_spike") {
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
        let mut desc = String::from(
            "Possible temp file spill: query slowdown coincides with disk I/O spikes. \
             When work_mem is too small, PostgreSQL spills sort and hash operations to \
             temporary files on disk, dramatically increasing query time.",
        );

        if let Some(ref s) = ctx.settings {
            if let Some(wm) = s.get_bytes("work_mem") {
                desc.push_str(&format!(
                    " Current work_mem={}. Consider increasing to 64-256 MiB depending \
                     on concurrency.",
                    format_bytes(wm)
                ));
            }
            if let Some(ltf) = s.get_i64("log_temp_files")
                && ltf < 0
            {
                desc.push_str(
                    " log_temp_files is disabled. Enable it (set to 0) to confirm \
                     temp file usage in PostgreSQL logs.",
                );
            }
        }

        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Possible temp file spill — increase work_mem".to_string(),
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
        };
        let recs = VacuumBlockedAdvisor.evaluate(&ctx);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].description.contains("CRITICAL"));
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
}
