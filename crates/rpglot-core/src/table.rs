//! Generic table widget state: sorting, filtering, diff tracking.

use std::collections::HashMap;

/// Sort key types for table columns.
#[derive(Debug, Clone, PartialEq)]
pub enum SortKey {
    Integer(i64),
    Float(f64),
    String(String),
}

impl PartialOrd for SortKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (SortKey::Integer(a), SortKey::Integer(b)) => a.partial_cmp(b),
            (SortKey::Float(a), SortKey::Float(b)) => a.partial_cmp(b),
            (SortKey::String(a), SortKey::String(b)) => a.partial_cmp(b),
            _ => None,
        }
    }
}

/// Diff status for highlighting changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum DiffStatus {
    /// New item (green).
    New,
    /// Modified item with changed column indices (yellow).
    Modified(Vec<usize>),
    /// No changes.
    #[default]
    Unchanged,
}

/// Column type for adaptive width calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnType {
    /// Fixed width based on content (PID, S, THR).
    Fixed,
    /// Flexible width that adapts to content (RUID, EUID, CPU%).
    Flexible,
    /// Expandable column that takes remaining space (CMD, CMDLINE).
    Expandable,
}

/// Cached column widths for all view modes.
#[derive(Debug, Clone, Default)]
pub struct CachedWidths {
    /// Widths for Generic view mode.
    pub generic: Vec<u16>,
    /// Widths for Command view mode.
    pub command: Vec<u16>,
    /// Widths for Memory view mode.
    pub memory: Vec<u16>,
    /// Widths for Disk view mode.
    pub disk: Vec<u16>,
}

/// Trait for table row items.
pub trait TableRow: Clone {
    /// Unique identifier for diff tracking.
    fn id(&self) -> u64;

    /// Number of columns.
    fn column_count() -> usize;

    /// Column headers.
    fn headers() -> Vec<&'static str>;

    /// Cell values as strings.
    fn cells(&self) -> Vec<String>;

    /// Sort key for the specified column.
    fn sort_key(&self, column: usize) -> SortKey;

    /// Check if item matches the filter.
    fn matches_filter(&self, filter: &str) -> bool;
}

/// State for a table widget.
#[derive(Debug, Clone)]
pub struct TableState<T: TableRow> {
    /// All items (unfiltered).
    pub items: Vec<T>,
    /// Selected row index (in filtered view).
    pub selected: usize,
    /// Sort column index.
    pub sort_column: usize,
    /// Sort direction (true = ascending).
    pub sort_ascending: bool,
    /// Filter string.
    pub filter: Option<String>,
    /// Scroll offset for large tables.
    pub scroll_offset: usize,
    /// Previous items for diff tracking.
    previous: HashMap<u64, T>,
    /// Diff status for each item.
    pub diff_status: HashMap<u64, DiffStatus>,
    /// Tracked entity ID — follows the selected row across sort/filter changes.
    pub tracked_id: Option<u64>,
}

impl<T: TableRow> Default for TableState<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: TableRow> TableState<T> {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected: 0,
            sort_column: 0,
            sort_ascending: false, // Default descending (highest first)
            filter: None,
            scroll_offset: 0,
            previous: HashMap::new(),
            diff_status: HashMap::new(),
            tracked_id: None,
        }
    }

    /// Updates items and computes diff status.
    pub fn update(&mut self, new_items: Vec<T>) {
        // Compute diff status
        self.diff_status.clear();
        for item in &new_items {
            let id = item.id();
            let status = if let Some(prev) = self.previous.get(&id) {
                // Check which cells changed
                let prev_cells = prev.cells();
                let new_cells = item.cells();
                let changed: Vec<usize> = prev_cells
                    .iter()
                    .zip(new_cells.iter())
                    .enumerate()
                    .filter(|(_, (p, n))| p != n)
                    .map(|(i, _)| i)
                    .collect();

                if changed.is_empty() {
                    DiffStatus::Unchanged
                } else {
                    DiffStatus::Modified(changed)
                }
            } else {
                DiffStatus::New
            };
            self.diff_status.insert(id, status);
        }

        // Save current as previous for next update
        self.previous.clear();
        for item in &new_items {
            self.previous.insert(item.id(), item.clone());
        }

        self.items = new_items;
        self.apply_sort();

        // Adjust selection if needed
        let filtered_len = self.filtered_items().len();
        if self.selected >= filtered_len && filtered_len > 0 {
            self.selected = filtered_len - 1;
        }
    }

    /// Returns filtered and sorted items.
    pub fn filtered_items(&self) -> Vec<&T> {
        self.items
            .iter()
            .filter(|item| {
                self.filter
                    .as_ref()
                    .map(|f| item.matches_filter(f))
                    .unwrap_or(true)
            })
            .collect()
    }

    /// Applies current sort to items.
    fn apply_sort(&mut self) {
        let col = self.sort_column;
        let asc = self.sort_ascending;

        self.items.sort_by(|a, b| {
            let key_a = a.sort_key(col);
            let key_b = b.sort_key(col);
            let cmp = key_a
                .partial_cmp(&key_b)
                .unwrap_or(std::cmp::Ordering::Equal);
            if asc { cmp } else { cmp.reverse() }
        });
    }

    /// Cycles to next sort column.
    pub fn next_sort_column(&mut self) {
        self.sort_column = (self.sort_column + 1) % T::column_count();
        self.apply_sort();
    }

    /// Toggles sort direction.
    pub fn toggle_sort_direction(&mut self) {
        self.sort_ascending = !self.sort_ascending;
        self.apply_sort();
    }

    /// Sets filter string.
    pub fn set_filter(&mut self, filter: Option<String>) {
        self.filter = filter;
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Moves selection up.
    pub fn select_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            self.tracked_id = None;
        }
    }

    /// Moves selection down.
    pub fn select_down(&mut self) {
        let max = self.filtered_items().len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
            self.tracked_id = None;
        }
    }

    /// Moves selection up by a page.
    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
        self.tracked_id = None;
    }

    /// Moves selection down by a page.
    pub fn page_down(&mut self, page_size: usize) {
        let max = self.filtered_items().len().saturating_sub(1);
        self.selected = (self.selected + page_size).min(max);
        self.tracked_id = None;
    }

    /// Resolves selection by tracked entity ID.
    /// If the tracked entity is found in the current filtered items, moves
    /// `selected` to its new index. If not found, clears `tracked_id` and
    /// clamps `selected`. Always updates `tracked_id` from the current row.
    pub fn resolve_selection(&mut self) {
        // Collect IDs to avoid borrow conflict with self
        let ids: Vec<u64> = self.filtered_items().iter().map(|item| item.id()).collect();
        let len = ids.len();
        if len == 0 {
            self.selected = 0;
            self.tracked_id = None;
            return;
        }

        // Try to find tracked entity in current filtered list
        if let Some(tid) = self.tracked_id {
            if let Some(pos) = ids.iter().position(|&id| id == tid) {
                self.selected = pos;
            } else {
                // Entity disappeared — clamp selection
                self.tracked_id = None;
                if self.selected >= len {
                    self.selected = len - 1;
                }
            }
        } else if self.selected >= len {
            self.selected = len - 1;
        }

        // Update tracked_id from current selection
        if let Some(&id) = ids.get(self.selected) {
            self.tracked_id = Some(id);
        }
    }
}
