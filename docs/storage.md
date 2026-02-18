# Storage Architecture

## Цели дизайна

1. **Минимальная память при навигации** — UI перемещается по timeline, читая один снапшот за раз, без загрузки всего файла в память
2. **Random access** — O(1) доступ к любому снапшоту по timestamp через индекс в начале файла
3. **S3-ready** — формат совместим с HTTP Range requests: header+index, dictionary, snapshot frame — три отдельных запроса
4. **Компактность** — zstd с обученным dictionary даёт высокий коэффициент сжатия для повторяющихся структур
5. **SIGKILL resilience** — WAL с CRC32 framing гарантирует восстановление после аварийного завершения

## Файловая структура на диске

```
/var/lib/rpglot/
  rpglot_2026-02-15_00.zst       # chunk: 00:00–00:59
  rpglot_2026-02-15_00.heatmap   # heatmap sidecar
  rpglot_2026-02-15_01.zst
  rpglot_2026-02-15_01.heatmap
  ...
  rpglot_2026-02-15_23.zst
  rpglot_2026-02-15_23.heatmap
  wal.log                         # текущий WAL (ещё не flushed в chunk)
```

Файлы именуются `rpglot_YYYY-MM-DD_HH.zst`. Каждый chunk содержит данные за один час (~360 снапшотов при 10-секундном интервале). При collision (повторный flush в тот же час) добавляется суффикс с nanosecond timestamp.

## Модули

```
src/storage/
├── mod.rs              # Module exports
├── storage.md          # Внутренняя документация модуля
├── interner.rs         # Дедупликация строк через XXH3 хеширование
├── chunk.rs            # Chunk формат RPG3 (zstd + dictionary + index)
├── heatmap.rs          # Heatmap формат HM03 (timeline sidecar)
├── manager.rs          # StorageManager: WAL, chunks, flush, rotation
└── model/
    ├── mod.rs          # Re-exports всех моделей
    ├── process.rs      # Метрики процессов (/proc/[pid]/)
    ├── system.rs       # Системные метрики (/proc/*)
    ├── postgres.rs     # Метрики PostgreSQL (pg_stat_*)
    ├── cgroup.rs       # Метрики cgroup v2
    └── snapshot.rs     # Snapshot, DataBlock enum
```

## Chunk format (RPG3)

```
+--------------------------------------------------+
| HEADER (48 bytes)                                |
|   magic: "RPG3" (4 bytes)                        |
|   version: u16 = 3                               |
|   snapshot_count: u16                             |
|   interner_offset: u64                            |
|   interner_compressed_len: u64                    |
|   dict_offset: u64                                |
|   dict_len: u64                                   |
|   _reserved: 4 bytes                              |
+--------------------------------------------------+
| INDEX TABLE (snapshot_count × 28 bytes)          |
|   Per entry:                                      |
|     offset: u64        (byte position in file)    |
|     compressed_len: u64                           |
|     timestamp: i64     (epoch seconds)            |
|     uncompressed_len: u32                         |
+--------------------------------------------------+
| DICTIONARY (raw bytes, ~64–112 KB)               |
|   zstd trained dictionary                         |
+--------------------------------------------------+
| SNAPSHOT FRAMES                                   |
|   zstd_with_dict(postcard(Snapshot_0))           |
|   zstd_with_dict(postcard(Snapshot_1))           |
|   ...                                             |
+--------------------------------------------------+
| INTERNER FRAME                                    |
|   zstd(postcard(StringInterner))                 |
+--------------------------------------------------+
```

**Константы (chunk.rs):**
- `MAGIC = b"RPG3"`, `VERSION = 3`
- `HEADER_SIZE = 48`, `INDEX_ENTRY_SIZE = 28`
- `DICT_MAX_SIZE = 112 KB`

**Ключевые решения:**

- **Index в начале файла** (после фиксированного header). Позволяет прочитать header + index одним Range request и узнать timestamp/offset каждого снапшота без чтения данных.
- **Dictionary перед снапшотами**. Загружается один раз, используется для декомпрессии всех snapshot frames.
- **Каждый снапшот — отдельный zstd frame**. Для чтения снапшота #N: seek к offset[N], прочитать compressed_len[N] байт, decompress с dictionary.
- **StringInterner в конце**, сжат без dictionary. Содержит только строки из снапшотов этого chunk.
- **Serialization**: postcard (не bincode, не JSON) — компактный, стабильный, без length prefix.
- **Atomic writes**: chunk пишется в `.tmp` файл, затем rename.

### Random access: чтение одного снапшота

```
1. Read header (48 bytes) → snapshot_count, dict_offset, dict_len
2. Read index (snapshot_count × 28 bytes) → массив (offset, len, timestamp)
3. Read dictionary → zstd::dict::DecoderDictionary
4. Найти snapshot по timestamp в index
5. Read snapshot frame (offset..offset+compressed_len)
6. Decompress with dictionary
7. Deserialize postcard → Snapshot
```

Шаги 1–3 выполняются один раз при открытии chunk (ChunkReader::open). Шаги 5–7 — для каждого снапшота (ChunkReader::read_snapshot).

### ChunkReader API

```rust
ChunkReader::open(path) -> io::Result<Self>     // Читает весь файл, парсит header/index/dict
  .snapshot_count() -> usize
  .timestamps() -> Vec<i64>                       // Из index, без декомпрессии
  .read_snapshot(idx: usize) -> io::Result<Snapshot>
  .read_interner() -> io::Result<StringInterner>
```

### S3-совместимость

| Операция | Range request |
|----------|--------------|
| Header + index | `bytes=0-{48 + count*28}` |
| Dictionary | `bytes={dict_offset}-{dict_offset+dict_len}` |
| Snapshot N | `bytes={offset[N]}-{offset[N]+len[N]}` |
| Interner | `bytes={int_offset}-{int_offset+int_len}` |

## WAL format (CRC32 framing)

Текущие снапшоты записываются в `wal.log` до flush в chunk.

### Frame structure

```
+----------------------------+
| length: u32 LE  (4 bytes)  |
| crc32:  u32 LE  (4 bytes)  |
| payload: [u8]   (length B) |
|   postcard(WalEntry {      |
|     snapshot,               |
|     interner (filtered)     |
|   })                        |
+----------------------------+
```

**Константы:**
- `WAL_FRAME_HEADER_SIZE = 8`
- `MAX_WAL_ENTRY_SIZE = 256 MB` (sanity check)
- CRC algorithm: `crc32fast::hash()` (CRC-32 IEEE 802.3)

### Запись

```rust
let encoded = postcard::to_allocvec(&entry)?;
file.write_all(&(encoded.len() as u32).to_le_bytes())?;
file.write_all(&crc32fast::hash(&encoded).to_le_bytes())?;
file.write_all(&encoded)?;
file.sync_all()?;  // fsync после каждого фрейма
```

### Recovery при старте

1. Читает WAL последовательно
2. Для каждого фрейма: проверяет length (< MAX), CRC32, postcard десериализацию
3. При первом несовпадении — truncate WAL до последней валидной позиции
4. Восстановленные снапшоты доступны для просмотра

### Lazy metadata scan

```rust
StorageManager::scan_wal_metadata(wal_path) -> Vec<(byte_offset, frame_length, timestamp)>
```
Читает только заголовки фреймов + timestamps без десериализации полных снапшотов. Используется HistoryProvider для построения индекса.

```rust
StorageManager::load_wal_snapshot_at(wal_path, offset, length) -> Snapshot
```
Загружает один снапшот по offset/length из scan_wal_metadata.

## Heatmap format (HM03)

Sidecar файл `.heatmap` рядом с каждым `.zst` chunk. Позволяет отрисовать timeline без декомпрессии снапшотов.

```
+------------------------------------------+
| magic: "HM03" (4 bytes)                 |
| entries: [HeatmapEntry] (14 bytes each) |
+------------------------------------------+
```

### HeatmapEntry (14 bytes, little-endian)

| Offset | Тип | Поле | Описание |
|--------|-----|------|----------|
| 0–1 | u16 | active_sessions | Активные PGA сессии (state != idle) |
| 2–3 | u16 | cpu_pct_x10 | CPU% хоста × 10 (0–1000) |
| 4–5 | u16 | cgroup_cpu_pct_x10 | CPU% cgroup × 10 |
| 6–7 | u16 | cgroup_mem_pct_x10 | Memory% cgroup × 10 |
| 8 | u8 | errors_critical | PG ошибки: resource + corruption + system |
| 9 | u8 | errors_warning | PG ошибки: timeout + connection + auth + syntax + other |
| 10 | u8 | errors_info | PG ошибки: lock + constraint + serialization |
| 11 | u8 | checkpoint_count | Checkpoint events |
| 12 | u8 | autovacuum_count | Autovacuum/autoanalyze events |
| 13 | u8 | slow_query_count | Slow query events |

### Классификация ошибок по severity

```
Critical (errors_critical): Resource, DataCorruption, System
Warning  (errors_warning):  Timeout, Connection, Auth, Syntax, Other
Info     (errors_info):     Lock, Constraint, Serialization
```

### Источники событий (приоритет)

1. `PgLogDetailedEvents` (source-of-truth, parsed fields)
2. `PgLogEvents` (legacy fallback, простые счётчики)

### Bucketing для frontend

```rust
bucket_heatmap(entries: &[(i64, HeatmapEntry)], start_ts, end_ts, num_buckets) -> Vec<HeatmapBucket>
```

- Метрики (CPU%, memory%, sessions, errors): **MAX** внутри бакета (пиковые)
- События (checkpoints, autovacuums, slow queries): **SUM** внутри бакета
- Первый снапшот в chunk: CPU% = 0 (нет предыдущих данных для дельты)

### Автоматическое восстановление

Если `.heatmap` файл отсутствует или magic не совпадает (старый HM02), HistoryProvider автоматически пересоздаёт его из снапшотов chunk.

## StringInterner

Дедупликация строк через xxh3_64 хеширование (`xxhash_rust` crate).

```rust
#[derive(Default, Serialize, Deserialize, Clone)]
pub struct StringInterner {
    strings: HashMap<u64, String>,
}
```

**API:**
```rust
intern(&mut self, s: &str) -> u64         // hash → store → return hash
resolve(&self, hash: u64) -> Option<&str> // lookup
merge(&mut self, other: &StringInterner)  // объединить два interner
filter(&self, used: &HashSet<u64>) -> StringInterner  // оставить только нужные
```

**Стратегия хранения:**
- В WAL: каждый entry содержит filtered interner (только хеши из этого снапшота)
- В chunk: один interner frame с объединением всех хешей chunk'а
- В HistoryProvider: кешируется один interner последнего открытого chunk'а (~50 KB)

## StorageManager

```rust
pub struct StorageManager {
    base_path: PathBuf,
    chunk_size_limit: usize,        // default: 360 (~1 hour)
    wal_file: File,
    wal_entries_count: usize,
    current_hour: Option<u32>,
    current_date: Option<NaiveDate>,
}
```

### Жизненный цикл записи

```
add_snapshot(snapshot, interner)
  ├── Проверка hour boundary → flush если час сменился
  ├── Filter interner → только хеши из этого снапшота
  ├── Write WalEntry в wal.log (CRC32 framing, fsync)
  ├── wal_entries_count++
  └── Если count >= 360 → flush_chunk()

flush_chunk()
  ├── Прочитать все WalEntry из wal.log
  ├── Build filtered interner (объединение всех entries)
  ├── chunk::write_chunk() → .tmp → rename → .zst (atomic)
  ├── heatmap::write_heatmap() → .heatmap sidecar
  └── Truncate wal.log, reset count
```

### Hourly segmentation

- `rpglot_YYYY-MM-DD_HH.zst` — данные за один календарный час
- Flush при смене часа (current_hour != prev_hour)
- Также flush при достижении chunk_size_limit (360 entries)

### Interner optimization

```rust
collect_snapshot_hashes(snapshot: &Snapshot) -> HashSet<u64>
```
Обходит все DataBlock варианты, собирает все `*_hash` поля. Используется для фильтрации interner — в WAL entry и в chunk попадают только реально использованные строки.

## Snapshot и DataBlock

```rust
pub struct Snapshot {
    pub timestamp: i64,          // Unix epoch seconds
    pub blocks: Vec<DataBlock>,  // Гетерогенные блоки данных
}
```

### DataBlock enum (25 вариантов)

| Вариант | Источник | Тип |
|---------|----------|-----|
| **OS Processes** | | |
| `Processes(Vec<ProcessInfo>)` | `/proc/[pid]/*` | per-process |
| **PostgreSQL instance-level** | | |
| `PgStatActivity(Vec<PgStatActivityInfo>)` | `pg_stat_activity` | per-session |
| `PgStatStatements(Vec<PgStatStatementsInfo>)` | `pg_stat_statements` | per-query |
| `PgStatDatabase(Vec<PgStatDatabaseInfo>)` | `pg_stat_database` | per-database |
| `PgStatBgwriter(PgStatBgwriterInfo)` | `pg_stat_bgwriter` | singleton |
| `PgLockTree(Vec<PgLockTreeNode>)` | `pg_locks` + CTE | per-lock-chain |
| `PgStatProgressVacuum(Vec<PgStatProgressVacuumInfo>)` | `pg_stat_progress_vacuum` | per-vacuum |
| `PgLogErrors(Vec<PgLogEntry>)` | PG log files | grouped by pattern |
| `PgLogEvents(PgLogEventsInfo)` | PG log files | singleton (legacy counts) |
| `PgLogDetailedEvents(Vec<PgLogEventEntry>)` | PG log files | per-event (source-of-truth) |
| `PgSettings(Vec<PgSettingEntry>)` | `pg_settings` | collected hourly |
| **PostgreSQL per-database** | | |
| `PgStatUserTables(Vec<PgStatUserTablesInfo>)` | `pg_stat_user_tables` | per-table |
| `PgStatUserIndexes(Vec<PgStatUserIndexesInfo>)` | `pg_stat_user_indexes` | per-index |
| **System metrics** | | |
| `SystemCpu(Vec<SystemCpuInfo>)` | `/proc/stat` | per-CPU + aggregate |
| `SystemLoad(SystemLoadInfo)` | `/proc/loadavg` | singleton |
| `SystemMem(SystemMemInfo)` | `/proc/meminfo` | singleton |
| `SystemNet(Vec<SystemNetInfo>)` | `/proc/net/dev` | per-interface |
| `SystemDisk(Vec<SystemDiskInfo>)` | `/proc/diskstats` | per-device |
| `SystemPsi(Vec<SystemPsiInfo>)` | `/proc/pressure/*` | per-resource |
| `SystemVmstat(SystemVmstatInfo)` | `/proc/vmstat` | singleton |
| `SystemFile(SystemFileInfo)` | `/proc/sys/fs/*` | singleton |
| `SystemInterrupts(Vec<SystemInterruptInfo>)` | `/proc/interrupts` | per-IRQ |
| `SystemSoftirqs(Vec<SystemSoftirqInfo>)` | `/proc/softirqs` | per-softirq |
| `SystemStat(SystemStatInfo)` | `/proc/stat` | singleton |
| `SystemNetSnmp(SystemNetSnmpInfo)` | `/proc/net/snmp` | singleton |
| **Container** | | |
| `Cgroup(CgroupInfo)` | `/sys/fs/cgroup/*` | singleton |

Не все блоки присутствуют в каждом снапшоте. PgSettings собирается раз в час. PgStatProgressVacuum отсутствует когда нет активных вакуумов. Cgroup — только в контейнерных окружениях.

### String interning

Все строковые поля в моделях хранятся как `u64` хеш:
- `name_hash`, `cmdline_hash` (процессы)
- `query_hash`, `datname_hash`, `usename_hash`, `state_hash` (PostgreSQL)
- `device_hash`, `irq_hash` (system)

Сами строки хранятся в StringInterner, который сериализуется отдельно.

## HistoryProvider

Обеспечивает навигацию по историческим данным с минимальным потреблением памяти.

### Архитектура

```rust
pub struct HistoryProvider {
    chunks: Vec<ChunkMeta>,              // metadata для .zst файлов
    wal: Option<WalIndex>,               // metadata WAL (lazy load)
    cursor: usize,                       // текущая позиция (0-based)
    total_snapshots: usize,
    timestamps: Vec<i64>,                // отсортированы, 1:1 с позициями
    current_buffer: Option<Snapshot>,
    current_interner: Option<StringInterner>,
    interner_cache: Option<CachedInterner>,  // кеш одного chunk interner
}
```

**Принцип: metadata в RAM, данные на диске.**

| В памяти | Размер | Описание |
|----------|--------|----------|
| timestamps | 8 байт × N | Все timestamps всех снапшотов |
| chunk metadata | ~100 байт × chunks | Пути, counts, offsets |
| WAL metadata | 24 байт × WAL entries | byte_offset, length, timestamp |
| interner cache | ~50 KB | Один chunk interner |
| current snapshot | ~10–100 KB | Текущий загруженный снапшот |

**Типичный профиль:** 100K снапшотов → ~1–2 MB metadata (vs ~500 MB если все в RAM).

### Построение индекса

```rust
fn build_index(storage_path) -> (chunks, wal, total, timestamps)
```

1. Сканирует `.zst` файлы, читает header+index каждого (без снапшотов)
2. Сканирует `wal.log` через `scan_wal_metadata()` (без десериализации payload)
3. Собирает все timestamps, сортирует
4. Каждый chunk и WAL entry получает `global_offset` — позицию первого снапшота

### Навигация

```rust
advance() / rewind()                    // cursor ± 1
jump_to(position)                       // абсолютная позиция
jump_to_timestamp_floor(ts)             // binary search: latest ts <= target
jump_to_timestamp_ceil(ts)              // binary search: earliest ts >= target
snapshot_at(position) -> Snapshot       // без смены cursor
snapshot_with_interner_at(position)     // для analysis модуля
```

### Lazy loading

```
resolve_position(cursor) → SnapshotLocation
  ├── Chunk { chunk_idx, offset_in_chunk }
  └── Wal(wal_idx)

load_from_chunk(chunk_idx, offset)
  ├── ChunkReader::open(path) → read_snapshot(offset)
  └── interner: из кеша (если chunk совпадает) или read_interner()

load_from_wal(wal_idx)
  └── StorageManager::load_wal_snapshot_at(path, byte_offset, byte_length)
```

### Heatmap loading

```rust
load_heatmap_range(start_ts, end_ts) -> Vec<(i64, HeatmapEntry)>
```

1. Для каждого chunk: читает `.heatmap` sidecar (быстро, ~14 × N байт)
2. Fallback: декомпрессирует все снапшоты chunk'а, строит heatmap, кеширует на диск
3. Для WAL entries: строит HeatmapEntry из каждого снапшота (CPU% = 0, нет prev)

### Hot-reload

```rust
refresh(storage_path) -> usize  // количество новых снапшотов
```
Обнаруживает новые `.zst` файлы и обновлённый WAL без перезапуска.

## Ротация

| Параметр | Default | Описание |
|----------|---------|----------|
| `--max-size` | 1 GB | Максимальный суммарный размер .zst файлов |
| `--max-days` | 7 | Максимальный возраст файлов |

```rust
pub struct RotationConfig {
    pub max_total_size: u64,      // default: 1 GB
    pub max_retention_days: u32,  // default: 7 days
}
```

**Алгоритм:**
1. Собрать все `.zst` файлы с датами из имён файлов
2. Сортировать по дате (oldest first)
3. Удалить файлы старше `max_retention_days`
4. Если суммарный размер > `max_total_size` — удалять самые старые
5. При удалении `.zst` удаляется и `.heatmap` sidecar

**Результат:**
```rust
pub struct RotationResult {
    pub files_removed_by_age: usize,
    pub files_removed_by_size: usize,
    pub bytes_freed: u64,
    pub total_size_after: u64,
    pub files_remaining: usize,
}
```

## SIGKILL Resilience

1. **Каждый снапшот пишется в WAL** с `sync_all()` после каждого фрейма
2. **CRC32 верификация** — обнаружение повреждённых записей
3. **Chunk writes atomic** — запись в `.tmp`, затем `rename()`
4. **WAL recovery** — truncate повреждённого хвоста, восстановление валидных записей
5. **Startup cleanup** — удаление orphan `.tmp` файлов

## Backward Compatibility

При добавлении новых полей в модели — использовать `#[serde(default)]` для обратной совместимости со старыми снапшотами.

При смене формата файлов:
- Chunk: смена magic (`RPG3` → следующая версия)
- Heatmap: смена magic (`HM03` → следующая версия). Старые файлы автоматически пересоздаются из снапшотов.

## Memory Management

### WAL-only architecture

StorageManager **не** держит снапшоты в памяти:
- Запись напрямую в WAL на диск
- Delta computation при flush (чтение WAL обратно)
- ~0 bytes на хранение chunk'ов

### Interner optimization

- WAL entry: filtered interner (только хеши одного снапшота)
- Chunk: filtered interner (только хеши всех снапшотов chunk'а)
- После flush: `clear_interner()` + `shrink_to_fit()` в collector
- `release_memory_to_os()` через jemalloc после flush

### Мониторинг

Каждые 60 снапшотов (~10 минут) daemon логирует:
```
Memory stats: collector_interner=X strings, wal_entries=Y
```
