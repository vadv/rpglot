# rpglot

**rpglot** — инструмент для анализа проблем PostgreSQL. Объединяет системные метрики и метрики PostgreSQL в едином интерфейсе с навигацией по времени.

Демон `rpglotd` непрерывно записывает метрики, `rpglot-web` позволяет просматривать историю через браузер. Можно перемотать на момент инцидента и увидеть полную картину: что происходило с CPU, диском, памятью, какие запросы выполнялись, какие сессии были активны, были ли блокировки и ошибки.

---

## Quick Start

```bash
# На сервере с PostgreSQL — запустить демон сбора
rpglotd -o /var/lib/rpglot

# Запустить веб-интерфейс для анализа
rpglot-web --history /var/lib/rpglot --listen 0.0.0.0:8080
```

Открыть `http://server:8080` в браузере.

---

## Три бинарника

| Бинарник | Назначение |
|----------|-----------|
| `rpglotd` | Демон — собирает метрики каждые 10 сек и пишет на диск |
| `rpglot-web` | Веб-сервер — REST API + React фронтенд для анализа в браузере |
| `rpglot` | TUI — терминальный интерфейс в стиле atop/htop |

**rpglot-web** — основной способ просмотра. Работает в двух режимах:
- **history** (основной) — читает файлы с диска, навигация по timeline
- **live** — сбор и отображение метрик в реальном времени

```bash
rpglot-web --history /var/lib/rpglot   # history mode
rpglot-web                              # live mode
```

**rpglotd** — фоновый демон, запускается как systemd unit:

```bash
rpglotd -i 10 -o /var/lib/rpglot --max-size 2G --max-days 14
```

---

## Что собирается

**Система:** CPU (per-core), memory, swap, disk I/O (per-device), network (per-interface), load average, PSI, vmstat, cgroup metrics

**PostgreSQL:** pg_stat_activity, pg_stat_statements (TOP 500), pg_stat_user_tables (все базы), pg_stat_user_indexes (все базы), pg_store_plans, pg_stat_database, pg_stat_bgwriter, pg_locks, PostgreSQL log (errors, checkpoints, autovacuum), replication status

**Минимальная версия PostgreSQL: 10**

---

## Вкладки

| Вкладка | Источник | Назначение |
|---------|----------|-----------|
| PRC | `/proc/[pid]/*` | Все процессы системы с обогащением PG бэкендов |
| PGA | `pg_stat_activity` | Активные сессии PostgreSQL |
| PGS | `pg_stat_statements` | TOP запросов по calls, time, I/O, temp |
| PGP | `pg_store_plans` | Планы выполнения запросов |
| PGT | `pg_stat_user_tables` | Таблицы: I/O, reads, writes, scans, maintenance |
| PGI | `pg_stat_user_indexes` | Индексы: usage, unused, I/O |
| PGE | PostgreSQL log | Ошибки, checkpoints, autovacuum events |
| PGL | `pg_locks` | Дерево блокировок: кто кого блокирует |

Drill-down: PRC → PGA → PGS, PGT → PGI, PGL → PGA.

---

## Аутентификация

```bash
# Basic Auth
rpglot-web --auth-user admin --auth-password secret

# SSO Proxy (JWT)
rpglot-web --sso-proxy-url https://sso.example.com/oauth2/start \
           --sso-proxy-key-file /etc/rpglot/public.pem \
           --sso-proxy-allowed-users "alice,bob"
```

---

## Сборка

```bash
cargo build --release
```

Версия включает git SHA: `rpglotd --version` → `0.4.2-abc1234`

Архитектура проекта: [ARCHITECTURE.md](ARCHITECTURE.md)

## Лицензия

MIT
