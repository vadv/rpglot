//! Help popup widget with context-sensitive column descriptions.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::tui::state::{PgStatementsViewMode, ProcessViewMode, Tab};

/// Renders the help popup centered on screen with scroll support.
pub fn render_help(
    frame: &mut Frame,
    area: Rect,
    tab: Tab,
    view_mode: ProcessViewMode,
    pgs_view_mode: PgStatementsViewMode,
    scroll: &mut usize,
) {
    // Calculate popup size (60% width, 80% height, clamped to 40-80 x 10-30)
    let popup_width = (area.width * 60 / 100).clamp(40, 80);
    let popup_height = (area.height * 80 / 100).clamp(10, 30);

    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Clear the area behind popup
    frame.render_widget(Clear, popup_area);

    // Get help content based on tab
    let (title, content) = get_help_content(tab, view_mode, pgs_view_mode);
    let content_lines = content.len();

    let block = Block::default()
        .title(format!(" {} ", title))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(popup_area);

    // Render block
    frame.render_widget(block, popup_area);

    // Split inner area: content + footer
    let chunks = Layout::vertical([
        Constraint::Min(1),    // Content
        Constraint::Length(1), // Footer
    ])
    .split(inner);

    // Calculate visible content height (excluding border and footer)
    let visible_height = chunks[0].height as usize;

    // Clamp scroll to valid range
    let max_scroll = content_lines.saturating_sub(visible_height);
    if *scroll > max_scroll {
        *scroll = max_scroll;
    }

    // Render content with scroll
    let paragraph = Paragraph::new(content)
        .wrap(Wrap { trim: false })
        .scroll((*scroll as u16, 0))
        .style(Style::default().fg(Color::White));
    frame.render_widget(paragraph, chunks[0]);

    // Render footer with scroll indicator
    let scroll_info = if max_scroll > 0 {
        format!(" [{}/{}]", *scroll + 1, max_scroll + 1)
    } else {
        String::new()
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("Press ", Style::default().fg(Color::DarkGray)),
        Span::styled("?", Style::default().fg(Color::Yellow)),
        Span::styled(" or ", Style::default().fg(Color::DarkGray)),
        Span::styled("H", Style::default().fg(Color::Yellow)),
        Span::styled(" to close", Style::default().fg(Color::DarkGray)),
        Span::styled(", ", Style::default().fg(Color::DarkGray)),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::styled(" to scroll", Style::default().fg(Color::DarkGray)),
        Span::styled(scroll_info, Style::default().fg(Color::DarkGray)),
    ]));
    frame.render_widget(footer, chunks[1]);
}

/// Returns help title and content lines for the given tab.
fn get_help_content(
    tab: Tab,
    view_mode: ProcessViewMode,
    pgs_view_mode: PgStatementsViewMode,
) -> (&'static str, Vec<Line<'static>>) {
    match tab {
        Tab::Processes => get_process_help(view_mode),
        Tab::PostgresActive => ("PostgreSQL Activity Help (PGA)", get_postgres_help()),
        Tab::PgStatements => get_pgs_help(pgs_view_mode),
    }
}

fn get_pgs_help(mode: PgStatementsViewMode) -> (&'static str, Vec<Line<'static>>) {
    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "View modes: t=Time, c=Calls, i=I/O, e=Temp",
        Style::default().fg(Color::Cyan),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Filtering: matches queryid (prefix), DB, USER or QUERY (substring)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Rates: most columns are per-second (/s) computed from deltas between two real samples",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Title indicators:",
        Style::default().fg(Color::Yellow),
    )));
    lines.push(Line::from(
        "  dt=Xs  - sample interval; rates are based on this period",
    ));
    lines.push(Line::from(
        "  age=Ys - time since last real data collection",
    ));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Live mode: fresh data every tick (no caching)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "History mode: data reflects daemon's collection (~30s intervals)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "-- means not enough data yet (first sample or after stats reset)",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(Span::styled(
        "Sorting: s=next column, r=reverse direction",
        Style::default().fg(Color::DarkGray),
    )));
    lines.push(Line::from(""));

    match mode {
        PgStatementsViewMode::Time => {
            lines.push(Line::from(Span::styled(
                "Columns (Time):",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("CALLS/s - executions per second"),
                Line::from("TIME/s  - execution time per second (ms/s)"),
                Line::from("          divide by 1000 to get CPU count used by this query type"),
                Line::from("          e.g. TIME/s=2500 means ~2.5 CPUs for CPU-bound queries"),
                Line::from("MEAN    - mean execution time per call (ms)"),
                Line::from("ROWS/s  - rows per second"),
                Line::from("DB/USER - database and role name"),
                Line::from("QUERY   - normalized query text"),
            ]);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Troubleshooting Tips:",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("Sort by TIME/s to find queries consuming most CPU time"),
                Line::from("High MEAN + low CALLS/s = single slow query (optimize it)"),
                Line::from("Low MEAN + high CALLS/s = hot path (caching/batching helps)"),
                Line::from("MEAN suddenly increased = plan regression, check EXPLAIN"),
                Line::from("SUM of all TIME/s / 1000 ~= total CPU used by PostgreSQL"),
            ]);
            ("PostgreSQL Statements Help (PGS) - Time (t)", lines)
        }
        PgStatementsViewMode::Calls => {
            lines.push(Line::from(Span::styled(
                "Columns (Calls):",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("CALLS/s - executions per second"),
                Line::from("ROWS/s  - rows per second"),
                Line::from("R/CALL  - rows per call (derived from rates when available)"),
                Line::from("MEAN    - mean execution time per call (ms)"),
                Line::from("DB/USER - database and role name"),
                Line::from("QUERY   - normalized query text"),
            ]);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Troubleshooting Tips:",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("High CALLS/s = hot path, consider caching or batching"),
                Line::from("R/CALL >> expected = missing WHERE clause or bad join"),
                Line::from("R/CALL = 0 with high CALLS/s = possible UPDATE/DELETE overhead"),
            ]);
            ("PostgreSQL Statements Help (PGS) - Calls (c)", lines)
        }
        PgStatementsViewMode::Io => {
            lines.push(Line::from(Span::styled(
                "Columns (I/O):",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("BLK_RD/s  - shared blocks read per second"),
                Line::from("BLK_HIT/s - shared blocks hit per second"),
                Line::from("HIT%    - shared buffer cache hit ratio"),
                Line::from("BLK_DIRT/s- shared blocks dirtied per second"),
                Line::from("BLK_WR/s  - shared blocks written per second"),
                Line::from("DB      - database name"),
                Line::from("QUERY   - normalized query text"),
            ]);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Troubleshooting Tips:",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("HIT% < 99% for OLTP = consider increasing shared_buffers"),
                Line::from("HIT% < 90% = query reads much data from disk, check indexes"),
                Line::from("High BLK_RD/s = missing index or seq scan on large table"),
                Line::from("High BLK_DIRT/s = heavy writes, check checkpoint frequency"),
                Line::from("1 block = 8 KiB, so BLK_RD/s * 8 / 1024 = read MB/s"),
            ]);
            ("PostgreSQL Statements Help (PGS) - I/O (i)", lines)
        }
        PgStatementsViewMode::Temp => {
            lines.push(Line::from(Span::styled(
                "Columns (Temp):",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("TMP_RD/s, TMP_WR/s - temp blocks read/written per second"),
                Line::from(
                    "TMP_MB/s          - temp blocks converted to MB/s (assumes 8 KiB blocks)",
                ),
                Line::from("LOC_RD/s, LOC_WR/s - local blocks read/written per second"),
                Line::from("DB           - database name"),
                Line::from("QUERY        - normalized query text"),
            ]);
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Troubleshooting Tips:",
                Style::default().fg(Color::Yellow),
            )));
            lines.extend([
                Line::from("TMP_WR/s > 0 = query spills to disk, increase work_mem"),
                Line::from("  caused by: sorts, hash joins, hash aggregations"),
                Line::from("  try: SET work_mem = '256MB' for specific queries"),
                Line::from("High LOC blocks = temp tables, consider optimizing queries"),
                Line::from("Persistent temp usage = set work_mem in postgresql.conf"),
                Line::from("  but beware: work_mem is per-operation, not per-query"),
            ]);
            ("PostgreSQL Statements Help (PGS) - Temp (e)", lines)
        }
    }
}

/// Returns help content for process tab based on view mode.
fn get_process_help(mode: ProcessViewMode) -> (&'static str, Vec<Line<'static>>) {
    match mode {
        ProcessViewMode::Generic => (
            "Process Help - Generic (g)",
            vec![
                Line::from(Span::styled(
                    "View modes: g=Generic, c=Command, m=Memory",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Column Descriptions:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("PID     - Process ID (unique identifier)"),
                Line::from("SYSCPU  - System (kernel) CPU time in ticks"),
                Line::from("USRCPU  - User-space CPU time in ticks"),
                Line::from("RDELAY  - Run queue delay (scheduling latency, ms)"),
                Line::from("VGROW   - Virtual memory growth since last sample"),
                Line::from("RGROW   - Resident memory growth since last sample"),
                Line::from("RUID    - Real user name (who started the process)"),
                Line::from("EUID    - Effective user name (current privileges)"),
                Line::from("ST      - Process start time (HH:MM:SS or date if older)"),
                Line::from("EXC     - Exit signal (signal that terminated process,"),
                Line::from("          typically 17=SIGCHLD for running processes)"),
                Line::from("THR     - Number of threads in this process"),
                Line::from("S       - State: R=Running, S=Sleeping, D=Disk wait,"),
                Line::from("          Z=Zombie, T=Stopped"),
                Line::from("CPUNR   - Last CPU core where process ran"),
                Line::from("CPU     - CPU usage percentage over sample interval"),
                Line::from("CMD     - Process name (executable name)"),
                Line::from(""),
                Line::from(Span::styled(
                    "Troubleshooting Tips:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("High RDELAY - CPU saturation, processes wait in run queue"),
                Line::from("  check CPL avg1 vs num_cpus in summary panel"),
                Line::from("State D     - process blocked on disk I/O (uninterruptible)"),
                Line::from("  many D-state processes indicate I/O bottleneck"),
                Line::from("VGROW rising - possible memory leak if grows without bound"),
                Line::from("  compare VGROW and RGROW over time for suspect processes"),
                Line::from("THR growing - thread leak if count increases without bound"),
                Line::from(""),
                Line::from(Span::styled(
                    "PostgreSQL Integration:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("When process PID matches pg_stat_activity:"),
                Line::from("  CMD shows: name [query] or name [backend_type]"),
                Line::from("  (highlighted in cyan, backend_type if query is empty)"),
                Line::from("Use > or J to drill-down from PRC to PGA for PG processes"),
            ],
        ),
        ProcessViewMode::Command => (
            "Process Help - Command (c)",
            vec![
                Line::from(Span::styled(
                    "View modes: g=Generic, c=Command, m=Memory",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Column Descriptions:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("PID     - Process ID (unique identifier)"),
                Line::from("TID     - Thread ID (= PID for main thread)"),
                Line::from("S       - State: R=Running, S=Sleeping, D=Disk wait,"),
                Line::from("          Z=Zombie, T=Stopped"),
                Line::from("CPU     - CPU usage percentage over sample interval"),
                Line::from("MEM     - Resident memory size (physical memory used)"),
                Line::from("CMDLINE - Full command line with arguments"),
                Line::from(""),
                Line::from(Span::styled("Tip:", Style::default().fg(Color::Yellow))),
                Line::from(""),
                Line::from("Command view is useful for identifying processes by full path"),
                Line::from("  e.g. distinguishing multiple java/python/node instances"),
                Line::from(""),
                Line::from(Span::styled(
                    "PostgreSQL Integration:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("When process PID matches pg_stat_activity:"),
                Line::from("  CMDLINE shows: cmdline [query] or cmdline [backend_type]"),
                Line::from("  (highlighted in cyan, backend_type if query is empty)"),
            ],
        ),
        ProcessViewMode::Memory => (
            "Process Help - Memory (m)",
            vec![
                Line::from(Span::styled(
                    "View modes: g=Generic, c=Command, m=Memory",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Column Descriptions:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("PID     - Process ID (unique identifier)"),
                Line::from("TID     - Thread ID (= PID for main thread)"),
                Line::from("MINFLT  - Minor page faults (in-memory, no disk I/O)"),
                Line::from("MAJFLT  - Major page faults (require disk read)"),
                Line::from("VSTEXT  - Virtual size of executable code segment"),
                Line::from("VSLIBS  - Virtual size of shared libraries"),
                Line::from("VDATA   - Virtual size of data segment (heap)"),
                Line::from("VSTACK  - Virtual size of stack"),
                Line::from("LOCKSZ  - Memory locked in RAM (mlock)"),
                Line::from("VSIZE   - Total virtual memory (address space)"),
                Line::from("RSIZE   - Resident set size (physical RAM used)"),
                Line::from("PSIZE   - Proportional set size (shared pages divided)"),
                Line::from("VGROW   - Virtual memory growth since last sample"),
                Line::from("RGROW   - Resident memory growth since last sample"),
                Line::from("SWAPSZ  - Memory swapped to disk"),
                Line::from("RUID    - Real user name (who started process)"),
                Line::from("EUID    - Effective user name (current privileges)"),
                Line::from("MEM     - Memory usage as % of total system RAM"),
                Line::from("CMD     - Process name (executable name)"),
                Line::from(""),
                Line::from(Span::styled(
                    "Troubleshooting Tips:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("MAJFLT > 0 - process pages are being read from swap/disk"),
                Line::from("  high MAJFLT = severe performance degradation"),
                Line::from("SWAPSZ > 0 - process memory was swapped out (memory pressure)"),
                Line::from("VSIZE >> RSIZE - large address space but low actual usage"),
                Line::from("  normal for Java/Go (pre-allocated heap)"),
                Line::from("RGROW rising without VGROW - process is touching more pages"),
                Line::from("LOCKSZ > 0 - memory pinned in RAM (e.g. shared_buffers huge pages)"),
                Line::from(""),
                Line::from(Span::styled(
                    "PostgreSQL Integration:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("When process PID matches pg_stat_activity:"),
                Line::from("  CMD shows: name [query] or name [backend_type]"),
                Line::from("  (highlighted in cyan, backend_type if query is empty)"),
            ],
        ),
        ProcessViewMode::Disk => (
            "Process Help - Disk I/O (d)",
            vec![
                Line::from(Span::styled(
                    "View modes: g=Generic, c=Command, m=Memory, d=Disk",
                    Style::default().fg(Color::Cyan),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Column Descriptions:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("PID     - Process ID (unique identifier)"),
                Line::from("RDDSK   - Read throughput (bytes/sec from /proc/[pid]/io)"),
                Line::from("          Shows rate since last sample in auto units (B/K/M/G)"),
                Line::from("WRDSK   - Write throughput (bytes/sec from /proc/[pid]/io)"),
                Line::from("          Shows rate since last sample in auto units (B/K/M/G)"),
                Line::from("WCANCL  - Cancelled write bytes/sec (writes truncated/cancelled)"),
                Line::from("          Non-zero indicates I/O was started but not completed"),
                Line::from("DSK     - Disk I/O percentage of total system disk activity"),
                Line::from("          (RDDSK + WRDSK) / total_system_io * 100"),
                Line::from("CMD     - Process name (executable name)"),
                Line::from(""),
                Line::from(Span::styled(
                    "Data Source:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("Read from /proc/[pid]/io (requires permissions)"),
                Line::from("  read_bytes  - Total bytes read from storage"),
                Line::from("  write_bytes - Total bytes written to storage"),
                Line::from("  cancelled_write_bytes - Truncated page cache writes"),
                Line::from(""),
                Line::from(Span::styled(
                    "PostgreSQL Integration:",
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(""),
                Line::from("When process PID matches pg_stat_activity:"),
                Line::from("  CMD shows: name [query] or name [backend_type]"),
                Line::from("  (highlighted in cyan, backend_type if query is empty)"),
            ],
        ),
    }
}

/// Returns help content for PostgreSQL activity tab.
fn get_postgres_help() -> Vec<Line<'static>> {
    vec![
        Line::from(Span::styled(
            "View Modes (switch with g/v):",
            Style::default().fg(Color::Cyan),
        )),
        Line::from(""),
        Line::from(
            "g = Generic view: PID, CPU%, RSS, DB, USER, STATE, WAIT, QDUR, XDUR, BDUR, BTYPE, QUERY",
        ),
        Line::from("v = Stats view:   PID, DB, USER, STATE, QDUR, MEAN, MAX, CALL/s, HIT%, QUERY"),
        Line::from("    (Shows pg_stat_statements metrics linked by query_id)"),
        Line::from(""),
        Line::from(Span::styled(
            "Generic View Columns (g):",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("PID    - PostgreSQL backend process ID"),
        Line::from("CPU%   - CPU usage from OS (linked by PID)"),
        Line::from("RSS    - Resident Set Size from OS (linked by PID)"),
        Line::from("DB     - Database name"),
        Line::from("USER   - PostgreSQL user name"),
        Line::from("STATE  - Connection state (active/idle/idle in transaction)"),
        Line::from("WAIT   - Wait event type:event (e.g., Lock:tuple)"),
        Line::from("QDUR   - Query duration (time since query_start)"),
        Line::from("XDUR   - Transaction duration (time since xact_start)"),
        Line::from("BDUR   - Backend duration (time since backend_start)"),
        Line::from("BTYPE  - Backend type (client backend, autovacuum, etc.)"),
        Line::from("QUERY  - Current/last query text"),
        Line::from(""),
        Line::from(Span::styled(
            "Stats View Columns (v):",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("PID    - PostgreSQL backend process ID"),
        Line::from("DB     - Database name"),
        Line::from("USER   - PostgreSQL user name"),
        Line::from("STATE  - Connection state"),
        Line::from("QDUR   - Query duration (with anomaly highlighting)"),
        Line::from("MEAN   - Mean execution time from pg_stat_statements"),
        Line::from("MAX    - Max execution time from pg_stat_statements"),
        Line::from("CALL/s - Calls per second from pg_stat_statements"),
        Line::from("HIT%   - Buffer cache hit percentage from pg_stat_statements"),
        Line::from("QUERY  - Current/last query text"),
        Line::from(""),
        Line::from(Span::styled(
            "Stats View Anomaly Highlighting:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("Yellow - QDUR > 2× MEAN (slower than usual)"),
        Line::from("Red    - QDUR > 5× MEAN (much slower than usual)"),
        Line::from("Red+Bold - QDUR > MAX (new record!)"),
        Line::from("Yellow - HIT% < 80% (many disk reads)"),
        Line::from("Red    - HIT% < 50% (excessive disk reads)"),
        Line::from("'--'   - No stats (query_id=0 or not in pg_stat_statements)"),
        Line::from(""),
        Line::from(Span::styled(
            "Navigation:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("> or J - Drill-down to PGS (pg_stat_statements) for selected query"),
        Line::from("         (requires query_id, available in PostgreSQL 14+)"),
        Line::from(""),
        Line::from(Span::styled("Sorting:", Style::default().fg(Color::Yellow))),
        Line::from("Default: non-idle sessions first, sorted by QDUR desc"),
        Line::from("Use s/S to cycle sort column, r/R to reverse"),
        Line::from(""),
        Line::from(Span::styled(
            "Filtering:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("Press / or p to filter by PID, query_id, DB, USER, or QUERY"),
        Line::from("  - PID and query_id: prefix match (e.g., '123' matches '12345')"),
        Line::from("  - Text fields: substring match (case-insensitive)"),
        Line::from("Press i to toggle hide/show idle sessions"),
        Line::from(""),
        Line::from(Span::styled(
            "Color coding (Generic view):",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("Green  - active state"),
        Line::from("Yellow - idle in transaction, QDUR > 1min, WAIT event"),
        Line::from("Red    - QDUR > 5min (for active queries)"),
        Line::from("Gray   - idle sessions (shown at bottom)"),
        Line::from(""),
        Line::from(Span::styled(
            "Troubleshooting Tips:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from(""),
        Line::from("idle in transaction - holds locks, blocks autovacuum/vacuum"),
        Line::from("  long idle-in-transaction = danger of table bloat"),
        Line::from("  check XDUR to see how long the transaction has been open"),
        Line::from("WAIT Lock:* - session is waiting for a lock held by another"),
        Line::from("  sort by QDUR to find the longest-waiting sessions"),
        Line::from("QDUR > 5min (red) - likely needs EXPLAIN ANALYZE investigation"),
        Line::from("Many active sessions - possible connection pool exhaustion"),
        Line::from("  compare active count vs max_connections"),
        Line::from("BDUR very long - consider connection pooling (pgbouncer)"),
        Line::from("  long-lived connections use resources even when idle"),
        Line::from(""),
        Line::from(Span::styled(
            "Session Detail Popup:",
            Style::default().fg(Color::Yellow),
        )),
        Line::from("Press Enter to open detailed view with:"),
        Line::from("- Session Identity (PID, DB, User, App, Client, Backend Type)"),
        Line::from("- Timing (start times, durations)"),
        Line::from("- State & Wait events"),
        Line::from("- OS Process metrics (CPU, Memory, I/O)"),
        Line::from("- Statement Statistics (from pg_stat_statements, if available)"),
        Line::from("- Full query text"),
    ]
}
