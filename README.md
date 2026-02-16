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

## Веб-интерфейс

### Общий вид

Интерфейс состоит из:
- **Header** — дата/время, health score, настройки
- **Summary panel** — карточки с CPU, память, диск, сеть, PostgreSQL метрики
- **Табы** — PRC, PGA, PGS, PGT, PGI, PGE, PGL
- **Timeline с heatmap** — навигация по времени с визуализацией активности
- **Таблица данных** — основная рабочая область
- **Detail panel** — боковая панель с деталями выбранной строки

### Health Score

В header отображается индикатор здоровья 0-100. Учитывает количество активных сессий, CPU, disk I/O. Цвет: зелёный (80+), жёлтый (50-79), красный (<50). Позволяет мгновенно оценить состояние системы при навигации по timeline.

### Timeline и heatmap

Цветная полоса под timeline показывает активность за выбранный период. Позволяет визуально найти моменты пиковой нагрузки, не просматривая каждый снапшот. Тёмные участки = высокая активность.

Heatmap включает:
- CPU usage
- Активные PostgreSQL сессии
- Ошибки из лога PostgreSQL (красные маркеры)
- Checkpoint events
- Autovacuum events

Навигация:
- Клик по heatmap — перейти к нужному моменту
- `←/→` — шаг по снимкам
- `Shift+←/→` — шаг ±1 час
- Календарь для выбора даты и часа

### Цветовая подсветка

Все значения в таблицах подсвечиваются по пороговым правилам:

| Цвет | Что означает | Примеры |
|------|-------------|---------|
| Красный | Требует внимания | CPU > 90%, HIT% < 90%, DEAD% > 20%, query > 30s |
| Жёлтый | Стоит мониторить | CPU 50-90%, HIT% 90-99%, idle in transaction |
| Зелёный | Норма | HIT% >= 99%, swap = 0 |
| Серый | Нет активности | Rate = 0, idle |

### Summary panel

Карточки с ключевыми метриками системы. Каждая метрика подсвечена по порогам. В контейнерах вместо хостовых метрик CPU/Memory показываются cgroup limits и usage.

Секции: CPU, Load, Memory, Swap, Disk (per-device), Network (per-interface), PSI, PostgreSQL (hit ratio, deadlocks, errors), BGWriter (checkpoints, buffers).

### Контекстная справка

Наведение на заголовок колонки показывает tooltip с описанием метрики, пороговыми значениями, практическим советом и ссылкой на PostgreSQL документацию. Клавиша `?` открывает полную справку по текущей вкладке.

### Темы и timezone

- Три темы: Light, Dark, System (следует за OS)
- Три timezone: Local, UTC, Moscow
- Настройки сохраняются в браузере

---

## Вкладки

### PRC — Процессы

Все процессы системы. PostgreSQL бэкенды подсвечиваются и показывают текущий SQL запрос. View modes: Generic, Command, Memory, Disk.

### PGA — PostgreSQL Activity

Активные сессии из `pg_stat_activity` с системными метриками (CPU%, RSS) через привязку по PID.

**View modes:**
- **Generic** — PID, CPU%, RSS, DB, User, State, Wait Event, Query Duration, Query
- **Stats** — обогащение из pg_stat_statements: среднее/максимальное время запроса, calls/s, cache HIT%. Подсвечивает аномалии: если текущий запрос выполняется в 5x дольше среднего — красный

**Фильтры:** скрытие idle сессий, скрытие системных бэкендов

### PGS — PostgreSQL Statements

TOP 500 запросов из pg_stat_statements. Метрики вычисляются как rate (в секунду) из дельт между снимками.

**View modes:**
- **Calls** — самые часто вызываемые запросы
- **Time** — самые медленные по суммарному времени
- **I/O** — запросы с максимальным buffer I/O, cache hit ratio
- **Temp** — запросы, использующие temp файлы (work_mem overflow)

### PGT — PostgreSQL Tables

Статистика таблиц со всех баз на инстансе. Колонка Database показывает источник.

**View modes:**
- **I/O** — physical reads/hits по heap и index blocks, cache hit ratio
- **Reads** — seq scan reads, index fetches, total tuples read
- **Writes** — inserts, updates, deletes, HOT updates
- **Scans** — соотношение sequential vs index scans (высокий SEQ% = нет индекса)
- **Maintenance** — dead tuples, DEAD%, last vacuum/analyze
- **Schema** — агрегация по schema name
- **Database** — агрегация по database name

### PGI — PostgreSQL Indexes

Статистика индексов со всех баз.

**View modes:**
- **I/O** — block reads/hits, cache hit ratio
- **Usage** — scans, tuple reads/fetches
- **Unused** — индексы с нулём сканирований, кандидаты на DROP
- **Schema** / **Database** — агрегация

### PGE — PostgreSQL Events

События из лога PostgreSQL.

**View modes:**
- **Errors** — ERROR, FATAL, PANIC из лога, сгруппированные по паттерну
- **Checkpoints** — timing, buffers written, WAL files, sync duration
- **Autovacuum** — детали каждого autovacuum/autoanalyze: timing, buffers, dead tuples removed

### PGL — PostgreSQL Locks

Дерево блокировок: кто кого блокирует. Отображается как плоская таблица с индентацией по глубине. Позволяет быстро найти корневой блокировщик.

---

## Drill-down

Клавиша `>` позволяет провалиться от общего к частному:

| Откуда | Куда | По какому полю |
|--------|------|---------------|
| PRC | PGA | PID процесса |
| PGA | PGS | query_id |
| PGT | PGI | relid (индексы этой таблицы) |
| PGL | PGA | PID сессии |

**Пример:** В PRC видите postgres процесс с CPU 95%. `>` → PGA: видите active запрос с QDUR 45s. `>` → PGS: видите что запрос делает 0% cache hits и 50k reads/s.

---

## Detail panel

Клик по строке открывает боковую панель с полной информацией:
- **PRC** — процесс: CPU, memory, disk I/O, /proc/pid/io
- **PGA** — сессия: timing, wait events, OS metrics, полный текст запроса
- **PGS** — запрос: rates, timing breakdown, I/O, temp/WAL usage, нормализованный SQL
- **PGT** — таблица: size, scans, reads/writes, vacuum/analyze stats
- **PGI** — индекс: scans, usage, I/O

Кнопка Copy для копирования SQL запросов. Drill-down кнопка для перехода к связанной сущности.

---

## Keyboard shortcuts

| Клавиша | Действие |
|---------|----------|
| `1`-`7` | Переключение табов |
| `j/k` или `↑/↓` | Навигация по строкам |
| `Enter` | Открыть detail panel |
| `>` | Drill-down |
| `Escape` | Закрыть detail / снять выделение |
| `/` | Фильтр |
| `?` | Справка |
| `Space` | Пауза (live mode) |
| `←/→` | Навигация по снимкам (history) |
| `Shift+←/→` | ±1 час (history) |

---

## Что собирается

**Система:** CPU (per-core), memory, swap, disk I/O (per-device), network (per-interface), load average, PSI, vmstat, cgroup metrics

**PostgreSQL:** pg_stat_activity, pg_stat_statements (TOP 500), pg_stat_user_tables (все базы), pg_stat_user_indexes (все базы), pg_stat_database, pg_stat_bgwriter, pg_locks, PostgreSQL log (errors, checkpoints, autovacuum)

**Минимальная версия PostgreSQL: 10**

---

## Хранение данных

Данные записываются в часовые chunk-файлы с zstd-сжатием. Каждый снапшот доступен по индексу без декомпрессии всего файла. Рядом с каждым chunk хранится heatmap sidecar для быстрой отрисовки timeline без чтения снапшотов.

Подробности формата: [docs/storage.md](docs/storage.md)

```bash
rpglotd --max-size 2G --max-days 14  # ротация по размеру и возрасту
```

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

Версия включает git SHA: `rpglotd --version` → `0.1.9-abc1234`

## Лицензия

MIT
