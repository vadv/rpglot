# rpglot

**atop + pgBadger + pg_stat_statements + pg_locks + log analysis — в одном инструменте с навигацией по времени.**

rpglot записывает системные метрики и метрики PostgreSQL каждые 10 секунд. Когда случается инцидент — открываете web-интерфейс, перематываете на нужный момент и видите полную картину: CPU, диск, память, активные запросы, блокировки, ошибки из лога, autovacuum — всё в одном окне.

## Зачем, если есть pgBadger / atop / Grafana?

| Проблема | Как обычно | rpglot |
|----------|-----------|--------|
| "Вчера в 3 ночи база тормозила" | Ищете логи, открываете pgBadger, отдельно смотрите atop, пытаетесь сопоставить по времени | Открыли rpglot-web, кликнули на 03:00, видите всё сразу |
| "Какой запрос съел CPU?" | pg_stat_statements показывает запросы, atop показывает процессы — но связать PID с query нужно вручную | PRC показывает процессы с SQL запросами, один клик — переход в PGA/PGS |
| "Кто кого блокирует?" | `pg_locks` + ручной SQL, или pgAdmin | Вкладка PGL — готовое дерево блокировок с drill-down |
| "Индекс используется?" | `pg_stat_user_indexes` вручную | PGI Unused view — неиспользуемые индексы со всех баз, отсортированные по размеру |
| "Нужен мониторинг" | Prometheus + Grafana + postgres_exporter + node_exporter + настройка дашбордов | `rpglotd` — один бинарник, zero config |

## Ключевые возможности

**Навигация по времени** — heatmap показывает активность за период. Кликните на пик — увидите что происходило в этот момент. Стрелки ←/→ для пролистывания снимков, Shift+←/→ для прыжков на час.

**OS + PostgreSQL в одном окне** — CPU per-core, memory, disk I/O, network, PSI, swap, cgroup + pg_stat_activity, pg_stat_statements, таблицы, индексы, блокировки, ошибки из лога.

**Drill-down между вкладками** — от процесса к сессии PostgreSQL, от сессии к статистике запроса, от таблицы к её индексам, от блокировки к сессии.

**Lock tree** — дерево блокировок: кто корневой блокировщик, кто ждёт, какой lock mode, на каком объекте.

**Anomaly detection** — автоматический анализ: CPU saturation, memory pressure, disk bottleneck, long queries, lock chains, cache misses, dead tuples.

**Multi-database** — таблицы и индексы собираются со всех баз на инстансе, не только с текущей.

**Цветовая подсветка** — каждое значение подсвечено по порогам: красный (проблема), жёлтый (внимание), зелёный (норма), серый (нет активности).

**Zero dependencies** — один статический бинарник на Rust. Нет Python, Perl, Java, Docker, Prometheus, Grafana. `rpglotd` на сервере, `rpglot-web` где удобно.

## Quick Start

```bash
# На сервере с PostgreSQL — запустить демон сбора
rpglotd -o /var/lib/rpglot

# Анализ в браузере (можно на другой машине)
rpglot-web --history /var/lib/rpglot --listen 0.0.0.0:8080
```

Открыть `http://server:8080`.

## Три бинарника

| Бинарник | Назначение |
|----------|-----------|
| `rpglotd` | Демон — собирает метрики каждые 10 сек, пишет на диск (zstd, ~50 MB/день) |
| `rpglot-web` | Веб-сервер — REST API + React UI для анализа |
| `rpglot` | TUI — терминальный интерфейс в стиле atop/htop |

```bash
rpglotd -i 10 -o /var/lib/rpglot --max-size 2G --max-days 14
rpglot-web --history /var/lib/rpglot          # history mode
rpglot-web                                     # live mode (сбор + отображение)
```

## Вкладки

| Вкладка | Источник | Что видно |
|---------|----------|-----------|
| **PRC** | `/proc/[pid]/*` | Все процессы. PG бэкенды обогащены текущим SQL запросом |
| **PGA** | `pg_stat_activity` | Активные сессии: state, wait event, query duration, CPU%, RSS |
| **PGS** | `pg_stat_statements` | TOP 500 запросов: calls/s, time/s, I/O, temp, cache hit% |
| **PGP** | `pg_store_plans` | Планы выполнения запросов (если расширение установлено) |
| **PGT** | `pg_stat_user_tables` | Таблицы со всех баз: I/O, reads, writes, scans, vacuum, dead tuples |
| **PGI** | `pg_stat_user_indexes` | Индексы со всех баз: usage, unused (кандидаты на DROP), I/O |
| **PGE** | PostgreSQL log | Ошибки, checkpoints, autovacuum events |
| **PGL** | `pg_locks` | Дерево блокировок: кто кого блокирует |

Каждая вкладка имеет несколько view modes. Например, PGT: I/O, Reads, Writes, Scans, Maintenance, Schema, Database.

## Что собирается

**OS:** CPU (per-core), memory, swap, disk I/O (per-device), network (per-interface), load average, PSI, vmstat, /proc/[pid]/io, cgroup v2

**PostgreSQL:** pg_stat_activity, pg_stat_statements (TOP 500), pg_store_plans, pg_stat_user_tables, pg_stat_user_indexes, pg_stat_database, pg_stat_bgwriter, pg_stat_progress_vacuum, pg_locks (blocking tree), pg_settings, replication status, PostgreSQL log (errors, checkpoints, autovacuum)

**PostgreSQL 10+.** Version-aware: query_id (PG 14+), plan time (PG 13+), split bgwriter/checkpointer (PG 17+).

## Хранение

Данные в часовых chunk-файлах с zstd-сжатием (~50 MB/день при 10s интервале). Random access к любому снапшоту без декомпрессии всего файла. WAL с CRC32 для crash safety.

```bash
rpglotd --max-size 2G --max-days 14   # ротация по размеру и возрасту
```

## Аутентификация

```bash
rpglot-web --auth-user admin --auth-password secret              # Basic Auth
rpglot-web --sso-proxy-url https://sso.example.com/oauth2/start  # SSO (JWT)
```

## Сборка

```bash
cargo build --release
```

Один бинарник, статическая линковка. Frontend встроен в `rpglot-web`.

## Документация

- [ARCHITECTURE.md](ARCHITECTURE.md) — архитектура проекта, модули, форматы данных

## Лицензия

MIT
