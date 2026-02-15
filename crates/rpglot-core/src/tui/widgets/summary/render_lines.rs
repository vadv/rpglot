//! Rendering functions for summary panel lines.

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use crate::fmt::{self, FmtStyle};
use crate::tui::state::Tab;
use crate::tui::style::Styles;

use super::metric_widths::*;
use super::{
    BgwSummary, CpuMetrics, DiskSummary, NetSummary, PgSummary, PsiSummary, SummaryMetrics,
    VmstatRates,
};

fn line_with_padding(spans: Vec<Span<'static>>, width: usize) -> Line<'static> {
    let content_len: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let mut result = spans;
    if content_len < width {
        result.push(Span::raw(" ".repeat(width - content_len)));
    }
    Line::from(result)
}

fn metric_spans(label: &str, value: &str, total_width: usize, style: Style) -> Vec<Span<'static>> {
    let label_with_colon = format!("{}:", label);
    let value_width = total_width.saturating_sub(label_with_colon.len());
    let formatted_value = format!("{:>width$}", value, width = value_width);
    vec![
        Span::raw(label_with_colon),
        Span::styled(formatted_value, style),
    ]
}

/// Creates spans for a labeled metric with default style
fn metric_spans_default(label: &str, value: &str, total_width: usize) -> Vec<Span<'static>> {
    metric_spans(label, value, total_width, Styles::default())
}

/// Renders CPL (load) line - fixed-width metrics.
pub(super) fn render_cpl_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let num_cpus = metrics.num_cpus.max(1) as f64;

    let mut spans = vec![Span::styled("CPL", Styles::dim()), Span::raw(" │ ")];

    spans.extend(metric_spans(
        "avg1",
        &format!("{:.2}", metrics.load1),
        AVG,
        style_for_load(metrics.load1, num_cpus),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "avg5",
        &format!("{:.2}", metrics.load5),
        AVG,
        style_for_load(metrics.load5, num_cpus),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "avg15",
        &format!("{:.2}", metrics.load15),
        AVG15,
        style_for_load(metrics.load15, num_cpus),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "procs",
        &format!("{}", metrics.nr_procs),
        PROCS,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "run",
        &format!("{}", metrics.nr_running),
        RUN,
        style_for_running(metrics.nr_running, num_cpus),
    ));

    line_with_padding(spans, width)
}

/// Renders CPU line (total or per-core) - fixed-width metrics.
pub(super) fn render_cpu_line(
    cpu: &CpuMetrics,
    is_total: bool,
    cpu_id: i16,
    width: usize,
) -> Line<'static> {
    let label = if is_total {
        Span::styled("CPU", Styles::cpu())
    } else {
        Span::styled("cpu", Styles::dim())
    };

    let mut spans = vec![label, Span::raw(" │ ")];

    spans.extend(metric_spans(
        "sys",
        &format!("{:.1}%", cpu.sys),
        CPU_PCT,
        style_for_cpu_sys(cpu.sys),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "usr",
        &format!("{:.1}%", cpu.usr),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "irq",
        &format!("{:.1}%", cpu.irq),
        CPU_PCT,
        style_for_cpu_irq(cpu.irq),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "iow",
        &format!("{:.1}%", cpu.iow),
        CPU_PCT,
        style_for_cpu_iow(cpu.iow),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "idle",
        &format!("{:.1}%", cpu.idle),
        IDLE,
        style_for_cpu_idle(cpu.idle),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "stl",
        &format!("{:.1}%", cpu.steal),
        STL,
        style_for_cpu_steal(cpu.steal),
    ));

    if !is_total {
        spans.push(Span::raw("  "));
        spans.extend(metric_spans(
            "cpu",
            &format!("{}", cpu_id),
            CPUNUM,
            Styles::dim(),
        ));
    }

    line_with_padding(spans, width)
}

/// Renders container CPU line using cgroup v2 metrics.
///
/// Displayed only when `cpu.max` sets a quota (`quota > 0`).
pub(super) fn render_cgroup_cpu_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let Some(curr) = metrics.cgroup_cpu.as_ref() else {
        return render_cpu_line(&metrics.cpu_total, true, -1, width);
    };

    // If we got here, quota/period should be valid, but keep it defensive.
    let limit_cores = if curr.quota > 0 && curr.period > 0 {
        curr.quota as f64 / curr.period as f64
    } else {
        0.0
    };

    let prev = metrics.cgroup_cpu_prev.as_ref().unwrap_or(curr);
    let delta_usage = curr.usage_usec.saturating_sub(prev.usage_usec);
    let delta_user = curr.user_usec.saturating_sub(prev.user_usec);
    let delta_system = curr.system_usec.saturating_sub(prev.system_usec);
    let delta_throttled_ms = curr.throttled_usec.saturating_sub(prev.throttled_usec) / 1000;
    let delta_nr_throttled = curr.nr_throttled.saturating_sub(prev.nr_throttled);

    let used_pct = if metrics.delta_time > 0.0 && limit_cores > 0.0 {
        let delta_usage_s = delta_usage as f64 / 1_000_000.0;
        ((delta_usage_s / metrics.delta_time) / limit_cores) * 100.0
    } else {
        0.0
    };

    let usr_pct = if delta_usage > 0 {
        (delta_user as f64 / delta_usage as f64) * 100.0
    } else {
        0.0
    };
    let sys_pct = if delta_usage > 0 {
        (delta_system as f64 / delta_usage as f64) * 100.0
    } else {
        0.0
    };

    let thrtl_style = if delta_throttled_ms > 1000 {
        Styles::critical()
    } else if delta_throttled_ms > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };

    let mut spans = vec![Span::styled("CPU", Styles::cpu()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "lim",
        &format!("{:.1}", limit_cores),
        CG_LIM,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "used",
        &format!("{:.0}%", used_pct.min(999.0)),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "usr",
        &format!("{:.0}%", usr_pct),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "sys",
        &format!("{:.0}%", sys_pct),
        CPU_PCT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "thrtl",
        &format!("{}ms", delta_throttled_ms),
        CG_THR,
        thrtl_style,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "nr",
        &format!("{}", delta_nr_throttled),
        CG_NR,
    ));

    line_with_padding(spans, width)
}

/// Renders PSI (Pressure Stall Information) line.
/// Format: PSI │ CPU:   5.1% MEM:   0.0% I/O:   0.1%
pub(super) fn render_psi_line(psi: &[PsiSummary], width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("PSI", Styles::dim()), Span::raw(" │ ")];

    for (i, p) in psi.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }

        // Color based on some_avg10 value
        let style = if p.some >= 10.0 {
            Styles::critical()
        } else if p.some >= 5.0 {
            Styles::modified_item()
        } else {
            Styles::default()
        };

        spans.extend(metric_spans(p.name, &format!("{:.1}%", p.some), 12, style));
    }

    line_with_padding(spans, width)
}

/// Renders vmstat rates line.
/// Format: VMS │ pgin:  3.4K/s pgout:  5.9K/s swin:        0 swout:        0 flt: 36.1K/s ctx: 11.9K/s
pub(super) fn render_vmstat_line(rates: &VmstatRates, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("VMS", Styles::dim()), Span::raw(" │ ")];

    // Swap in/out - critical if any swapping is happening
    let swin_style = if rates.pswpin_s > 0.0 {
        Styles::critical()
    } else {
        Styles::default()
    };
    let swout_style = if rates.pswpout_s > 0.0 {
        Styles::critical()
    } else {
        Styles::default()
    };

    spans.extend(metric_spans_default(
        "pgin",
        &fmt::format_rate(rates.pgpgin_s, FmtStyle::Compact),
        13,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "pgout",
        &fmt::format_rate(rates.pgpgout_s, FmtStyle::Compact),
        14,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "swin",
        &fmt::format_rate(rates.pswpin_s, FmtStyle::Compact),
        13,
        swin_style,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "swout",
        &fmt::format_rate(rates.pswpout_s, FmtStyle::Compact),
        14,
        swout_style,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "flt",
        &fmt::format_rate(rates.pgfault_s, FmtStyle::Compact),
        13,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "ctx",
        &fmt::format_rate(rates.ctxt_s, FmtStyle::Compact),
        13,
    ));

    line_with_padding(spans, width)
}

/// Renders PostgreSQL summary line from pg_stat_database.
/// Format: PG  │ tps:    750/s  hit:  99.3%  iohr:  98.5%  tup: 410.4K/s  tmp:          0  dlock:     0
pub(super) fn render_pg_line(pg: &PgSummary, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled(" PG", Styles::cpu()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "tps",
        &fmt::format_rate(pg.tps, FmtStyle::Compact),
        PG_TPS,
    ));
    spans.push(Span::raw(" "));

    let hit_style = if pg.hit_ratio < 90.0 {
        Styles::critical()
    } else if pg.hit_ratio < 95.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "hit",
        &format!("{:.1}%", pg.hit_ratio),
        PG_HIT,
        hit_style,
    ));
    spans.push(Span::raw(" "));

    let iohr_style = if pg.backend_io_hit < 90.0 {
        Styles::critical()
    } else if pg.backend_io_hit < 95.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "iohr",
        &format!("{:.1}%", pg.backend_io_hit),
        PG_IOHR,
        iohr_style,
    ));
    spans.push(Span::raw(" "));

    spans.extend(metric_spans_default(
        "tup",
        &fmt::format_rate(pg.tup_s, FmtStyle::Compact),
        PG_TUP,
    ));
    spans.push(Span::raw(" "));

    let tmp_style = if pg.tmp_bytes_s > 100.0 * 1024.0 * 1024.0 {
        Styles::critical()
    } else if pg.tmp_bytes_s > 0.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "tmp",
        &fmt::format_bytes_rate(pg.tmp_bytes_s, FmtStyle::Compact),
        PG_TMP,
        tmp_style,
    ));
    spans.push(Span::raw(" "));

    let dlock_style = if pg.deadlocks > 0 {
        Styles::critical()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "dlock",
        &format!("{}", pg.deadlocks),
        PG_DLOCK,
        dlock_style,
    ));
    spans.push(Span::raw(" "));

    let err_style = if pg.errors > 0 {
        Styles::critical()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "err",
        &format!("{}", pg.errors),
        PG_ERR,
        err_style,
    ));

    line_with_padding(spans, width)
}

/// Renders PostgreSQL bgwriter summary line.
/// Format: BGW │ ckpt:  0.0/m  wr:      0ms  be:        0  cln:    224/s  mxw:       0  alloc:    770/s
pub(super) fn render_bgw_line(bgw: &BgwSummary, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("BGW", Styles::cpu()), Span::raw(" │ ")];

    let ckpt_style = if bgw.checkpoints_per_min > 2.0 {
        Styles::critical()
    } else if bgw.checkpoints_per_min > 0.5 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "ckpt",
        &format!("{:.1}/m", bgw.checkpoints_per_min),
        BGW_CKPT,
        ckpt_style,
    ));
    spans.push(Span::raw(" "));

    let wr_style = if bgw.ckpt_write_time_ms >= 30_000.0 {
        Styles::critical()
    } else if bgw.ckpt_write_time_ms >= 5_000.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "wr",
        &fmt::format_ms(bgw.ckpt_write_time_ms, FmtStyle::Compact),
        BGW_WR,
        wr_style,
    ));
    spans.push(Span::raw(" "));

    let be_style = if bgw.buffers_backend_s > 100.0 {
        Styles::critical()
    } else if bgw.buffers_backend_s > 0.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "be",
        &fmt::format_rate(bgw.buffers_backend_s, FmtStyle::Compact),
        BGW_BE,
        be_style,
    ));
    spans.push(Span::raw(" "));

    spans.extend(metric_spans_default(
        "cln",
        &fmt::format_rate(bgw.buffers_clean_s, FmtStyle::Compact),
        BGW_CLN,
    ));
    spans.push(Span::raw(" "));

    let mxw_style = if bgw.maxwritten_clean > 10 {
        Styles::critical()
    } else if bgw.maxwritten_clean > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "mxw",
        &format!("{}", bgw.maxwritten_clean),
        BGW_MXW,
        mxw_style,
    ));
    spans.push(Span::raw(" "));

    spans.extend(metric_spans_default(
        "alloc",
        &fmt::format_rate(bgw.buffers_alloc_s, FmtStyle::Compact),
        BGW_ALLOC,
    ));

    line_with_padding(spans, width)
}

/// Renders MEM line - fixed-width metrics.
pub(super) fn render_cgroup_mem_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let Some(mem) = metrics.cgroup_memory.as_ref() else {
        return render_mem_line(metrics, width);
    };

    let used_style = if mem.max > 0 {
        let used_percent = (mem.current as f64 / mem.max as f64) * 100.0;
        if used_percent > 95.0 {
            Styles::critical()
        } else if used_percent > 80.0 {
            Styles::modified_item()
        } else {
            Styles::default()
        }
    } else {
        Styles::default()
    };

    let oom_style = if mem.oom_kill > 0 {
        Styles::critical()
    } else {
        Styles::default()
    };

    let mut spans = vec![Span::styled("MEM", Styles::mem()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "lim",
        &format_size_bytes(mem.max),
        MEM_TOT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "used",
        &format_size_bytes(mem.current),
        MEM_AVAIL,
        used_style,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "anon",
        &format_size_bytes(mem.anon),
        MEM_CACHE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "file",
        &format_size_bytes(mem.file),
        MEM_CACHE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "slab",
        &format_size_bytes(mem.slab),
        MEM_SLAB,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "oom",
        &format!("{}", mem.oom_kill),
        CG_OOM,
        oom_style,
    ));

    line_with_padding(spans, width)
}

pub(super) fn render_mem_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled("MEM", Styles::mem()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "tot",
        &format_size_gib(metrics.mem_total),
        MEM_TOT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "avail",
        &format_size_gib(metrics.mem_available),
        MEM_AVAIL,
        style_for_mem_free(metrics.mem_available, metrics.mem_total),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "cache",
        &format_size_gib(metrics.mem_cached),
        MEM_CACHE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "buf",
        &format_size_gib(metrics.mem_buffers),
        MEM_BUF,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "slab",
        &format_size_gib(metrics.mem_slab),
        MEM_SLAB,
        style_for_slab(metrics.mem_slab),
    ));

    line_with_padding(spans, width)
}

/// Renders SWP line - fixed-width metrics.
pub(super) fn render_swp_line(metrics: &SummaryMetrics, width: usize) -> Line<'static> {
    let swap_used = metrics.swap_total.saturating_sub(metrics.swap_free);

    let mut spans = vec![Span::styled("SWP", Styles::mem()), Span::raw(" │ ")];

    spans.extend(metric_spans_default(
        "tot",
        &format_size_gib(metrics.swap_total),
        MEM_TOT,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans_default(
        "free",
        &format_size_gib(metrics.swap_free),
        SWP_FREE,
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "swpd",
        &format_size_gib(swap_used),
        SWP_SWPD,
        style_for_swap(swap_used),
    ));
    spans.push(Span::raw("  "));
    spans.extend(metric_spans(
        "dirty",
        &format_size_gib(metrics.mem_dirty),
        SWP_DIRTY,
        style_for_dirty(metrics.mem_dirty),
    ));
    spans.push(Span::raw("  "));
    let wback_style = if metrics.mem_writeback > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "wback",
        &format_size_gib(metrics.mem_writeback),
        SWP_WBACK,
        wback_style,
    ));

    line_with_padding(spans, width)
}

/// Formats disk name with fixed width padding (right-padded name + colon)
fn fmt_disk_name(name: &str) -> String {
    let truncated = if name.len() > DEV_NAME {
        &name[..DEV_NAME]
    } else {
        name
    };
    format!("{:width$}:", truncated, width = DEV_NAME)
}

/// Renders empty DSK line when no disk data.
pub(super) fn render_dsk_empty_line(width: usize) -> Line<'static> {
    let spans = vec![
        Span::styled("DSK", Styles::disk()),
        Span::raw(" │ "),
        Span::styled("(no disk data)", Styles::dim()),
    ];
    line_with_padding(spans, width)
}

/// Renders a single DSK line for one disk device.
/// Format: DSK │ vda:   rMB:   4.0  wMB:   1.0  rd/s:   396  wr/s:    74  aw:   0.2  ut:   26%
pub(super) fn render_single_dsk_line(
    disk: &DiskSummary,
    is_first: bool,
    width: usize,
) -> Line<'static> {
    let label = if is_first {
        Span::styled("DSK", Styles::disk())
    } else {
        Span::styled("dsk", Styles::dim())
    };

    let mut spans = vec![label, Span::raw(" │ ")];

    spans.push(Span::styled(fmt_disk_name(&disk.name), Styles::dim()));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rMB",
        &format!("{:.1}", disk.read_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "wMB",
        &format!("{:.1}", disk.write_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rd/s",
        &format!("{:.0}", disk.r_iops),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "wr/s",
        &format!("{:.0}", disk.w_iops),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    let await_max = disk.r_await.max(disk.w_await);
    spans.extend(metric_spans(
        "aw",
        &format!("{:.1}", await_max),
        IO_STAT,
        style_for_await(await_max),
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "ut",
        &format!("{:.0}%", disk.util),
        IO_STAT,
        style_for_disk_util(disk.util),
    ));

    line_with_padding(spans, width)
}

/// Formats network interface name with fixed width padding (right-padded name + colon)
fn fmt_net_name(name: &str) -> String {
    let truncated = if name.len() > DEV_NAME {
        &name[..DEV_NAME]
    } else {
        name
    };
    format!("{:width$}:", truncated, width = DEV_NAME)
}

/// Renders empty NET line when no network data.
pub(super) fn render_net_empty_line(width: usize) -> Line<'static> {
    let spans = vec![
        Span::styled("NET", Styles::default()),
        Span::raw(" │ "),
        Span::styled("(no network data)", Styles::dim()),
    ];
    line_with_padding(spans, width)
}

/// Renders a single NET line for one network interface.
/// Format: NET │ ens2:  rxMB:  0.2  txMB:  0.5  rxPk:    1K  txPk:  833  er:     0  dr:     0
pub(super) fn render_single_net_line(
    net: &NetSummary,
    is_first: bool,
    width: usize,
) -> Line<'static> {
    let label = if is_first {
        Span::styled("NET", Styles::default())
    } else {
        Span::styled("net", Styles::dim())
    };

    let mut spans = vec![label, Span::raw(" │ ")];

    spans.push(Span::styled(fmt_net_name(&net.name), Styles::dim()));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rxMB",
        &format!("{:.1}", net.rx_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "txMB",
        &format!("{:.1}", net.tx_mb_s),
        IO_MB,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "rxPk",
        &format_pkt_rate(net.rx_pkt_s),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans_default(
        "txPk",
        &format_pkt_rate(net.tx_pkt_s),
        IO_OPS,
    ));
    spans.push(Span::raw(" "));
    spans.extend(metric_spans(
        "er",
        &format!("{}", net.errors),
        IO_STAT,
        style_for_net_errors(net.errors),
    ));
    spans.push(Span::raw(" "));
    let total_drops = net.rx_drp_s + net.tx_drp_s;
    let drp_style = if total_drops > 0.0 {
        Styles::critical()
    } else {
        Styles::default()
    };
    spans.extend(metric_spans(
        "dr",
        &format!("{:.0}", total_drops),
        IO_STAT,
        drp_style,
    ));

    line_with_padding(spans, width)
}

/// Renders help line - compact layout with context-sensitive hints.
pub(super) fn render_help_line(width: usize, tab: Tab) -> Line<'static> {
    let mut spans = vec![
        Span::styled("q", Styles::help_key()),
        Span::styled(":quit(", Styles::help()),
        Span::styled("qq", Styles::help_key()),
        Span::styled("/", Styles::help()),
        Span::styled("Enter", Styles::help_key()),
        Span::styled(") ", Styles::help()),
        Span::styled("s", Styles::help_key()),
        Span::styled(":sort ", Styles::help()),
        Span::styled("r", Styles::help_key()),
        Span::styled(":rev ", Styles::help()),
        Span::styled("/", Styles::help_key()),
        Span::styled(":filter ", Styles::help()),
        Span::styled("b", Styles::help_key()),
        Span::styled(":time ", Styles::help()),
    ];

    // Tab-specific hints
    match tab {
        Tab::Processes => {
            spans.push(Span::styled("g/c/m/d", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
        }
        Tab::PostgresActive => {
            spans.push(Span::styled("g/v", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
            spans.push(Span::styled("i", Styles::help_key()));
            spans.push(Span::styled(":hide idle ", Styles::help()));
            spans.push(Span::styled(">", Styles::help_key()));
            spans.push(Span::styled(":drill ", Styles::help()));
        }
        Tab::PgStatements => {
            spans.push(Span::styled("t/c/i/e", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
        }
        Tab::PgTables => {
            spans.push(Span::styled("a/w/x/n/i", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
            spans.push(Span::styled(">", Styles::help_key()));
            spans.push(Span::styled(":drill ", Styles::help()));
        }
        Tab::PgIndexes => {
            spans.push(Span::styled("u/w/i", Styles::help_key()));
            spans.push(Span::styled(":view ", Styles::help()));
        }
        Tab::PgErrors => {}
        Tab::PgLocks => {
            spans.push(Span::styled(">", Styles::help_key()));
            spans.push(Span::styled(":drill ", Styles::help()));
        }
    }

    spans.push(Span::styled("?", Styles::help_key()));
    spans.push(Span::styled(":help", Styles::help()));

    line_with_padding(spans, width)
}

// ============================================================================
// Styling functions (anomaly highlighting)
// ============================================================================

/// Style for load average values.
fn style_for_load(load: f64, num_cpus: f64) -> Style {
    if load > num_cpus * 2.0 {
        Styles::critical()
    } else if load > num_cpus {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for running processes count.
fn style_for_running(running: u32, num_cpus: f64) -> Style {
    let running = running as f64;
    if running > num_cpus * 2.0 {
        Styles::critical()
    } else if running > num_cpus {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU sys%.
fn style_for_cpu_sys(sys: f64) -> Style {
    if sys > 40.0 {
        Styles::critical()
    } else if sys > 20.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU irq%.
fn style_for_cpu_irq(irq: f64) -> Style {
    if irq > 15.0 {
        Styles::critical()
    } else if irq > 5.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU iowait%.
fn style_for_cpu_iow(iow: f64) -> Style {
    if iow > 30.0 {
        Styles::critical()
    } else if iow > 10.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU idle%.
fn style_for_cpu_idle(idle: f64) -> Style {
    if idle < 5.0 {
        Styles::critical()
    } else if idle < 20.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for CPU steal%.
fn style_for_cpu_steal(steal: f64) -> Style {
    if steal > 15.0 {
        Styles::critical()
    } else if steal > 5.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for memory available/free.
fn style_for_mem_free(available: u64, total: u64) -> Style {
    if total == 0 {
        return Styles::default();
    }
    let percent = (available as f64 / total as f64) * 100.0;
    if percent < 5.0 {
        Styles::critical()
    } else if percent < 15.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for slab memory (in KB).
fn style_for_slab(slab_kb: u64) -> Style {
    let slab_gib = slab_kb as f64 / (1024.0 * 1024.0);
    if slab_gib > 5.0 {
        Styles::critical()
    } else if slab_gib > 3.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for swap used (in KB).
fn style_for_swap(swap_used_kb: u64) -> Style {
    let swap_gib = swap_used_kb as f64 / (1024.0 * 1024.0);
    if swap_gib > 1.0 {
        Styles::critical()
    } else if swap_used_kb > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for dirty pages (in KB).
fn style_for_dirty(dirty_kb: u64) -> Style {
    let dirty_mib = dirty_kb as f64 / 1024.0;
    if dirty_mib > 2048.0 {
        Styles::critical()
    } else if dirty_mib > 500.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for disk utilization%.
fn style_for_disk_util(util: f64) -> Style {
    if util > 80.0 {
        Styles::critical()
    } else if util > 50.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for disk await (ms).
fn style_for_await(await_ms: f64) -> Style {
    if await_ms > 100.0 {
        Styles::critical()
    } else if await_ms > 20.0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

/// Style for network errors.
fn style_for_net_errors(errors: u64) -> Style {
    if errors > 100 {
        Styles::critical()
    } else if errors > 0 {
        Styles::modified_item()
    } else {
        Styles::default()
    }
}

// ============================================================================
// Formatting helpers
// ============================================================================

/// Formats size in KB to human-readable GiB/MiB.
fn format_size_gib(kb: u64) -> String {
    let mib = kb as f64 / 1024.0;
    if mib >= 1024.0 {
        format!("{:.1} GiB", mib / 1024.0)
    } else if mib >= 1.0 {
        format!("{:.0} MiB", mib)
    } else {
        format!("{} KiB", kb)
    }
}

/// Formats size in bytes to human-readable GiB/MiB.
fn format_size_bytes(bytes: u64) -> String {
    // The rest of the summary formatting is KB-based, so keep it consistent.
    format_size_gib(bytes / 1024)
}

/// Formats packet rate to compact form (K/M suffix).
fn format_pkt_rate(pkt_s: f64) -> String {
    if pkt_s >= 1_000_000.0 {
        format!("{:.1}M", pkt_s / 1_000_000.0)
    } else if pkt_s >= 1_000.0 {
        format!("{:.0}K", pkt_s / 1_000.0)
    } else {
        format!("{:.0}", pkt_s)
    }
}
