//! Process table widget with sorting, filtering, and diff highlighting.
//! Supports multiple view modes similar to atop: Generic (g), Command (c), Memory (m).

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Row, Table};

use crate::tui::fmt::normalize_for_display;
use crate::tui::state::{
    AppState, CachedWidths, ColumnType, DiffStatus, ProcessRow, ProcessViewMode, TableRow,
};
use crate::tui::style::Styles;

/// Renders the process table with the current view mode.
pub fn render_processes(frame: &mut Frame, area: Rect, state: &mut AppState) {
    // Resolve selection by tracked entity ID
    state.process_table.resolve_selection();

    // Sync ratatui TableState for auto-scrolling
    state
        .prc_ratatui_state
        .select(Some(state.process_table.selected));

    let table_state = &state.process_table;
    let view_mode = state.process_view_mode;

    // Get headers for current view mode
    let all_headers = ProcessRow::headers_for_mode(view_mode);

    // Get widths from cache or fallback to defaults
    let all_widths = state
        .cached_widths
        .as_ref()
        .map(|c| match view_mode {
            ProcessViewMode::Generic => c.generic.clone(),
            ProcessViewMode::Command => c.command.clone(),
            ProcessViewMode::Memory => c.memory.clone(),
            ProcessViewMode::Disk => c.disk.clone(),
        })
        .unwrap_or_else(|| ProcessRow::widths_for_mode(view_mode));

    // Apply horizontal scroll
    let scroll = state
        .horizontal_scroll
        .min(all_headers.len().saturating_sub(1));
    let visible_headers: Vec<&str> = all_headers.iter().skip(scroll).copied().collect();
    let visible_widths: Vec<u16> = all_widths.iter().skip(scroll).copied().collect();

    // Headers with sort indicator
    let headers: Vec<Span> = visible_headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let actual_col = i + scroll;
            let indicator = if actual_col == table_state.sort_column {
                if table_state.sort_ascending {
                    "▲"
                } else {
                    "▼"
                }
            } else {
                ""
            };
            Span::styled(format!("{}{}", h, indicator), Styles::table_header())
        })
        .collect();

    let header = Row::new(headers).style(Styles::table_header()).height(1);

    // Rows with diff highlighting
    let filtered = table_state.filtered_items();
    let rows: Vec<Row> = filtered
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let id = item.id();
            let diff_status = table_state
                .diff_status
                .get(&id)
                .cloned()
                .unwrap_or_default();

            let all_cells = item.cells_for_mode(view_mode);
            let visible_cells: Vec<String> = all_cells.into_iter().skip(scroll).collect();
            let has_pg_query = item.query.is_some();
            let cmd_col_idx = all_headers.len().saturating_sub(1); // CMD is last column

            let styled_cells: Vec<Span> = visible_cells
                .iter()
                .enumerate()
                .map(|(col_idx, cell)| {
                    let actual_col = col_idx + scroll;
                    let mut style =
                        cell_style(&diff_status, actual_col, idx == table_state.selected);

                    // Highlight CMD column in cyan if process has PostgreSQL query
                    if has_pg_query && actual_col == cmd_col_idx {
                        style = style.fg(ratatui::style::Color::Cyan);
                    }

                    Span::styled(cell.clone(), style)
                })
                .collect();

            let row_style = if idx == table_state.selected {
                Styles::selected()
            } else {
                match &diff_status {
                    DiffStatus::New => Styles::new_item(),
                    DiffStatus::Modified(_) => Styles::modified_item(),
                    DiffStatus::Unchanged => Styles::default(),
                }
            };

            Row::new(styled_cells).style(row_style).height(1)
        })
        .collect();

    // Build title with mode indicator and filter info
    let mode_name = match view_mode {
        ProcessViewMode::Generic => "GEN",
        ProcessViewMode::Command => "CMD",
        ProcessViewMode::Memory => "MEM",
        ProcessViewMode::Disk => "DSK",
    };

    let title = if let Some(filter) = &table_state.filter {
        format!(
            " Processes [{}] (filter: {}) [{}/{}] ",
            mode_name,
            filter,
            filtered.len(),
            table_state.items.len()
        )
    } else {
        format!(" Processes [{}] [{}] ", mode_name, table_state.items.len())
    };

    // Add scroll indicator if scrolled
    let title = if scroll > 0 {
        format!("{}← scroll: {} ", title, scroll)
    } else {
        title
    };

    let widths: Vec<ratatui::layout::Constraint> = visible_widths
        .iter()
        .map(|w| ratatui::layout::Constraint::Length(*w))
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Styles::selected());

    // Clear the area before rendering to avoid artifacts
    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.prc_ratatui_state);
}

/// Returns style for a cell based on diff status and selection.
fn cell_style(
    diff_status: &DiffStatus,
    col_idx: usize,
    is_selected: bool,
) -> ratatui::style::Style {
    let base = if is_selected {
        Styles::selected()
    } else {
        Styles::default()
    };

    match diff_status {
        DiffStatus::New => base.fg(ratatui::style::Color::Green),
        DiffStatus::Modified(cols) if cols.contains(&col_idx) => base
            .fg(ratatui::style::Color::Yellow)
            .add_modifier(Modifier::BOLD),
        _ => base,
    }
}

/// Extracts process rows from snapshot with VGROW/RGROW and CPU% calculation.
/// Also enriches processes with PostgreSQL query information when PID matches pg_stat_activity.
#[allow(clippy::too_many_arguments)]
pub fn extract_processes(
    snapshot: &crate::storage::model::Snapshot,
    interner: Option<&crate::storage::StringInterner>,
    user_resolver: Option<&crate::collector::UserResolver>,
    prev_mem: &std::collections::HashMap<u32, (u64, u64)>,
    prev_cpu: &std::collections::HashMap<u32, (u64, u64)>,
    prev_dsk: &std::collections::HashMap<u32, (u64, u64, u64)>,
    prev_total_cpu_time: Option<u64>,
    current_total_cpu_time: u64,
    total_mem_kb: u64,
    elapsed_secs: f64,
) -> Vec<ProcessRow> {
    use crate::storage::model::DataBlock;

    // Build a mapping from PostgreSQL backend PID to (query, backend_type)
    // We include all PIDs that have pg_stat_activity entry, even if query is empty
    let pg_info: std::collections::HashMap<u32, (Option<String>, Option<String>)> = snapshot
        .blocks
        .iter()
        .filter_map(|block| {
            if let DataBlock::PgStatActivity(activities) = block {
                Some(activities.iter().filter_map(|pg| {
                    // Convert i32 pid to u32, skip negative pids
                    let pid = u32::try_from(pg.pid).ok()?;
                    // Resolve query hash to string
                    let query = interner
                        .and_then(|i| i.resolve(pg.query_hash))
                        .map(|s| s.to_string())
                        .filter(|s| !s.is_empty());
                    // Resolve backend_type hash to string
                    let backend_type = interner
                        .and_then(|i| i.resolve(pg.backend_type_hash))
                        .map(|s| s.to_string())
                        .filter(|s| !s.is_empty());
                    // Include if at least one of query or backend_type is present
                    if query.is_some() || backend_type.is_some() {
                        Some((pid, (query, backend_type)))
                    } else {
                        None
                    }
                }))
            } else {
                None
            }
        })
        .flatten()
        .collect();

    for block in &snapshot.blocks {
        if let DataBlock::Processes(processes) = block {
            return processes
                .iter()
                .map(|p| {
                    let name = interner
                        .and_then(|i| i.resolve(p.name_hash))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("[{}]", p.pid));

                    let cmdline = interner
                        .and_then(|i| i.resolve(p.cmdline_hash))
                        .map(|s| s.to_string())
                        .unwrap_or_default();

                    // Calculate VGROW and RGROW from previous values
                    let (vgrow, rgrow) =
                        if let Some(&(prev_vsize, prev_rsize)) = prev_mem.get(&p.pid) {
                            (
                                p.mem.vmem as i64 - prev_vsize as i64,
                                p.mem.rmem as i64 - prev_rsize as i64,
                            )
                        } else {
                            (0, 0)
                        };

                    // Calculate memory percentage
                    let mem_percent = if total_mem_kb > 0 {
                        (p.mem.rmem as f64 / total_mem_kb as f64) * 100.0
                    } else {
                        0.0
                    };

                    // Resolve user names
                    let ruser = user_resolver
                        .map(|r| r.resolve(p.uid))
                        .unwrap_or_else(|| p.uid.to_string());
                    let euser = user_resolver
                        .map(|r| r.resolve(p.euid))
                        .unwrap_or_else(|| p.euid.to_string());

                    // Calculate CPU% from delta
                    let cpu_percent = if let (Some(prev_total), Some(&(prev_utime, prev_stime))) =
                        (prev_total_cpu_time, prev_cpu.get(&p.pid))
                    {
                        let delta_process =
                            (p.cpu.utime + p.cpu.stime).saturating_sub(prev_utime + prev_stime);
                        let delta_total = current_total_cpu_time.saturating_sub(prev_total);
                        if delta_total > 0 {
                            (delta_process as f64 / delta_total as f64) * 100.0
                        } else {
                            0.0
                        }
                    } else {
                        0.0 // No previous data, show 0%
                    };

                    // Calculate disk I/O rates
                    let (rddsk, wrdsk, wcancl) = if elapsed_secs > 0.0 {
                        if let Some(&(prev_rsz, prev_wsz, prev_cwsz)) = prev_dsk.get(&p.pid) {
                            let delta_rsz = p.dsk.rsz.saturating_sub(prev_rsz);
                            let delta_wsz = p.dsk.wsz.saturating_sub(prev_wsz);
                            let delta_cwsz = p.dsk.cwsz.saturating_sub(prev_cwsz);
                            (
                                (delta_rsz as f64 / elapsed_secs) as i64,
                                (delta_wsz as f64 / elapsed_secs) as i64,
                                (delta_cwsz as f64 / elapsed_secs) as i64,
                            )
                        } else {
                            (0, 0, 0)
                        }
                    } else {
                        (0, 0, 0)
                    };

                    // Look up PostgreSQL info for this PID
                    // Normalize query text to remove newlines/tabs that would cause
                    // ratatui rendering artifacts (text wrapping into adjacent cells).
                    let (query, backend_type) = pg_info
                        .get(&p.pid)
                        .map(|(q, bt)| (q.clone().map(|s| normalize_for_display(&s)), bt.clone()))
                        .unwrap_or((None, None));

                    ProcessRow {
                        pid: p.pid,
                        tid: p.pid, // TID = PID for main process (threads would have different TID)
                        name,
                        cmdline,

                        // CPU metrics
                        syscpu: p.cpu.stime,
                        usrcpu: p.cpu.utime,
                        cpu_percent,
                        rdelay: p.cpu.rundelay,
                        cpunr: p.cpu.curcpu,

                        // Memory metrics (already in KB)
                        minflt: p.mem.minflt,
                        majflt: p.mem.majflt,
                        vstext: p.mem.vexec,
                        vslibs: p.mem.vlibs,
                        vdata: p.mem.vdata,
                        vstack: p.mem.vstack,
                        vlock: p.mem.vlock,
                        vsize: p.mem.vmem,
                        rsize: p.mem.rmem,
                        psize: p.mem.pmem,
                        vswap: p.mem.vswap,
                        mem_percent,

                        // Deltas
                        vgrow,
                        rgrow,

                        // User identification
                        ruid: p.uid,
                        euid: p.euid,
                        ruser,
                        euser,

                        // State
                        state: p.state.to_string(),
                        exit_code: p.exit_signal,
                        num_threads: p.num_threads,
                        btime: p.btime,

                        // PostgreSQL integration
                        query,
                        backend_type,

                        // Disk I/O metrics
                        rddsk,
                        wrdsk,
                        wcancl,
                        dsk_percent: 0.0, // Calculated after all processes are collected
                    }
                })
                .collect();
        }
    }

    Vec::new()
}

/// Updates the prev_process_mem map with current memory values.
pub fn update_prev_mem(
    snapshot: &crate::storage::model::Snapshot,
    prev_mem: &mut std::collections::HashMap<u32, (u64, u64)>,
) {
    use crate::storage::model::DataBlock;

    prev_mem.clear();
    for block in &snapshot.blocks {
        if let DataBlock::Processes(processes) = block {
            for p in processes {
                prev_mem.insert(p.pid, (p.mem.vmem, p.mem.rmem));
            }
        }
    }
}

/// Gets total system memory from snapshot (for MEM% calculation).
pub fn get_total_memory(snapshot: &crate::storage::model::Snapshot) -> u64 {
    use crate::storage::model::DataBlock;

    for block in &snapshot.blocks {
        if let DataBlock::SystemMem(mem) = block {
            return mem.total;
        }
    }
    // Default to 16GB if not found
    16 * 1024 * 1024
}

/// Updates the prev_process_cpu map with current CPU values.
pub fn update_prev_cpu(
    snapshot: &crate::storage::model::Snapshot,
    prev_cpu: &mut std::collections::HashMap<u32, (u64, u64)>,
) {
    use crate::storage::model::DataBlock;

    prev_cpu.clear();
    for block in &snapshot.blocks {
        if let DataBlock::Processes(processes) = block {
            for p in processes {
                prev_cpu.insert(p.pid, (p.cpu.utime, p.cpu.stime));
            }
        }
    }
}

/// Updates the prev_process_dsk map with current disk I/O values.
pub fn update_prev_dsk(
    snapshot: &crate::storage::model::Snapshot,
    prev_dsk: &mut std::collections::HashMap<u32, (u64, u64, u64)>,
) {
    use crate::storage::model::DataBlock;

    prev_dsk.clear();
    for block in &snapshot.blocks {
        if let DataBlock::Processes(processes) = block {
            for p in processes {
                prev_dsk.insert(p.pid, (p.dsk.rsz, p.dsk.wsz, p.dsk.cwsz));
            }
        }
    }
}

/// Gets total CPU time from snapshot (sum of all CPU time for all cores).
pub fn get_total_cpu_time(snapshot: &crate::storage::model::Snapshot) -> u64 {
    use crate::storage::model::DataBlock;

    for block in &snapshot.blocks {
        if let DataBlock::SystemCpu(cpus) = block {
            // Find total CPU (cpu_id == -1)
            if let Some(total) = cpus.iter().find(|c| c.cpu_id == -1) {
                return total.user
                    + total.nice
                    + total.system
                    + total.idle
                    + total.iowait
                    + total.irq
                    + total.softirq;
            }
        }
    }
    0
}

/// Calculates adaptive column widths based on actual data.
/// Called once on first snapshot and on terminal resize.
pub fn calculate_adaptive_widths(
    items: &[ProcessRow],
    mode: ProcessViewMode,
    available_width: u16,
) -> Vec<u16> {
    let headers = ProcessRow::headers_for_mode(mode);
    let min_widths = ProcessRow::min_widths_for_mode(mode);
    let col_types = ProcessRow::column_types_for_mode(mode);

    // 1. Calculate content width for each column (start with header lengths + 1 for sort indicator)
    let mut content_widths: Vec<u16> = headers.iter().map(|h| h.len() as u16 + 1).collect();

    // Find max cell width for each column
    for item in items {
        let cells = item.cells_for_mode(mode);
        for (i, cell) in cells.iter().enumerate() {
            if i < content_widths.len() {
                content_widths[i] = content_widths[i].max(cell.len() as u16);
            }
        }
    }

    // Cap Expandable columns to prevent them from distorting proportional shrink.
    // CMD/COMMAND-LINE can be 300+ chars due to appended SQL queries, but during
    // width calculation we only need a reasonable cap — the column will get whatever
    // space remains after Fixed/Flexible columns are satisfied.
    const EXPANDABLE_CAP: u16 = 40;
    for (i, col_type) in col_types.iter().enumerate() {
        if matches!(col_type, ColumnType::Expandable) {
            content_widths[i] = content_widths[i].min(EXPANDABLE_CAP);
        }
    }

    // 2. Apply minimum widths
    let mut widths: Vec<u16> = content_widths
        .iter()
        .zip(min_widths.iter())
        .map(|(content, min)| (*content).max(*min))
        .collect();

    // 3. Calculate total and adjust
    let total: u16 = widths.iter().sum();
    let spacing = (widths.len() as u16).saturating_sub(1); // gaps between columns
    let target = available_width.saturating_sub(spacing).saturating_sub(2); // -2 for borders

    if total <= target {
        // 4a. Space available: expand Expandable columns
        let extra = target - total;
        for (i, col_type) in col_types.iter().enumerate() {
            if matches!(col_type, ColumnType::Expandable) {
                widths[i] += extra;
                break; // Give all extra to first expandable
            }
        }
    } else {
        // 4b. Need to shrink: reduce Flexible/Expandable proportionally
        // Keep Fixed columns, shrink others
        let fixed_total: u16 = widths
            .iter()
            .zip(col_types.iter())
            .filter(|(_, t)| matches!(t, ColumnType::Fixed))
            .map(|(w, _)| *w)
            .sum();

        let shrinkable_target = target.saturating_sub(fixed_total);
        let shrinkable_current: u16 = widths
            .iter()
            .zip(col_types.iter())
            .filter(|(_, t)| !matches!(t, ColumnType::Fixed))
            .map(|(w, _)| *w)
            .sum();

        if shrinkable_current > 0 {
            let ratio = shrinkable_target as f64 / shrinkable_current as f64;
            for (i, col_type) in col_types.iter().enumerate() {
                if !matches!(col_type, ColumnType::Fixed) {
                    widths[i] = ((widths[i] as f64 * ratio) as u16).max(min_widths[i]);
                }
            }
        }

        // Overflow guard: if min_widths pushed total above target,
        // shrink Expandable column(s) to absorb the excess.
        let final_total: u16 = widths.iter().sum();
        if final_total > target {
            let excess = final_total - target;
            for (i, col_type) in col_types.iter().enumerate().rev() {
                if matches!(col_type, ColumnType::Expandable) {
                    widths[i] = widths[i].saturating_sub(excess).max(min_widths[i]);
                    break;
                }
            }
        }
    }

    widths
}

/// Calculates cached widths for all view modes.
pub fn calculate_cached_widths(items: &[ProcessRow], available_width: u16) -> CachedWidths {
    CachedWidths {
        generic: calculate_adaptive_widths(items, ProcessViewMode::Generic, available_width),
        command: calculate_adaptive_widths(items, ProcessViewMode::Command, available_width),
        memory: calculate_adaptive_widths(items, ProcessViewMode::Memory, available_width),
        disk: calculate_adaptive_widths(items, ProcessViewMode::Disk, available_width),
    }
}
