# Provider Architecture

This document describes the provider subsystem for rpglot, designed to abstract snapshot data sources for the TUI.

## Overview

The provider system implements a **Trait-based Provider** pattern that allows the TUI to work with different data sources through a unified interface:
- **Live mode**: Real-time data collection from the system (like `atop`)
- **History mode**: Historical data from storage files (like `atop -r file.atop`)

## Directory Structure

```
src/provider/
├── mod.rs          # Module exports and SnapshotProvider trait
├── provider.md     # This documentation
├── live.rs         # LiveProvider for real-time monitoring
└── history.rs      # HistoryProvider for historical data
```

## Core Components

### 1. SnapshotProvider Trait (`mod.rs`)

The main abstraction for snapshot data sources:

```rust
pub trait SnapshotProvider {
    fn current(&self) -> Option<&Snapshot>;
    fn advance(&mut self) -> Option<&Snapshot>;
    fn rewind(&mut self) -> Option<&Snapshot>;
    fn can_rewind(&self) -> bool;
    fn is_live(&self) -> bool;
    fn last_error(&self) -> Option<&ProviderError>;

    // Optional: support downcasting from Box<dyn SnapshotProvider>
    fn as_any(&self) -> Option<&dyn Any> { None }
    fn as_any_mut(&mut self) -> Option<&mut dyn Any> { None }
}
```

| Method | Description |
|--------|-------------|
| `current()` | Returns the current snapshot, if available |
| `advance()` | Moves to the next snapshot (collects new or moves cursor forward) |
| `rewind()` | Moves to the previous snapshot (history only) |
| `can_rewind()` | Returns `true` if provider supports rewinding |
| `is_live()` | Returns `true` if collecting live data |
| `last_error()` | Returns the last error for diagnostics |
| `as_any()` | Optional downcast hook for read-only access to concrete providers |
| `as_any_mut()` | Optional downcast hook for mutable access (e.g., history seek) |

### 2. LiveProvider (`live.rs`)

Provider for real-time system monitoring:

```rust
struct LiveProvider<F: FileSystem + Clone> {
    collector: Collector<F>,
    storage: Option<StorageManager>,
    current: Option<Snapshot>,
    last_error: Option<ProviderError>,
}
```

**Behavior:**
- `advance()` → calls `collector.collect_snapshot()`, optionally writes to storage
- `rewind()` → returns `None` (not supported)
- `can_rewind()` → returns `false`
- `is_live()` → returns `true`

**Usage:**
```rust
let fs = RealFs::new();
let collector = Collector::new(fs, "/proc");
let storage = Some(StorageManager::new("data"));
let mut provider = LiveProvider::new(collector, storage);

// Collect snapshots in a loop
loop {
    if let Some(snapshot) = provider.advance() {
        // Process snapshot
    }
    std::thread::sleep(Duration::from_secs(10));
}
```

### 3. HistoryProvider (`history.rs`)

Provider for viewing historical data from storage:

```rust
struct HistoryProvider {
    snapshots: Vec<Snapshot>,
    cursor: usize,
    last_error: Option<ProviderError>,
}
```

**Behavior:**
- `advance()` → moves cursor forward in history
- `rewind()` → moves cursor backward in history
- `can_rewind()` → returns `true`
- `is_live()` → returns `false`

**Additional Methods:**
| Method | Description |
|--------|-------------|
| `from_path(path)` | Loads snapshots from storage directory |
| `from_snapshots(vec)` | Creates provider from existing snapshots |
| `len()` | Returns total number of snapshots |
| `position()` | Returns current cursor position |
| `jump_to(pos)` | Jumps to specific position in history |
| `jump_to_timestamp_floor(ts)` | Jumps to the latest snapshot with `timestamp <= ts` |

**Usage:**
```rust
// Load from storage
let provider = HistoryProvider::from_path("data")?;

// Navigate history
while let Some(snapshot) = provider.advance() {
    // Process snapshot
}

// Go back
provider.rewind();

// Jump to specific position
provider.jump_to(10);
```

### 4. ProviderError (`mod.rs`)

Error types for diagnostics:

```rust
pub enum ProviderError {
    Io(String),         // I/O errors
    Collection(String), // Data collection errors
    Parse(String),      // Data parsing errors
}
```

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                                                             │
│   ┌───────────────┐                                         │
│   │ LiveProvider  │──┐                                      │
│   │ (Collector +  │  │                                      │
│   │  opt Storage) │  │    impl SnapshotProvider             │
│   └───────────────┘  │           │                          │
│                      ├───────────┼───────► ┌──────┐         │
│   ┌───────────────┐  │           │         │ TUI  │         │
│   │HistoryProvider│──┘           │         └──────┘         │
│   │ (Storage)     │              │                          │
│   └───────────────┘              │                          │
│                        trait SnapshotProvider               │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

## Design Decisions

### Why `Box<dyn SnapshotProvider>`?

We chose dynamic dispatch over generics for several reasons:
1. **Runtime switching**: Ability to switch between live and history mode
2. **Cleaner code**: No generic parameters propagating through App → TUI → ...
3. **Extensibility**: Easy to add new providers (RemoteProvider, MockProvider)
4. **Minimal overhead**: One vtable lookup per `advance()` call (every 10 seconds)

### Why `Option` instead of `Result`?

The `advance()` and `rewind()` methods return `Option<&Snapshot>`:
- `None` means "no more data" (end of history) — this is a normal situation, not an error
- For actual errors, check `last_error()` for diagnostics
- Keeps the API simple for the common use case

## Integration with TUI

```rust
fn main() {
    let args = parse_args();
    
    let provider: Box<dyn SnapshotProvider> = if let Some(file) = args.history_file {
        Box::new(HistoryProvider::from_path(file)?)
    } else {
        Box::new(LiveProvider::new(
            Collector::new(RealFs::new(), "/proc"),
            args.record_file.map(StorageManager::new),
        ))
    };
    
    let mut app = App::new(provider);
    app.run()
}
```

## Testing

Each provider has unit tests:
- `live.rs`: 3 tests (advance, cannot rewind, is_live)
- `history.rs`: 6 tests (creation, empty error, navigation, jump_to, jump_to_timestamp_floor, traits)

Run tests with:
```bash
cargo test provider
```
