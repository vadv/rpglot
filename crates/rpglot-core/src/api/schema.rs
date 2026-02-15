//! API schema — extended metadata describing snapshot structure.
//!
//! Clients use this to understand column types, units, formatting rules,
//! available views, drill-down targets, and entity IDs.

use serde::Serialize;
use utoipa::ToSchema;

/// Top-level API schema returned by `GET /api/v1/schema`.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiSchema {
    /// Operating mode.
    pub mode: ApiMode,
    /// Timeline info (only in history mode).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline: Option<TimelineInfo>,
    /// Summary panel field descriptions.
    pub summary: SummarySchema,
    /// Tab descriptions.
    pub tabs: TabsSchema,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum ApiMode {
    Live,
    History,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TimelineInfo {
    /// First snapshot timestamp (epoch seconds).
    pub start: i64,
    /// Last snapshot timestamp (epoch seconds).
    pub end: i64,
    /// Total number of snapshots.
    pub total_snapshots: usize,
    /// Per-date index for efficient navigation. Present in timeline endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dates: Option<Vec<DateInfo>>,
}

/// Information about snapshots available on a specific date.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DateInfo {
    /// Date string in "YYYY-MM-DD" format (UTC).
    pub date: String,
    /// Number of snapshots on this date.
    pub count: usize,
    /// Timestamp of the first snapshot on this date.
    pub first_timestamp: i64,
    /// Timestamp of the last snapshot on this date.
    pub last_timestamp: i64,
}

// ============================================================
// Summary schema
// ============================================================

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SummarySchema {
    pub system: Vec<SummarySection>,
    pub pg: Vec<SummarySection>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SummarySection {
    pub key: String,
    pub label: String,
    pub fields: Vec<FieldSchema>,
}

// ============================================================
// Tab schema
// ============================================================

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TabsSchema {
    pub prc: TabSchema,
    pub pga: TabSchema,
    pub pgs: TabSchema,
    pub pgt: TabSchema,
    pub pgi: TabSchema,
    pub pgl: TabSchema,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TabSchema {
    pub name: String,
    pub description: String,
    /// Field name used as unique row identifier.
    pub entity_id: String,
    /// All available columns across all views.
    pub columns: Vec<ColumnSchema>,
    /// Available view modes.
    pub views: Vec<ViewSchema>,
    /// Drill-down navigation target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drill_down: Option<DrillDown>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ColumnSchema {
    /// JSON field name in the row object.
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Format>,
    pub sortable: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub filterable: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ViewSchema {
    pub key: String,
    pub label: String,
    /// Column keys to display in this view.
    pub columns: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
    /// Default sort column key.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort: Option<String>,
    /// Whether default sort is descending.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub default_sort_desc: bool,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DrillDown {
    /// Target tab key (e.g. "pgs", "pgi").
    pub target: String,
    /// Field in the SOURCE tab to get the value from.
    pub via: String,
    /// Field in the TARGET tab to search by. If absent, uses target's entity_id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_field: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FieldSchema {
    pub key: String,
    pub label: String,
    #[serde(rename = "type")]
    pub data_type: DataType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<Unit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<Format>,
}

// ============================================================
// Value types
// ============================================================

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DataType {
    Integer,
    Number,
    String,
    Boolean,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    Kb,
    Bytes,
    #[serde(rename = "bytes/s")]
    BytesPerSec,
    Ms,
    #[serde(rename = "s")]
    Seconds,
    Percent,
    #[serde(rename = "/s")]
    PerSec,
    #[serde(rename = "/min")]
    PerMin,
    #[serde(rename = "blks/s")]
    BlksPerSec,
    #[serde(rename = "MB/s")]
    MbPerSec,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// Human-readable bytes (e.g. "1.2 GiB").
    Bytes,
    /// Duration (e.g. "2h 15m", "3.5s").
    Duration,
    /// Rate (e.g. "1.2K/s").
    Rate,
    /// Percentage (e.g. "95.2%").
    Percent,
    /// Epoch age (e.g. "2h ago").
    Age,
}

// ============================================================
// Schema generation
// ============================================================

impl ApiSchema {
    /// Generate the full schema for a given mode.
    pub fn generate(mode: ApiMode, timeline: Option<TimelineInfo>) -> Self {
        Self {
            mode,
            timeline,
            summary: generate_summary_schema(),
            tabs: generate_tabs_schema(),
        }
    }
}

fn generate_summary_schema() -> SummarySchema {
    SummarySchema {
        system: vec![
            SummarySection {
                key: "cpu".into(),
                label: "CPU".into(),
                fields: vec![
                    field(
                        "sys_pct",
                        "System",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "usr_pct",
                        "User",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "irq_pct",
                        "IRQ",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "iow_pct",
                        "I/O Wait",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "idle_pct",
                        "Idle",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "steal_pct",
                        "Steal",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                ],
            },
            SummarySection {
                key: "load".into(),
                label: "Load".into(),
                fields: vec![
                    field("avg1", "1 min", DataType::Number, None, None),
                    field("avg5", "5 min", DataType::Number, None, None),
                    field("avg15", "15 min", DataType::Number, None, None),
                    field("nr_threads", "Threads", DataType::Integer, None, None),
                    field("nr_running", "Running", DataType::Integer, None, None),
                ],
            },
            SummarySection {
                key: "memory".into(),
                label: "Memory".into(),
                fields: vec![
                    field(
                        "total_kb",
                        "Total",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "available_kb",
                        "Available",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "cached_kb",
                        "Cached",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "buffers_kb",
                        "Buffers",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "slab_kb",
                        "Slab",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                ],
            },
            SummarySection {
                key: "swap".into(),
                label: "Swap".into(),
                fields: vec![
                    field(
                        "total_kb",
                        "Total",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "free_kb",
                        "Free",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "used_kb",
                        "Used",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "dirty_kb",
                        "Dirty",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                    field(
                        "writeback_kb",
                        "Writeback",
                        DataType::Integer,
                        Some(Unit::Kb),
                        Some(Format::Bytes),
                    ),
                ],
            },
            SummarySection {
                key: "psi".into(),
                label: "Pressure".into(),
                fields: vec![
                    field(
                        "cpu_some_pct",
                        "CPU",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "mem_some_pct",
                        "Memory",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "io_some_pct",
                        "I/O",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                ],
            },
            SummarySection {
                key: "vmstat".into(),
                label: "VMstat".into(),
                fields: vec![
                    field(
                        "pgin_s",
                        "Page In",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "pgout_s",
                        "Page Out",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "swin_s",
                        "Swap In",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "swout_s",
                        "Swap Out",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "pgfault_s",
                        "Faults",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "ctxsw_s",
                        "Context Sw",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                ],
            },
            SummarySection {
                key: "cgroup_cpu".into(),
                label: "Cgroup CPU".into(),
                fields: vec![
                    field("limit_cores", "Limit", DataType::Number, None, None),
                    field(
                        "used_pct",
                        "Used",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "usr_pct",
                        "User",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "sys_pct",
                        "System",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "throttled_ms",
                        "Throttled",
                        DataType::Number,
                        Some(Unit::Ms),
                        None,
                    ),
                    field("nr_throttled", "Thr Events", DataType::Number, None, None),
                ],
            },
            SummarySection {
                key: "cgroup_memory".into(),
                label: "Cgroup Memory".into(),
                fields: vec![
                    field(
                        "limit_bytes",
                        "Limit",
                        DataType::Integer,
                        Some(Unit::Bytes),
                        Some(Format::Bytes),
                    ),
                    field(
                        "used_bytes",
                        "Used",
                        DataType::Integer,
                        Some(Unit::Bytes),
                        Some(Format::Bytes),
                    ),
                    field(
                        "used_pct",
                        "Used%",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "anon_bytes",
                        "Anon",
                        DataType::Integer,
                        Some(Unit::Bytes),
                        Some(Format::Bytes),
                    ),
                    field(
                        "file_bytes",
                        "File",
                        DataType::Integer,
                        Some(Unit::Bytes),
                        Some(Format::Bytes),
                    ),
                    field(
                        "slab_bytes",
                        "Slab",
                        DataType::Integer,
                        Some(Unit::Bytes),
                        Some(Format::Bytes),
                    ),
                    field("oom_kills", "OOM Kills", DataType::Integer, None, None),
                ],
            },
            SummarySection {
                key: "cgroup_pids".into(),
                label: "Cgroup PIDs".into(),
                fields: vec![
                    field("current", "Current", DataType::Integer, None, None),
                    field("max", "Max", DataType::Integer, None, None),
                ],
            },
        ],
        pg: vec![
            SummarySection {
                key: "pg".into(),
                label: "PostgreSQL".into(),
                fields: vec![
                    field(
                        "tps",
                        "TPS",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "hit_ratio_pct",
                        "Hit Ratio",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "backend_io_hit_pct",
                        "Backend IO Hit",
                        DataType::Number,
                        Some(Unit::Percent),
                        Some(Format::Percent),
                    ),
                    field(
                        "tuples_s",
                        "Tuples",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "temp_bytes_s",
                        "Temp",
                        DataType::Number,
                        Some(Unit::BytesPerSec),
                        Some(Format::Bytes),
                    ),
                    field("deadlocks", "Deadlocks", DataType::Number, None, None),
                ],
            },
            SummarySection {
                key: "bgwriter".into(),
                label: "Background Writer".into(),
                fields: vec![
                    field(
                        "checkpoints_per_min",
                        "Ckpt/min",
                        DataType::Number,
                        Some(Unit::PerMin),
                        Some(Format::Rate),
                    ),
                    field(
                        "checkpoint_write_time_ms",
                        "Ckpt Write",
                        DataType::Number,
                        Some(Unit::Ms),
                        None,
                    ),
                    field(
                        "buffers_backend_s",
                        "BE Bufs",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "buffers_clean_s",
                        "Clean",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                    field(
                        "maxwritten_clean",
                        "MaxWritten",
                        DataType::Number,
                        None,
                        None,
                    ),
                    field(
                        "buffers_alloc_s",
                        "Alloc",
                        DataType::Number,
                        Some(Unit::PerSec),
                        Some(Format::Rate),
                    ),
                ],
            },
        ],
    }
}

fn generate_tabs_schema() -> TabsSchema {
    TabsSchema {
        prc: generate_prc_schema(),
        pga: generate_pga_schema(),
        pgs: generate_pgs_schema(),
        pgt: generate_pgt_schema(),
        pgi: generate_pgi_schema(),
        pgl: generate_pgl_schema(),
    }
}

fn generate_prc_schema() -> TabSchema {
    TabSchema {
        name: "Processes".into(),
        description: "OS processes".into(),
        entity_id: "pid".into(),
        columns: vec![
            col("pid", "PID", DataType::Integer, None, None, true, false),
            col("ppid", "PPID", DataType::Integer, None, None, true, false),
            col("name", "Name", DataType::String, None, None, true, true),
            col(
                "cmdline",
                "Command",
                DataType::String,
                None,
                None,
                false,
                true,
            ),
            col("state", "State", DataType::String, None, None, true, true),
            col(
                "cpu_pct",
                "CPU%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "mem_pct",
                "MEM%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "vsize_kb",
                "VIRT",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "rsize_kb",
                "RES",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vgrow_kb",
                "VGROW",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "rgrow_kb",
                "RGROW",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vswap_kb",
                "SWAP",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "read_bytes_s",
                "Read/s",
                DataType::Number,
                Some(Unit::BytesPerSec),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "write_bytes_s",
                "Write/s",
                DataType::Number,
                Some(Unit::BytesPerSec),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "read_ops_s",
                "RdOps/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "write_ops_s",
                "WrOps/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col("uid", "UID", DataType::Integer, None, None, true, false),
            col("euid", "EUID", DataType::Integer, None, None, true, false),
            col("gid", "GID", DataType::Integer, None, None, true, false),
            col("egid", "EGID", DataType::Integer, None, None, true, false),
            col(
                "num_threads",
                "Threads",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col("curcpu", "CPU#", DataType::Integer, None, None, true, false),
            col("nice", "Nice", DataType::Integer, None, None, true, false),
            col(
                "priority",
                "Priority",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "rtprio",
                "RT Prio",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "policy",
                "Policy",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "blkdelay",
                "I/O Delay",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "nvcsw_s",
                "VCSW/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "nivcsw_s",
                "IVCSW/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "psize_kb",
                "PSS",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vstext_kb",
                "CODE",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vdata_kb",
                "DATA",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vstack_kb",
                "STACK",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vslibs_kb",
                "LIBS",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "vlock_kb",
                "LOCK",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "minflt",
                "MinFlt",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "majflt",
                "MajFlt",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "total_read_bytes",
                "Total Read",
                DataType::Integer,
                Some(Unit::Bytes),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "total_write_bytes",
                "Total Write",
                DataType::Integer,
                Some(Unit::Bytes),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "cancelled_write_bytes",
                "Cancelled",
                DataType::Integer,
                Some(Unit::Bytes),
                Some(Format::Bytes),
                true,
                false,
            ),
            col("tty", "TTY", DataType::Integer, None, None, true, false),
            col(
                "exit_signal",
                "Exit Sig",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "pg_query",
                "PG Query",
                DataType::String,
                None,
                None,
                false,
                true,
            ),
            col(
                "pg_backend_type",
                "PG Backend",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
        ],
        views: vec![
            ViewSchema {
                key: "generic".into(),
                label: "Generic".into(),
                columns: vec![
                    "pid",
                    "name",
                    "state",
                    "cpu_pct",
                    "mem_pct",
                    "vgrow_kb",
                    "rgrow_kb",
                    "uid",
                    "euid",
                    "num_threads",
                    "curcpu",
                    "cmdline",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: true,
                default_sort: Some("cpu_pct".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "command".into(),
                label: "Command".into(),
                columns: vec![
                    "pid", "name", "ppid", "state", "cpu_pct", "mem_pct", "cmdline",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("cpu_pct".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "memory".into(),
                label: "Memory".into(),
                columns: vec![
                    "pid",
                    "name",
                    "mem_pct",
                    "vsize_kb",
                    "rsize_kb",
                    "psize_kb",
                    "vgrow_kb",
                    "rgrow_kb",
                    "vswap_kb",
                    "vstext_kb",
                    "vdata_kb",
                    "vstack_kb",
                    "vslibs_kb",
                    "vlock_kb",
                    "minflt",
                    "majflt",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("mem_pct".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "disk".into(),
                label: "Disk".into(),
                columns: vec![
                    "pid",
                    "name",
                    "read_bytes_s",
                    "write_bytes_s",
                    "read_ops_s",
                    "write_ops_s",
                    "cancelled_write_bytes",
                    "cmdline",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("read_bytes_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "pg".into(),
                label: "PostgreSQL".into(),
                columns: vec![
                    "pid",
                    "name",
                    "cpu_pct",
                    "mem_pct",
                    "pg_backend_type",
                    "pg_query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("cpu_pct".into()),
                default_sort_desc: true,
            },
        ],
        drill_down: Some(DrillDown {
            target: "pga".into(),
            via: "pid".into(),
            target_field: None,
            description: "Navigate to session details for this PID".into(),
        }),
    }
}

fn generate_pga_schema() -> TabSchema {
    TabSchema {
        name: "pg_stat_activity".into(),
        description: "Active PostgreSQL backends".into(),
        entity_id: "pid".into(),
        columns: vec![
            col("pid", "PID", DataType::Integer, None, None, true, false),
            col(
                "database",
                "Database",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col("user", "User", DataType::String, None, None, true, true),
            col(
                "application_name",
                "App",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col(
                "client_addr",
                "Client",
                DataType::String,
                None,
                None,
                true,
                false,
            ),
            col("state", "State", DataType::String, None, None, true, true),
            col(
                "wait_event_type",
                "Wait Type",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col(
                "wait_event",
                "Wait Event",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col(
                "backend_type",
                "Backend Type",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col("query", "Query", DataType::String, None, None, false, true),
            col(
                "query_id",
                "Query ID",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "query_duration_s",
                "Query Dur",
                DataType::Number,
                Some(Unit::Seconds),
                Some(Format::Duration),
                true,
                false,
            ),
            col(
                "xact_duration_s",
                "Xact Dur",
                DataType::Number,
                Some(Unit::Seconds),
                Some(Format::Duration),
                true,
                false,
            ),
            col(
                "backend_duration_s",
                "Backend Dur",
                DataType::Number,
                Some(Unit::Seconds),
                Some(Format::Duration),
                true,
                false,
            ),
            col(
                "backend_start",
                "Backend Start",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "xact_start",
                "Xact Start",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "query_start",
                "Query Start",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "cpu_pct",
                "CPU%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "rss_kb",
                "RSS",
                DataType::Integer,
                Some(Unit::Kb),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "stmt_mean_exec_time_ms",
                "MEAN",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "stmt_max_exec_time_ms",
                "MAX",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "stmt_calls_s",
                "CALL/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "stmt_hit_pct",
                "HIT%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
        ],
        views: vec![
            ViewSchema {
                key: "generic".into(),
                label: "Generic".into(),
                columns: vec![
                    "pid",
                    "cpu_pct",
                    "rss_kb",
                    "database",
                    "user",
                    "state",
                    "wait_event_type",
                    "wait_event",
                    "query_duration_s",
                    "xact_duration_s",
                    "backend_duration_s",
                    "backend_type",
                    "query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: true,
                default_sort: Some("query_duration_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "stats".into(),
                label: "Stats".into(),
                columns: vec![
                    "pid",
                    "database",
                    "user",
                    "state",
                    "query_duration_s",
                    "stmt_mean_exec_time_ms",
                    "stmt_max_exec_time_ms",
                    "stmt_calls_s",
                    "stmt_hit_pct",
                    "query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("query_duration_s".into()),
                default_sort_desc: true,
            },
        ],
        drill_down: Some(DrillDown {
            target: "pgs".into(),
            via: "query_id".into(),
            target_field: Some("queryid".into()),
            description: "Navigate to statement stats by query_id".into(),
        }),
    }
}

fn generate_pgs_schema() -> TabSchema {
    TabSchema {
        name: "pg_stat_statements".into(),
        description: "PostgreSQL statement statistics".into(),
        entity_id: "queryid".into(),
        columns: vec![
            col(
                "queryid",
                "Query ID",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "database",
                "Database",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col("user", "User", DataType::String, None, None, true, true),
            col("query", "Query", DataType::String, None, None, false, true),
            col("calls", "Calls", DataType::Integer, None, None, true, false),
            col("rows", "Rows", DataType::Integer, None, None, true, false),
            col(
                "mean_exec_time_ms",
                "Mean Time",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "min_exec_time_ms",
                "Min Time",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "max_exec_time_ms",
                "Max Time",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "stddev_exec_time_ms",
                "Stddev",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "calls_s",
                "Calls/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "rows_s",
                "Rows/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "exec_time_ms_s",
                "Time/s",
                DataType::Number,
                Some(Unit::Ms),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "shared_blks_read_s",
                "Blk Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "shared_blks_hit_s",
                "Blk Hit/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "shared_blks_dirtied_s",
                "Blk Dirt/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "shared_blks_written_s",
                "Blk Wr/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "local_blks_read_s",
                "Loc Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "local_blks_written_s",
                "Loc Wr/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "temp_blks_read_s",
                "Tmp Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "temp_blks_written_s",
                "Tmp Wr/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "temp_mb_s",
                "Tmp MB/s",
                DataType::Number,
                Some(Unit::MbPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "rows_per_call",
                "R/Call",
                DataType::Number,
                None,
                None,
                true,
                false,
            ),
            col(
                "hit_pct",
                "HIT%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "total_plan_time",
                "Plan Time",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "total_exec_time",
                "Total Time",
                DataType::Number,
                Some(Unit::Ms),
                None,
                true,
                false,
            ),
            col(
                "wal_records",
                "WAL Rec",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "wal_bytes",
                "WAL Bytes",
                DataType::Integer,
                Some(Unit::Bytes),
                Some(Format::Bytes),
                true,
                false,
            ),
        ],
        views: vec![
            ViewSchema {
                key: "time".into(),
                label: "Time".into(),
                columns: vec![
                    "calls_s",
                    "exec_time_ms_s",
                    "mean_exec_time_ms",
                    "rows_s",
                    "database",
                    "user",
                    "query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: true,
                default_sort: Some("exec_time_ms_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "calls".into(),
                label: "Calls".into(),
                columns: vec![
                    "calls_s",
                    "rows_s",
                    "rows_per_call",
                    "mean_exec_time_ms",
                    "database",
                    "user",
                    "query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("calls_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "io".into(),
                label: "I/O".into(),
                columns: vec![
                    "calls_s",
                    "shared_blks_read_s",
                    "shared_blks_hit_s",
                    "hit_pct",
                    "shared_blks_dirtied_s",
                    "shared_blks_written_s",
                    "database",
                    "query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("shared_blks_read_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "temp".into(),
                label: "Temp".into(),
                columns: vec![
                    "calls_s",
                    "temp_blks_read_s",
                    "temp_blks_written_s",
                    "temp_mb_s",
                    "local_blks_read_s",
                    "local_blks_written_s",
                    "database",
                    "query",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("temp_mb_s".into()),
                default_sort_desc: true,
            },
        ],
        drill_down: None,
    }
}

fn generate_pgt_schema() -> TabSchema {
    TabSchema {
        name: "pg_stat_user_tables".into(),
        description: "PostgreSQL table statistics".into(),
        entity_id: "relid".into(),
        columns: vec![
            col("relid", "OID", DataType::Integer, None, None, true, false),
            col("schema", "Schema", DataType::String, None, None, true, true),
            col("table", "Table", DataType::String, None, None, true, true),
            col(
                "display_name",
                "Name",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col(
                "n_live_tup",
                "Live Tuples",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "n_dead_tup",
                "Dead Tuples",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "size_bytes",
                "Size",
                DataType::Integer,
                Some(Unit::Bytes),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "last_autovacuum",
                "Last AVacuum",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "last_autoanalyze",
                "Last AAnalyze",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "seq_scan_s",
                "Seq/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "seq_tup_read_s",
                "Seq Rd/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_scan_s",
                "Idx/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_tup_fetch_s",
                "Idx Ft/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "n_tup_ins_s",
                "Ins/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "n_tup_upd_s",
                "Upd/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "n_tup_del_s",
                "Del/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "n_tup_hot_upd_s",
                "Hot Upd/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "vacuum_count_s",
                "Vac/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "autovacuum_count_s",
                "AVac/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "heap_blks_read_s",
                "Heap Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "heap_blks_hit_s",
                "Heap Hit/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_blks_read_s",
                "Idx Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_blks_hit_s",
                "Idx Hit/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "tot_tup_read_s",
                "Tot Rd/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "disk_blks_read_s",
                "DISK/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "io_hit_pct",
                "HIT%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "seq_pct",
                "SEQ%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "dead_pct",
                "DEAD%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "hot_pct",
                "HOT%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "analyze_count_s",
                "Anl/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "autoanalyze_count_s",
                "AAnl/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "last_vacuum",
                "Last Vacuum",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "last_analyze",
                "Last Analyze",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                true,
                false,
            ),
            col(
                "toast_blks_read_s",
                "Toast Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "toast_blks_hit_s",
                "Toast Hit/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "tidx_blks_read_s",
                "TIdx Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "tidx_blks_hit_s",
                "TIdx Hit/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
        ],
        views: vec![
            ViewSchema {
                key: "reads".into(),
                label: "Reads".into(),
                columns: vec![
                    "seq_tup_read_s",
                    "idx_tup_fetch_s",
                    "tot_tup_read_s",
                    "seq_scan_s",
                    "idx_scan_s",
                    "io_hit_pct",
                    "disk_blks_read_s",
                    "size_bytes",
                    "display_name",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("tot_tup_read_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "writes".into(),
                label: "Writes".into(),
                columns: vec![
                    "n_tup_ins_s",
                    "n_tup_upd_s",
                    "n_tup_del_s",
                    "n_tup_hot_upd_s",
                    "n_live_tup",
                    "n_dead_tup",
                    "io_hit_pct",
                    "disk_blks_read_s",
                    "size_bytes",
                    "display_name",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("n_tup_ins_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "scans".into(),
                label: "Scans".into(),
                columns: vec![
                    "seq_scan_s",
                    "seq_tup_read_s",
                    "idx_scan_s",
                    "idx_tup_fetch_s",
                    "seq_pct",
                    "io_hit_pct",
                    "disk_blks_read_s",
                    "size_bytes",
                    "display_name",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("seq_pct".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "maintenance".into(),
                label: "Maintenance".into(),
                columns: vec![
                    "n_dead_tup",
                    "n_live_tup",
                    "dead_pct",
                    "vacuum_count_s",
                    "autovacuum_count_s",
                    "last_autovacuum",
                    "last_autoanalyze",
                    "display_name",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("dead_pct".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "io".into(),
                label: "I/O".into(),
                columns: vec![
                    "heap_blks_read_s",
                    "heap_blks_hit_s",
                    "idx_blks_read_s",
                    "idx_blks_hit_s",
                    "io_hit_pct",
                    "disk_blks_read_s",
                    "size_bytes",
                    "display_name",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: true,
                default_sort: Some("heap_blks_read_s".into()),
                default_sort_desc: true,
            },
        ],
        drill_down: Some(DrillDown {
            target: "pgi".into(),
            via: "relid".into(),
            target_field: Some("relid".into()),
            description: "Navigate to indexes for this table".into(),
        }),
    }
}

fn generate_pgi_schema() -> TabSchema {
    TabSchema {
        name: "pg_stat_user_indexes".into(),
        description: "PostgreSQL index statistics".into(),
        entity_id: "indexrelid".into(),
        columns: vec![
            col(
                "indexrelid",
                "OID",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "relid",
                "Table OID",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col("schema", "Schema", DataType::String, None, None, true, true),
            col("table", "Table", DataType::String, None, None, true, true),
            col("index", "Index", DataType::String, None, None, true, true),
            col(
                "display_table",
                "Table Name",
                DataType::String,
                None,
                None,
                true,
                true,
            ),
            col(
                "idx_scan",
                "Idx Scans",
                DataType::Integer,
                None,
                None,
                true,
                false,
            ),
            col(
                "size_bytes",
                "Size",
                DataType::Integer,
                Some(Unit::Bytes),
                Some(Format::Bytes),
                true,
                false,
            ),
            col(
                "idx_scan_s",
                "Idx/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_tup_read_s",
                "Tup Rd/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_tup_fetch_s",
                "Tup Ft/s",
                DataType::Number,
                Some(Unit::PerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_blks_read_s",
                "Blk Rd/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "idx_blks_hit_s",
                "Blk Hit/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
            col(
                "io_hit_pct",
                "HIT%",
                DataType::Number,
                Some(Unit::Percent),
                Some(Format::Percent),
                true,
                false,
            ),
            col(
                "disk_blks_read_s",
                "DISK/s",
                DataType::Number,
                Some(Unit::BlksPerSec),
                Some(Format::Rate),
                true,
                false,
            ),
        ],
        views: vec![
            ViewSchema {
                key: "usage".into(),
                label: "Usage".into(),
                columns: vec![
                    "idx_scan_s",
                    "idx_tup_read_s",
                    "idx_tup_fetch_s",
                    "io_hit_pct",
                    "disk_blks_read_s",
                    "size_bytes",
                    "display_table",
                    "index",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: false,
                default_sort: Some("idx_tup_read_s".into()),
                default_sort_desc: true,
            },
            ViewSchema {
                key: "unused".into(),
                label: "Unused".into(),
                columns: vec!["idx_scan", "size_bytes", "display_table", "index"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                default: false,
                default_sort: Some("idx_scan".into()),
                default_sort_desc: false,
            },
            ViewSchema {
                key: "io".into(),
                label: "I/O".into(),
                columns: vec![
                    "idx_blks_read_s",
                    "idx_blks_hit_s",
                    "io_hit_pct",
                    "disk_blks_read_s",
                    "size_bytes",
                    "display_table",
                    "index",
                ]
                .into_iter()
                .map(String::from)
                .collect(),
                default: true,
                default_sort: Some("idx_blks_read_s".into()),
                default_sort_desc: true,
            },
        ],
        drill_down: None,
    }
}

fn generate_pgl_schema() -> TabSchema {
    TabSchema {
        name: "pg_locks".into(),
        description: "PostgreSQL blocking lock tree".into(),
        entity_id: "pid".into(),
        columns: vec![
            col("pid", "PID", DataType::Integer, None, None, false, false),
            col(
                "depth",
                "Depth",
                DataType::Integer,
                None,
                None,
                false,
                false,
            ),
            col(
                "root_pid",
                "Root PID",
                DataType::Integer,
                None,
                None,
                false,
                false,
            ),
            col(
                "database",
                "Database",
                DataType::String,
                None,
                None,
                false,
                true,
            ),
            col("user", "User", DataType::String, None, None, false, true),
            col("state", "State", DataType::String, None, None, false, true),
            col(
                "wait_event_type",
                "Wait Type",
                DataType::String,
                None,
                None,
                false,
                false,
            ),
            col(
                "wait_event",
                "Wait Event",
                DataType::String,
                None,
                None,
                false,
                false,
            ),
            col(
                "lock_type",
                "Lock Type",
                DataType::String,
                None,
                None,
                false,
                false,
            ),
            col(
                "lock_mode",
                "Lock Mode",
                DataType::String,
                None,
                None,
                false,
                false,
            ),
            col(
                "lock_target",
                "Target",
                DataType::String,
                None,
                None,
                false,
                true,
            ),
            col(
                "lock_granted",
                "Granted",
                DataType::Boolean,
                None,
                None,
                false,
                false,
            ),
            col("query", "Query", DataType::String, None, None, false, true),
            col(
                "xact_start",
                "Xact Start",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                false,
                false,
            ),
            col(
                "query_start",
                "Query Start",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                false,
                false,
            ),
            col(
                "state_change",
                "State Change",
                DataType::Integer,
                Some(Unit::Seconds),
                Some(Format::Age),
                false,
                false,
            ),
        ],
        views: vec![ViewSchema {
            key: "tree".into(),
            label: "Lock Tree".into(),
            columns: vec![
                "pid",
                "depth",
                "state",
                "wait_event_type",
                "wait_event",
                "lock_mode",
                "lock_target",
                "query",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            default: true,
            default_sort: None,
            default_sort_desc: false,
        }],
        drill_down: Some(DrillDown {
            target: "pga".into(),
            via: "pid".into(),
            target_field: None,
            description: "Navigate to session details for this PID".into(),
        }),
    }
}

// ============================================================
// Helpers
// ============================================================

fn field(
    key: &str,
    label: &str,
    data_type: DataType,
    unit: Option<Unit>,
    format: Option<Format>,
) -> FieldSchema {
    FieldSchema {
        key: key.into(),
        label: label.into(),
        data_type,
        unit,
        format,
    }
}

fn col(
    key: &str,
    label: &str,
    data_type: DataType,
    unit: Option<Unit>,
    format: Option<Format>,
    sortable: bool,
    filterable: bool,
) -> ColumnSchema {
    ColumnSchema {
        key: key.into(),
        label: label.into(),
        data_type,
        unit,
        format,
        sortable,
        filterable,
    }
}
