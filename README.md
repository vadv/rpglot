# rpglot

**rpglot** — инструмент для анализа проблем PostgreSQL, объединяющий системные метрики (CPU, память, диск, сеть) и метрики PostgreSQL (сессии, запросы, таблицы, индексы, блокировки, ошибки) в едином интерфейсе. Написан на Rust.

Основной сценарий использования — **post-mortem анализ**: демон `rpglotd` непрерывно записывает метрики на диск, а `rpglot-web` позволяет просматривать историю через браузер, перемещаясь по timeline и сопоставляя системную нагрузку с поведением PostgreSQL.

---

## Зачем это нужно

Когда PostgreSQL тормозит, причина может быть где угодно: высокий CPU от autovacuum, нехватка памяти и swap thrashing, блокировки между сессиями, плохие запросы без индексов, I/O saturation от checkpoint. Обычно приходится собирать данные из разных источников (`pg_stat_activity`, `pg_stat_statements`, `htop`, `iostat`, логи) и сопоставлять их по времени вручную.

rpglot собирает всё в одном месте и привязывает к единой шкале времени. Можно увидеть что в 14:32 был всплеск CPU, и в это же время в PGA появились 20 active сессий с одним и тем же запросом, а в PGS видно что этот запрос делает seq scan, а в PGT — что таблица давно не vacuum-илась.

---

## Что стоит смотреть в первую очередь

**1. Summary panel** (верхняя панель) — общая картина: CPU, memory, disk I/O, network, load average. Если CPU > 80% или disk util > 90% — это точка входа.

**2. PGA (PostgreSQL Activity)** — активные сессии. Что делает PostgreSQL прямо сейчас? Сколько active сессий? Есть ли long-running запросы? Какие wait events?

**3. PGS (Statements)** — статистика запросов. Какой запрос потребляет больше всего времени? Какой вызывается чаще всего? У какого низкий cache hit ratio?

**4. PGE (Events)** — ошибки и события из лога PostgreSQL. Есть ли ошибки? Как часто и какие checkpoints? Работает ли autovacuum?

**5. PGL (Locks)** — дерево блокировок. Кто кого блокирует?

---

## Три бинарника

### rpglotd — демон сбора метрик

Фоновый процесс, собирает системные и PostgreSQL метрики каждые N секунд и записывает на диск. Запускается рядом с PostgreSQL и работает непрерывно.

```bash
rpglotd                                # интервал 10 сек, директория ./data
rpglotd -i 5 -o /var/lib/rpglot       # интервал 5 сек, кастомная директория
```

| Флаг | Описание | По умолчанию |
|------|----------|-------------|
| `-i, --interval` | Интервал сбора (секунды) | 10 |
| `-o, --output` | Директория хранения | ./data |
| `--max-size` | Максимальный объём файлов | 1G |
| `--max-days` | Максимальный возраст файлов | 7 дней |
| `--postgres` | Сбор PostgreSQL метрик | true |

Данные хранятся в часовых chunk-файлах (postcard + zstd), ротация по размеру и возрасту.

### rpglot-web — веб-интерфейс (основной способ просмотра)

REST API сервер со встроенным React-фронтендом. Основной режим работы — **history**: подключается к директории с данными rpglotd и предоставляет веб-интерфейс для анализа.

```bash
# History mode (основной сценарий)
rpglot-web --history /var/lib/rpglot

# Live mode (сбор + просмотр в одном процессе)
rpglot-web

# Указание адреса
rpglot-web --listen 0.0.0.0:9090 --history /var/lib/rpglot
```

| Флаг | Env var | Описание | По умолчанию |
|------|---------|----------|-------------|
| `--listen` | `RPGLOT_LISTEN` | Адрес:порт | 0.0.0.0:8080 |
| `--history` | `RPGLOT_HISTORY` | Путь к директории с данными | — (live mode) |
| `--interval` | `RPGLOT_INTERVAL` | Интервал для live mode | 1 сек |
| `--auth-user` | `RPGLOT_AUTH_USER` | Basic Auth username | — |
| `--auth-password` | `RPGLOT_AUTH_PASSWORD` | Basic Auth password | — |
| `--sso-proxy-url` | `RPGLOT_SSO_PROXY_URL` | SSO proxy URL | — |

### rpglot — TUI-просмотрщик

Терминальный интерфейс в стиле atop/htop. Работает в live или history режиме.

```bash
rpglot                     # live mode, интервал 1 сек
rpglot -r /var/lib/rpglot  # history mode
rpglot -r -b -1h           # history, начиная с часа назад
```

---

## Вкладки

Переключение: цифры `1`-`7` или `Tab`/`Shift+Tab`.

### PRC — Процессы

Все процессы системы с CPU, памятью, I/O. PostgreSQL процессы подсвечиваются и показывают текущий SQL в колонке CMD.

**View modes:** Generic (`g`), Command (`c`), Memory (`m`), Disk (`d`)

### PGA — PostgreSQL Activity

Активные сессии из `pg_stat_activity`, обогащённые OS-метриками (CPU%, RSS) через привязку по PID.

**View modes:**
- Generic (`g`) — PID, CPU%, RSS, DB, User, State, Wait Event, Query Duration, Query
- Stats (`v`) — обогащение данными из pg_stat_statements: Mean/Max время, Calls/s, HIT%

**Подсветка аномалий в Stats режиме:**
- Жёлтый: QDUR > 2x Mean или HIT% < 80%
- Красный: QDUR > 5x Mean, QDUR > Max или HIT% < 50%

**Фильтры (web):** скрытие idle и system сессий

### PGS — PostgreSQL Statements

TOP 500 запросов из `pg_stat_statements` по total_exec_time. Все метрики — rate (/s), вычисленные из дельт между снимками.

**View modes:** Time (`t`), Calls (`c`), I/O (`i`), Temp (`e`)

### PGT — PostgreSQL Tables

Статистика по таблицам из `pg_stat_user_tables` + `pg_statio_user_tables`. Собирается со **всех доступных баз** на инстансе.

**View modes:** I/O (default), Reads, Writes, Scans, Maintenance, Schema, Database

View modes Schema и Database — клиентская агрегация: суммируют метрики всех таблиц по имени схемы или базы данных.

### PGI — PostgreSQL Indexes

Статистика по индексам из `pg_stat_user_indexes` + `pg_statio_user_indexes`. Также собирается со всех баз.

**View modes:** I/O (default), Usage, Unused, Schema, Database

View Unused — индексы с нулём сканирований, кандидаты на удаление.

### PGE — PostgreSQL Events

События из лога PostgreSQL: ошибки, checkpoints, autovacuum/autoanalyze.

**View modes:** Errors, Checkpoints, Autovacuum

Checkpoints view показывает timing, buffers written, WAL files, sync duration. Autovacuum view — детали каждого autovacuum/autoanalyze.

### PGL — PostgreSQL Locks

Дерево блокировок: кто кого блокирует. Отображается как плоская таблица с индентацией по глубине.

---

## Drill-down навигация

Клавиша `>` или `J` позволяет провалиться от общего к частному:

```
PRC (процессы) → PGA (сессии) → PGS (статистика запросов)
PGT (таблицы)  → PGI (индексы этой таблицы)
PGL (locks)    → PGA (сессия, держащая lock)
```

**Пример:** В PRC видите процесс postgres с CPU 95%. Нажимаете `>` — попадаете в PGA к этой сессии. Видите active запрос с QDUR 45s. Нажимаете `>` — попадаете в PGS и видите что этот запрос делает 0% cache hits и 50k shared_blks_read/s.

---

## Summary panel

Верхняя панель с общей картиной системы:

| Секция | Что показывает |
|--------|---------------|
| CPU | usr%, sys%, irq%, iow%, steal%, idle% |
| LOAD | load average 1/5/15 мин |
| MEM | total, available, buffers, cache, slab |
| SWP | total, free, dirty, writeback |
| DSK | per-disk: read/write MB/s, IOPS, util% |
| NET | per-interface: RX/TX MB/s, packets, errors |
| PSI | Pressure Stall Information (CPU, Memory, I/O) |
| VMS | vmstat rates: pgin/pgout/swin/swout/ctx/s |
| PG | connections, transactions, commits, rollbacks |
| BGWR | checkpoints, buffers written |
| CGROUP | CPU/Memory/PIDs limits и usage (в контейнерах) |

Цветовая индикация: жёлтый — warning, красный — critical.

---

## Веб-интерфейс

### Timeline и heatmap

В history mode доступна шкала времени с heatmap — визуализация активности за каждый час. Тёмные области = высокая нагрузка. Клик по heatmap переносит к нужному моменту.

**Навигация:**
- `←/→` — шаг по снимкам
- `Shift+←/→` — шаг ±1 час
- Календарь для выбора даты
- Прямой ввод времени

### Health Score

Индикатор здоровья системы 0-100 в header. Учитывает количество active сессий, CPU, disk I/O. Зелёный (80+), жёлтый (50-79), красный (<50).

### Detail panel

Клик по строке + Enter — боковая панель с детальной информацией. Для PGA показывает полный текст запроса, для PGS — breakdown по timing и I/O, для PRC — полную информацию о процессе включая /proc/pid/io.

### Темы и timezone

- Dark / Light / System theme
- Timezone: LOCAL / UTC / Moscow

### Keyboard shortcuts (web)

| Клавиша | Действие |
|---------|----------|
| `1`-`7` | Переключение табов |
| `?` | Справка |
| `Space` | Пауза (live mode) |
| `←/→` | Навигация по истории |
| `j/k` | Перемещение по строкам |
| `Enter` | Открыть detail panel |
| `Escape` | Закрыть detail / снять выделение |
| `/` | Фильтр |

---

## Собираемые метрики

### Системные (Linux /proc)

- **Процессы**: PID, state, CPU time (user/system), memory (VSZ/RSS/PSS/swap), I/O (read/write bytes), context switches, threads
- **CPU**: per-core usage, context switches, interrupts
- **Memory**: total, free, available, buffers, cache, slab, dirty, writeback
- **Swap**: total, free, dirty
- **Disk**: per-device read/write bytes и ops, util%
- **Network**: per-interface RX/TX bytes и packets, errors, drops
- **Load average**: 1m, 5m, 15m
- **PSI**: CPU/Memory/I/O pressure stall
- **Vmstat**: paging, context switches, faults
- **Cgroup**: CPU/Memory/PIDs limits и usage (автоопределение контейнера)

### PostgreSQL

- **pg_stat_activity**: все backends — state, wait events, query, duration
- **pg_stat_statements**: TOP 500 запросов — timing, calls, I/O, temp, WAL
- **pg_stat_user_tables**: per-table — scans, reads, writes, vacuum, I/O (все базы)
- **pg_stat_user_indexes**: per-index — scans, usage, I/O (все базы)
- **pg_stat_database**: connections, transactions, commits, rollbacks
- **pg_stat_bgwriter**: checkpoints, buffers written
- **pg_locks**: lock types, holders, waiters
- **PostgreSQL log**: errors, checkpoints, autovacuum events

**Минимальная версия PostgreSQL: 10**

---

## Мультибазовая коллекция

Метрики таблиц (PGT) и индексов (PGI) собираются со **всех доступных баз** на PostgreSQL инстансе. Коллектор автоматически поддерживает пул подключений ко всем базам, обновляя список каждые 10 минут.

Колонка Database в PGT/PGI показывает из какой базы пришла каждая строка. View modes Database и Schema агрегируют данные на клиенте.

---

## Хранение данных

Данные записываются в часовые chunk-файлы:

```
/var/lib/rpglot/
  2026-02-16.00.chunk
  2026-02-16.01.chunk
  ...
  wal.log              # текущий WAL до flush в chunk
  heatmap.bin          # кэш heatmap для timeline
```

**Формат chunk:** postcard (binary serialization) + zstd (dictionary compression). Random access по индексу в footer файла — чтение конкретного снапшота без распаковки всего файла.

**Ротация:** по `--max-size` (default 1G) и `--max-days` (default 7). Старые файлы удаляются автоматически.

---

## Аутентификация

### Basic Auth

```bash
rpglot-web --auth-user admin --auth-password secret --history /data
```

### SSO Proxy (JWT)

```bash
rpglot-web \
  --sso-proxy-url https://sso.example.com/oauth2/start \
  --sso-proxy-key-file /etc/rpglot/sso-public.pem \
  --sso-proxy-audience rpglot \
  --sso-proxy-allowed-users "alice,bob" \
  --history /data
```

---

## Сборка

```bash
cargo build --release
```

Бинарники: `target/release/rpglotd`, `target/release/rpglot-web`, `target/release/rpglot`.

Версия включает git SHA: `rpglotd --version` → `0.1.9-abc1234`.

---

## Типичный deployment

```bash
# На сервере с PostgreSQL:

# 1. Запустить демон сбора (systemd unit)
rpglotd -i 10 -o /var/lib/rpglot --max-size 2G --max-days 14

# 2. Запустить веб-сервер для просмотра истории
rpglot-web --history /var/lib/rpglot --listen 0.0.0.0:8080
```

Веб-интерфейс доступен на `http://server:8080`. Навигация по timeline, анализ любого момента в прошлом.

---

## Лицензия

MIT
