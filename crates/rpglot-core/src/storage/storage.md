# Storage Module — Internal Documentation

Полная документация по архитектуре хранения: [`docs/storage.md`](../../../../docs/storage.md).

Этот файл — краткая справка по модулю `crates/rpglot-core/src/storage/`.

## Модули

| Файл | Описание |
|------|----------|
| `interner.rs` | StringInterner — дедупликация строк через xxh3_64 хеши |
| `chunk.rs` | Chunk формат RPG3: zstd + dictionary + index, random access |
| `heatmap.rs` | Heatmap формат HM03: sidecar для timeline без декомпрессии |
| `manager.rs` | StorageManager: WAL (CRC32 framing), flush, rotation |
| `model/snapshot.rs` | Snapshot, DataBlock enum (25 вариантов) |
| `model/process.rs` | ProcessInfo — метрики /proc/[pid]/* |
| `model/system.rs` | System*Info — метрики /proc/* |
| `model/postgres.rs` | PgStat*Info — метрики PostgreSQL |
| `model/cgroup.rs` | CgroupInfo — метрики cgroup v2 |
| `model/mod.rs` | Re-exports всех моделей |

## Форматы файлов

- **Chunk**: `rpglot_YYYY-MM-DD_HH.zst` — RPG3 формат, postcard serialization, zstd + trained dictionary
- **Heatmap**: `rpglot_YYYY-MM-DD_HH.heatmap` — HM03, 14 bytes per entry (CPU%, sessions, errors, events)
- **WAL**: `wal.log` — CRC32-framed postcard(WalEntry), fsync после каждого фрейма

## Ключевые принципы

1. **WAL-only архитектура** — StorageManager не держит снапшоты в памяти
2. **Atomic writes** — chunk пишется в `.tmp`, затем `rename()`
3. **Random access** — index в начале chunk позволяет O(1) доступ по позиции
4. **String interning** — все строки хранятся как u64 хеши, interner сериализуется отдельно
5. **Hourly segmentation** — один файл на час, удобная ротация по дате
6. **Backward compatibility** — новые поля в моделях добавлять с `#[serde(default)]`

## DataBlock (25 вариантов)

### OS Processes
- `Processes(Vec<ProcessInfo>)` — /proc/[pid]/*

### PostgreSQL instance-level
- `PgStatActivity(Vec<PgStatActivityInfo>)` — pg_stat_activity
- `PgStatStatements(Vec<PgStatStatementsInfo>)` — pg_stat_statements
- `PgStatDatabase(Vec<PgStatDatabaseInfo>)` — pg_stat_database
- `PgStatBgwriter(PgStatBgwriterInfo)` — pg_stat_bgwriter
- `PgLockTree(Vec<PgLockTreeNode>)` — pg_locks
- `PgStatProgressVacuum(Vec<PgStatProgressVacuumInfo>)` — pg_stat_progress_vacuum
- `PgLogErrors(Vec<PgLogEntry>)` — PG log (grouped)
- `PgLogEvents(PgLogEventsInfo)` — PG log (legacy counts)
- `PgLogDetailedEvents(Vec<PgLogEventEntry>)` — PG log (per-event, source-of-truth)
- `PgSettings(Vec<PgSettingEntry>)` — pg_settings (hourly)

### PostgreSQL per-database
- `PgStatUserTables(Vec<PgStatUserTablesInfo>)` — pg_stat_user_tables
- `PgStatUserIndexes(Vec<PgStatUserIndexesInfo>)` — pg_stat_user_indexes

### System metrics
- `SystemCpu(Vec<SystemCpuInfo>)` — /proc/stat
- `SystemLoad(SystemLoadInfo)` — /proc/loadavg
- `SystemMem(SystemMemInfo)` — /proc/meminfo
- `SystemNet(Vec<SystemNetInfo>)` — /proc/net/dev
- `SystemDisk(Vec<SystemDiskInfo>)` — /proc/diskstats
- `SystemPsi(Vec<SystemPsiInfo>)` — /proc/pressure/*
- `SystemVmstat(SystemVmstatInfo)` — /proc/vmstat
- `SystemFile(SystemFileInfo)` — /proc/sys/fs/*
- `SystemInterrupts(Vec<SystemInterruptInfo>)` — /proc/interrupts
- `SystemSoftirqs(Vec<SystemSoftirqInfo>)` — /proc/softirqs
- `SystemStat(SystemStatInfo)` — /proc/stat (ctxt, procs)
- `SystemNetSnmp(SystemNetSnmpInfo)` — /proc/net/snmp

### Container
- `Cgroup(CgroupInfo)` — /sys/fs/cgroup/*
