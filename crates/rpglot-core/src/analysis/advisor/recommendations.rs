use crate::analysis::advisor::{Advisor, Recommendation};
use crate::analysis::{Incident, Severity};

// ============================================================
// Helpers
// ============================================================

fn find_incident<'a>(incidents: &'a [Incident], rule_id: &str) -> Option<&'a Incident> {
    incidents.iter().find(|i| i.rule_id == rule_id)
}

fn worst_severity(incidents: &[&Incident]) -> Severity {
    incidents
        .iter()
        .map(|i| i.severity)
        .max()
        .unwrap_or(Severity::Info)
}

// ============================================================
// 1. ReplicationLagAdvisor
// ============================================================

pub struct ReplicationLagAdvisor;

impl Advisor for ReplicationLagAdvisor {
    fn id(&self) -> &'static str {
        "replication_lag"
    }

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let sync = match find_incident(incidents, "wait_sync_replica") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![sync];
        let mut desc = String::from(
            "Synchronous replication waits detected. Check pg_stat_replication for replica \
             lag and verify synchronous_standby_names configuration. Consider switching to \
             asynchronous replication if latency is acceptable.",
        );

        if let Some(cpu) = find_incident(incidents, "cpu_high") {
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let blocked = match find_incident(incidents, "blocked_sessions") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![blocked];
        let mut desc = String::from(
            "Blocked sessions detected due to lock contention. Identify blocking queries \
             using pg_locks and pg_stat_activity. Review transaction isolation levels and \
             consider adding lock_timeout to prevent indefinite waits.",
        );

        if let Some(long_q) = find_incident(incidents, "long_query") {
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let cpu = match find_incident(incidents, "cpu_high") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cpu];
        let mut desc = String::from(
            "High CPU utilization detected. Check top queries in pg_stat_statements \
             ordered by total_exec_time. Look for missing indexes causing sequential scans \
             and consider query optimization.",
        );

        if let Some(iow) = find_incident(incidents, "iowait_high") {
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let mem = match find_incident(incidents, "memory_low") {
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

        if let Some(swap) = find_incident(incidents, "swap_usage") {
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let dead = match find_incident(incidents, "dead_tuples_high") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let related = vec![dead];
        let severity = worst_severity(&related);

        vec![Recommendation {
            id: self.id().to_string(),
            severity,
            title: "Reduce table bloat".to_string(),
            description: String::from(
                "High dead tuple ratio detected, indicating table bloat. Run VACUUM FULL \
                 on affected tables during a maintenance window to reclaim space. For \
                 long-term prevention, adjust autovacuum settings: lower \
                 autovacuum_vacuum_scale_factor (e.g. 0.05), increase \
                 autovacuum_vacuum_cost_limit, and reduce autovacuum_vacuum_cost_delay.",
            ),
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let cp = match find_incident(incidents, "checkpoint_spike") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![cp];
        let mut desc = String::from(
            "Checkpoint activity spike detected. Consider increasing checkpoint_timeout \
             (e.g. 15min) and max_wal_size (e.g. 4GB+) to spread checkpoint I/O over \
             longer intervals and reduce write amplification.",
        );

        if let Some(backend) = find_incident(incidents, "backend_buffers_high") {
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let disk = match find_incident(incidents, "disk_util_high") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![disk];
        let mut desc = String::from(
            "High disk utilization detected. Check the storage subsystem for performance \
             bottlenecks. Consider upgrading to faster storage (NVMe), distributing I/O \
             across multiple disks, or reducing write-heavy operations during peak hours.",
        );

        if let Some(iow) = find_incident(incidents, "iowait_high") {
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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let error = find_incident(incidents, "pg_errors");
        let fatal = find_incident(incidents, "pg_fatal_panic");

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

    fn evaluate(&self, incidents: &[Incident]) -> Vec<Recommendation> {
        let throttle = match find_incident(incidents, "cgroup_throttled") {
            Some(i) => i,
            None => return Vec::new(),
        };

        let mut related = vec![throttle];
        let mut desc = String::from(
            "Container CPU throttling detected. The container is hitting its CPU quota \
             limit. Increase the CPU limit (cpu.max quota) in the container resource \
             configuration to allow more CPU time.",
        );

        if let Some(cpu) = find_incident(incidents, "cpu_high") {
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
