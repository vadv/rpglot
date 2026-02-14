//! PostgreSQL lock tree (PGL) tab widget.
//! Thin TUI wrapper over [`crate::view::locks::build_locks_view`].

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Span;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Row, Table};

use crate::storage::StringInterner;
use crate::tui::state::AppState;
use crate::tui::style::Styles;
use crate::view::locks::build_locks_view;

pub fn render_pg_locks(
    frame: &mut Frame,
    area: Rect,
    state: &mut AppState,
    interner: Option<&StringInterner>,
) {
    let snapshot = match &state.current_snapshot {
        Some(s) => s,
        None => {
            let msg = Paragraph::new("No data available").block(
                Block::default()
                    .title("PGL: Lock Tree")
                    .borders(Borders::ALL),
            );
            frame.render_widget(Clear, area);
            frame.render_widget(msg, area);
            return;
        }
    };

    let vm = match build_locks_view(snapshot, &state.pgl, interner) {
        Some(vm) => vm,
        None => {
            // Either no nodes at all or filter yielded empty results
            let label = if state.pgl.filter.is_some() {
                "No matching rows (filter active)"
            } else {
                "No blocking chains detected"
            };
            let msg = Paragraph::new(label).block(
                Block::default()
                    .title("PGL: Lock Tree")
                    .borders(Borders::ALL),
            );
            frame.render_widget(Clear, area);
            frame.render_widget(msg, area);
            return;
        }
    };

    // Resolve selection
    let row_pids: Vec<i32> = vm.rows.iter().map(|r| r.id).collect();
    state.pgl.resolve_selection(&row_pids);

    // Header
    let header_cells: Vec<Span> = vm
        .headers
        .iter()
        .map(|h| Span::styled(h.clone(), Styles::table_header()))
        .collect();
    let header = Row::new(header_cells).style(Styles::table_header());

    // Widths
    let widths: Vec<ratatui::layout::Constraint> = vm
        .widths
        .iter()
        .map(|&w| ratatui::layout::Constraint::Min(w))
        .chain(std::iter::once(ratatui::layout::Constraint::Percentage(
            100,
        )))
        .collect();

    // Rows
    let rows: Vec<Row> = vm
        .rows
        .iter()
        .map(|vr| {
            let style = Styles::from_class(vr.style);
            let cells = vr.cells.iter().map(|c| match c.style {
                Some(s) => Span::styled(c.text.clone(), Styles::from_class(s)),
                None => Span::raw(c.text.clone()),
            });
            Row::new(cells).style(style)
        })
        .collect();

    let table = Table::new(rows, widths)
        .header(header)
        .block(
            Block::default()
                .title(vm.title)
                .borders(Borders::ALL)
                .style(Styles::default()),
        )
        .column_spacing(1)
        .row_highlight_style(Styles::selected());

    frame.render_widget(Clear, area);
    frame.render_stateful_widget(table, area, &mut state.pgl.ratatui_state);
}
