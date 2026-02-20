# Architecture

## Overview

rpglot — инструмент для анализа проблем PostgreSQL. Демон `rpglotd` записывает метрики каждые 10 секунд, `rpglot-web` показывает их через браузер, `rpglot` — через терминал.

```
┌─────────────┐          ┌──────────────┐          ┌──────────────┐
│   rpglotd   │──write──▶│  .zst chunks │◀──read───│  rpglot-web  │
│  (collector) │          │  + wal.log   │          │  (REST + SSE)│
└─────────────┘          └──────────────┘          └──────┬───────┘
                                                          │
                                                   React SPA
                                                   (embedded)
```

---

## Workspace

```
crates/
├── rpglot-core/     # shared library (collector, storage, models, rates, TUI, API)
├── rpglot/          # TUI binary (ratatui, live + history)
├── rpglot-web/      # Web binary (axum REST/SSE + React frontend)
├── rpglotd/         # Daemon binary (collection + storage)
└── rpglotd-dump/    # CLI tool для инспекции .zst/.heatmap/wal
```

**Feature gates (rpglot-core):**

| Feature    | Включает                               | Используется в       |
|------------|----------------------------------------|----------------------|
| `provider` | SnapshotProvider, LiveProvider, History | rpglot, rpglot-web   |
| `tui`      | ratatui виджеты, view models, state    | rpglot               |
| `api`      | JSON API types, analysis, convert      | rpglot-web           |

rpglotd и rpglotd-dump используют rpglot-core без features (только collector + storage).

---

## rpglot-core

### Модульная структура

```
src/
├── collector/           # Сбор метрик (OS + PostgreSQL + cgroup)
│   ├── procfs/          #   /proc/[pid]/stat, /proc/meminfo, diskstats, net/dev, ...
│   ├── pg_collector/    #   pg_stat_activity, statements, tables, indexes, locks, ...
│   ├── cgroup/          #   /sys/fs/cgroup (memory, CPU limits)
│   ├── log_collector/   #   PostgreSQL error log parsing (CSV/JSON)
│   └── mock/            #   MockFs для тестирования без /proc (macOS)
│
├── storage/             # Persistence
│   ├── chunk.rs         #   RPG6 format (zstd + dictionary + index)
│   ├── manager.rs       #   WAL, flush, rotation, hourly segmentation
│   ├── heatmap.rs       #   HM04 sidecar для timeline visualization
│   ├── interner.rs      #   StringInterner (xxh3 hash → string dedup)
│   └── model/           #   Snapshot, DataBlock enum, все Info structs
│
├── provider/            # Источник данных
│   ├── live.rs          #   LiveProvider (real-time collection)
│   └── history.rs       #   HistoryProvider (playback from disk, lazy init)
│
├── rates.rs             # Rate computation (единый для TUI и Web)
├── models/              # View modes, rate structs (PgStatementsRates, ...)
├── table.rs             # Generic table state (sort, filter, selection by entity ID)
├── fmt.rs               # Formatting (bytes, duration, rate, percent)
├── util/                # Helpers (container detection, time parsing)
│
├── api/                 # [feature "api"] JSON API types
│   ├── convert.rs       #   Snapshot → ApiSnapshot conversion
│   ├── snapshot.rs      #   ApiSnapshot (JSON-serializable)
│   └── schema.rs        #   ApiSchema (column metadata, units, thresholds)
│
├── analysis/            # [feature "api"] Anomaly detection
│   ├── rules/           #   Per-category detection (cpu, memory, pg_activity, ...)
│   └── advisor/         #   Incident grouping + recommendations
│
├── tui/                 # [feature "tui"] Terminal UI
│   ├── app.rs           #   App loop, snapshot advance/rewind
│   ├── input.rs         #   Key handling, NavigableTable dispatch
│   ├── navigable.rs     #   NavigableTable trait (shared navigation)
│   ├── state/           #   AppState, per-tab state (PGA, PGS, PGT, ...)
│   ├── widgets/         #   Per-tab rendering + detail popups
│   └── render.rs        #   Main render dispatcher
│
└── view/                # [feature "tui"] View models (column sets per view mode)
```

---

## Collector

### Архитектура

```
Collector<F: FileSystem>
├── SystemCollector      /proc/stat, meminfo, loadavg, diskstats, net/dev, pressure, vmstat
├── ProcessCollector     /proc/[pid]/stat, status, io, cmdline, comm
├── PostgresCollector    pg_stat_*, pg_locks, pg_store_plans, replication, settings, log
└── CgroupCollector      /sys/fs/cgroup (memory, CPU — контейнеры)
```

`FileSystem` trait абстрагирует `/proc` — на macOS используется `MockFs` для тестов.

### PostgreSQL Collector

**Два типа метрик:**
- **Instance-level** (одно соединение): pg_stat_activity, pg_stat_statements, pg_stat_database, pg_stat_bgwriter, pg_locks, pg_settings, replication
- **Per-database** (N соединений): pg_stat_user_tables, pg_stat_user_indexes

Пул соединений обновляется каждые 10 минут (`ensure_db_clients()`). OID таблиц/индексов уникальны в пределах кластера.

### Кеширование

Коллектор кеширует тяжёлые запросы:

| Источник             | Интервал кеша | Причина                                    |
|----------------------|---------------|--------------------------------------------|
| pg_stat_statements   | 30s           | ~500 строк, тяжёлый JOIN                   |
| pg_stat_user_tables  | 30s           | pg_relation_size() медленный               |
| pg_stat_user_indexes | 30s           | pg_relation_size() медленный               |
| pg_store_plans       | 5m (300s)     | Расширение для планов, редко меняется      |
| pg_settings          | 1h            | Конфигурация, почти не меняется            |
| replication_status   | 30s           | Лёгкий запрос, но не каждый tick           |

**Activity-only filtering:** Для statements/tables/indexes — в снапшот попадают только строки, у которых счётчики изменились с прошлого раза. Уменьшает размер снапшотов.

### Совместимость

PostgreSQL 10+. Version-aware SQL: `query_id` (PG 14+), `total_plan_time` (PG 13+), split bgwriter/checkpointer (PG 17+).

---

## Storage

### Файловая структура

```
/var/lib/rpglot/
  rpglot_2026-02-15_00.zst       # chunk: 00:00–00:59 (до 360 снапшотов)
  rpglot_2026-02-15_00.heatmap   # heatmap sidecar
  rpglot_2026-02-15_01.zst
  rpglot_2026-02-15_01.heatmap
  ...
  wal.log                         # текущие снапшоты до flush в chunk
```

### Chunk format (RPG6)

```
┌──────────────────────────────────────┐
│ HEADER (48 bytes)                    │  magic "RPG6", snapshot_count,
│                                      │  dict/interner offsets
├──────────────────────────────────────┤
│ INDEX TABLE (28 bytes × N)           │  offset, compressed_len, timestamp
├──────────────────────────────────────┤
│ DICTIONARY (~64–112 KB)              │  zstd trained dictionary
├──────────────────────────────────────┤
│ SNAPSHOT FRAMES                      │  zstd_with_dict(postcard(Snapshot))
│   frame_0, frame_1, ... frame_N     │
├──────────────────────────────────────┤
│ INTERNER FRAME                       │  zstd(postcard(StringInterner))
└──────────────────────────────────────┘
```

Random access к любому снапшоту: прочитать header+index (один раз), seek к offset[N], decompress с dictionary.

### WAL

Текущие снапшоты пишутся в `wal.log` с CRC32 framing + fsync. При падении — recovery: валидация CRC, truncate повреждённого хвоста. Flush в chunk каждый час или при 360 записях.

### Heatmap (HM04)

Sidecar файл `.heatmap` — 15 байт на снапшот: active_sessions, cpu%, cgroup metrics, error counts, checkpoint/autovacuum events, health score. Позволяет отрисовать timeline без декомпрессии снапшотов.

### StringInterner

Дедупликация строк через xxh3 хеширование. Все строковые поля (query, database, user, cmdline) хранятся как `u64` хеш. В WAL — filtered interner (только хеши текущего снапшота). В chunk — объединённый interner всех снапшотов.

### Ротация

```bash
rpglotd --max-size 2G --max-days 14
```

Удаление старейших `.zst` + `.heatmap` по возрасту или суммарному размеру.

---

## Rate Computation

Единый модуль `rates.rs` — source of truth для TUI и Web.

```
rates.rs
├── PgsRateState  (pg_stat_statements)    merge + stale eviction (300s)
├── PgpRateState  (pg_store_plans)        merge + stale eviction (900s)
├── PgtRateState  (pg_stat_user_tables)   full replace
└── PgiRateState  (pg_stat_user_indexes)  full replace
```

Rates вычисляются из дельт кумулятивных счётчиков PostgreSQL. Используется `collected_at` из данных (не `snapshot.timestamp`), т.к. коллектор кеширует данные. Если `collected_at` не изменился — skip (данные те же).

MAX_RATE_DT_SECS = 605s (PGS/PGT/PGI), MAX_PGP_RATE_DT_SECS = 905s (PGP) — cap для предотвращения мусорных rates после длинных пауз. При counter regression (pg_stat_statements_reset) rates обнуляются.

---

## Data Model

### Snapshot

```rust
struct Snapshot {
    timestamp: i64,           // Unix epoch seconds
    blocks: Vec<DataBlock>,   // гетерогенные блоки данных
}
```

### DataBlock (27 вариантов)

**Процессы:** `Processes(Vec<ProcessInfo>)`

**PostgreSQL instance-level:**
- `PgStatActivity`, `PgStatStatements`, `PgStorePlans`
- `PgStatDatabase`, `PgStatBgwriter`, `PgLockTree`
- `PgStatProgressVacuum`, `PgLogErrors`, `PgLogEvents`, `PgLogDetailedEvents`
- `PgSettingEntries`, `ReplicationStatus`

**PostgreSQL per-database:**
- `PgStatUserTables`, `PgStatUserIndexes`

**System:**
- `SystemCpu`, `SystemLoad`, `SystemMem`, `SystemNet`, `SystemDisk`
- `SystemPsi`, `SystemVmstat`, `SystemFile`
- `SystemInterrupts`, `SystemSoftirqs`, `SystemStat`, `SystemNetSnmp`

**Container:**
- `Cgroup`

Не все блоки присутствуют в каждом снапшоте. PgSettings — раз в час. Cgroup — только в контейнерах.

---

## Provider

`SnapshotProvider` trait абстрагирует источник данных:

- **LiveProvider** — коллектор + таймер, отдаёт свежие снапшоты каждый tick
- **HistoryProvider** — читает .zst чанки и WAL с диска, навигация по timeline

HistoryProvider строит index лениво (при первом запросе): сканирует headers чанков + WAL metadata без декомпрессии снапшотов. В памяти ~8 байт × N timestamps + metadata чанков. 100K снапшотов ≈ 1–2 MB.

---

## rpglot-web

### Backend (axum)

```
main.rs
├── WebAppInner              # Shared state: provider, rate states, cache
├── tick_loop()              # Live mode: collect → convert → broadcast SSE
├── reconvert_current()      # History mode: load snapshot → compute rates → convert
├── /api/v1/snapshot         # GET: текущий или по timestamp
├── /api/v1/schema           # GET: metadata колонок (units, thresholds)
├── /api/v1/stream           # SSE: live snapshots
├── /api/v1/timeline         # GET: metadata (dates, total)
├── /api/v1/timeline/heatmap # GET: bucketed heatmap data
├── /api/v1/analysis         # GET: anomaly detection results
└── /swagger-ui/             # OpenAPI docs
```

Auth: Basic Auth или SSO Proxy (JWT).

### Frontend (React SPA, embedded)

```
frontend/src/
├── api/client.ts            # HTTP + SSE клиент
├── hooks/
│   ├── useSnapshot.ts       # Live (SSE) + History (fetch)
│   ├── useTabState.ts       # State 9 табов (selection, filters, views)
│   ├── useSchema.ts         # Schema кеш
│   └── useUrlState.ts       # URL ↔ state sync
├── components/
│   ├── Header.tsx           # DB selector, live/history, play controls, theme
│   ├── SummaryPanel.tsx     # CPU, Memory, Disk, Network, PG status cards
│   ├── Timeline.tsx         # Slider + heatmap + playback
│   ├── TabBar.tsx           # PRC, PGA, PGS, PGP, PGT, PGI, PGE, PGL
│   ├── DataTable.tsx        # @tanstack/react-table, sort, filter, views
│   ├── DetailPanel.tsx      # Row details + SQL + drill-down
│   └── AnalysisModal.tsx    # Anomaly reports, incidents
└── utils/
    ├── formatters.ts        # Bytes, duration, rates
    ├── thresholds.ts        # Color coding (red/yellow/green/gray)
    └── aggregation.ts       # Group by schema/database
```

**Stack:** React 19, TypeScript, Vite 7, TailwindCSS 4, TanStack Table v8.
**State:** React hooks + URL sync (без Redux/Zustand).

---

## rpglot (TUI)

Терминальный интерфейс на ratatui 0.29. Те же данные, что и в Web.

Табы: PRC (процессы), PGA (activity), PGS (statements), PGP (store plans), PGT (tables), PGI (indexes), PGE (events/errors), PGL (locks).

Каждый таб имеет view modes (например PGT: I/O, Reads, Writes, Scans, Maintenance, Schema, Database). NavigableTable trait унифицирует навигацию (up/down/page/home/end) для всех табов.

Selection tracking по entity ID (PID, queryid, relid, indexrelid) — переживает пересортировку. Detail popups закрываются если сущность пропала из данных.

---

## rpglotd (daemon)

```bash
rpglotd -i 10 -o /var/lib/rpglot --max-size 2G --max-days 14
```

Каждые N секунд: `collect_snapshot()` → WAL append (fsync) → flush chunk каждый час → rotation. Memory management через jemalloc arena purge после flush.

---

## rpglotd-dump

Утилита для инспекции хранилища: показывает содержимое .zst чанков, .heatmap файлов, WAL. С флагом `--blocks` выводит размеры каждого DataBlock.

---

## Ключевые паттерны

### String interning
Все строки (query, database, cmdline) хранятся как `u64` хеш, сами строки — в отдельном StringInterner. Экономит место при серлиализации и сравнении.

### Activity-only filtering
Коллектор отправляет в снапшот только строки с изменёнными счётчиками. Уменьшает размер на ~80% для стабильных workloads.

### Stale merge
PGS/PGP: prev_sample сохраняется между снапшотами (merge, не replace). Записи, пропавшие из текущего снапшота, показываются как "stale" (серые, без rates) до eviction timeout.

### Diff tracking
TUI подсвечивает жёлтым строки, изменившиеся с прошлого снапшота. Per-column change tracking через `DiffStatus`.

---

## Build

```bash
cargo build --release     # все бинарники
```

Release profile: LTO, `opt-level = "s"`, strip symbols, `panic = "abort"`.

Frontend: `cd crates/rpglot-web/frontend && npm run build` — собирается в `dist/`, embedded через `rust-embed`.
