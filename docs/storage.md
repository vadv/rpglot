# Storage Architecture

## Цели дизайна

1. **Минимальная память при навигации** — UI перемещается по timeline, читая один снапшот за раз, без загрузки всего файла в память
2. **Random access** — O(1) доступ к любому снапшоту по timestamp через индекс в начале файла
3. **S3-ready** — формат совместим с HTTP Range requests: header+index, dictionary, snapshot frame — три отдельных запроса
4. **Компактность** — zstd с обученным dictionary даёт высокий коэффициент сжатия для повторяющихся структур

## Файловая структура на диске

```
/var/lib/rpglot/
  rpglot_2026-02-15_00.zst       # chunk: 00:00-00:59
  rpglot_2026-02-15_00.heatmap   # heatmap sidecar
  rpglot_2026-02-15_01.zst
  rpglot_2026-02-15_01.heatmap
  ...
  rpglot_2026-02-15_23.zst
  rpglot_2026-02-15_23.heatmap
  wal.log                         # текущий WAL (ещё не flushed в chunk)
```

Файлы именуются `rpglot_YYYY-MM-DD_HH.zst`. Каждый chunk содержит данные за один час. При collision (повторный flush в тот же час) добавляется суффикс с nanosecond timestamp.

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
| INDEX TABLE (snapshot_count x 28 bytes)          |
|   Per entry:                                      |
|     offset: u64        (byte position in file)    |
|     compressed_len: u64                           |
|     timestamp: i64     (epoch seconds)            |
|     uncompressed_len: u32                         |
+--------------------------------------------------+
| DICTIONARY (raw bytes, ~64-112 KB)               |
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

**Ключевые решения:**

- **Index в начале файла** (после фиксированного 48-байтного header). Это позволяет прочитать header + index одним Range request и узнать timestamp и offset каждого снапшота без чтения данных.
- **Dictionary перед снапшотами**. Загружается один раз, используется для декомпрессии всех snapshot frames в файле.
- **Каждый снапшот — отдельный zstd frame**. Для чтения снапшота #N нужно: seek к offset[N], прочитать compressed_len[N] байт, decompress с dictionary.
- **StringInterner в конце**, сжат без dictionary. Содержит только строки, используемые в снапшотах этого chunk.

### Random access: чтение одного снапшота

```
1. Read header (48 bytes) → узнаём snapshot_count, dict_offset, dict_len
2. Read index (snapshot_count × 28 bytes) → массив (offset, len, timestamp)
3. Read dictionary (dict_offset..dict_offset+dict_len) → prepared dict
4. Найти нужный snapshot по timestamp в index
5. Read snapshot frame (offset..offset+compressed_len)
6. Decompress with prepared dictionary
7. Deserialize postcard → Snapshot
```

Шаги 1-3 выполняются один раз при открытии chunk. Шаг 5-7 — для каждого снапшота.

### S3-совместимость

Текущий формат полностью совместим с S3 Range requests:

| Операция | Range request |
|----------|--------------|
| Header + index | `Range: bytes=0-{48 + count*28}` |
| Dictionary | `Range: bytes={dict_offset}-{dict_offset+dict_len}` |
| Snapshot N | `Range: bytes={offset[N]}-{offset[N]+len[N]}` |
| Interner | `Range: bytes={int_offset}-{int_offset+int_len}` |

Не требуется скачивание всего файла. Навигация по одному chunk — 1 HTTP запрос на снапшот (после начальной загрузки index + dict).

## WAL format (CRC32 framing)

Текущие снапшоты записываются в `wal.log` до flush в chunk. Каждая запись:

```
+----------------------------+
| length: u32 LE  (4 bytes)  |
| crc32: u32 LE   (4 bytes)  |
| payload: [u8]   (length B) |
|   postcard(WalEntry {      |
|     snapshot,               |
|     interner (filtered)     |
|   })                        |
+----------------------------+
```

**Recovery при старте:**
- Читает WAL последовательно, проверяя CRC32 каждой записи
- При первом несовпадении CRC или неполной записи — truncate WAL до последней валидной позиции
- Восстановленные снапшоты доступны для просмотра

**Flush в chunk:**
- Происходит при смене часа (calendar hour boundary) или при достижении 360 снапшотов
- WAL truncate-ится после успешной записи chunk
- Atomic write: chunk пишется во временный файл, затем rename

## Heatmap format (HM01)

Sidecar файл `.heatmap` рядом с каждым `.zst` chunk. Позволяет отрисовать timeline без декомпрессии снапшотов.

```
+----------------------------------------+
| magic: "HM01" (4 bytes)               |
| entries: [HeatmapEntry] (12 bytes ea) |
+----------------------------------------+
```

Каждый `HeatmapEntry` (12 bytes):

| Поле | Тип | Описание |
|------|-----|----------|
| active_sessions | u16 | Активные PGA сессии (state != idle) |
| cpu_pct_x10 | u16 | CPU% хоста x10 (0-1000) |
| cgroup_cpu_pct_x10 | u16 | CPU% cgroup x10 |
| cgroup_mem_pct_x10 | u16 | Memory% cgroup x10 |
| error_count | u16 | PG ошибки (ERROR+FATAL+PANIC) |
| checkpoint_count | u8 | Checkpoint events |
| autovacuum_count | u8 | Autovacuum/autoanalyze events |

**Bucketing для frontend:**
- Frontend запрашивает heatmap за временной диапазон
- Сервер агрегирует entries в N бакетов (default 400)
- Метрики агрегируются по MAX (пиковые значения), события — по SUM
- Результат отрисовывается как цветная полоса под timeline

**Per-date кэширование:**
- Heatmap для прошлых дат неизменен (immutable) — вычисляется один раз при flush chunk
- Heatmap для текущего дня пересчитывается по мере поступления новых снапшотов

## StringInterner

Дедупликация строк через xxh3_64 хеширование. Часто повторяющиеся строки (имена таблиц, SQL запросы, имена пользователей) хранятся один раз.

- В снапшотах строки заменены на u64 hash
- Interner хранит mapping hash → string
- В WAL: каждая запись содержит filtered interner (только хеши из этого снапшота)
- В chunk: один interner frame с объединением всех хешей

## Ротация

| Параметр | Default | Описание |
|----------|---------|----------|
| `--max-size` | 1 GB | Максимальный суммарный размер всех .zst файлов |
| `--max-days` | 7 | Максимальный возраст файлов |

Ротация выполняется при flush chunk:
1. Удаляются файлы старше `max-days`
2. Если суммарный размер > `max-size` — удаляются самые старые файлы
3. При удалении .zst удаляется и соответствующий .heatmap sidecar
