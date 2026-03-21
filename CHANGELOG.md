# Changelog

All notable changes to this project will be documented in this file.

## [v0.2.0] - 2026-03-21

### Architecture & Refactoring
- **Modular UI Screens**: Extracted all screen rendering functions from monolithic `render.rs` into dedicated modules (`overview.rs`, `channel_list.rs`, `messages.rs`, `support.rs`, `activity.rs`, `settings.rs`, `download.rs`) for improved maintainability and code organization.

### Features
- **Analysis Abort Support**: Added cancellation capability to the analysis engine. Analysis now accepts an atomic abort flag that can be triggered to stop long-running operations.
- **Step-Resolved Progress**: Enhanced progress tracking with a dedicated `AnalysisStep` enum to provide more granular status updates during analysis phases.
- **Back Navigation from Processing**: Added 'b' key binding to navigate back from Analyzing and Downloading screens without waiting for completion.

### Performance & Efficiency
- **Incremental Analysis Engine**: Integrated file modification time (`mtime`) tracking. Re-analyzing unchanged exports is now near-instant, only processing new or updated channels and activity logs.
- **Parallel Analysis**: Implemented a thread-pooled message analyzer. Multiple channels are now processed simultaneously, providing massive speedups for large exports on multi-core CPUs.
- **Streaming Message Previews**: Refactored the message preview system to use a high-performance tail-reader. For large channels, it now reads only the last few hundred messages from the file, resulting in a **90%+ reduction in memory usage**.
- **Optimized Data Aggregation**: Global stats now merge from cached per-channel data using deterministic `BTreeMap` structures.

### UI & Experience
- **Attachment Gallery Browser**: Added a dedicated screen to explore downloaded media. Features category-based filtering (All, Images, Videos, etc.) with high-performance list rendering and mouse support.
- **Enhanced Status Bar**: Redesigned the status bar with color-coded shortcuts and context-sensitive help for every screen.
- **Inline Key Shortcuts**: Critical actions like `[Enter] Open` and `[B/Esc] Back` are now displayed prominently in block titles throughout the app.
- **Improved Discovery**: All navigation hints are now highlighted in Cyan for better visibility.

### Architecture & Under-the-hood
- **Centralized Utilities**: Created a unified `src/data/utils.rs` module. Eliminated severe code duplication across `analyzer.rs`, `messages.rs`, `support.rs`, and `activity.rs`.
- **Global NDJSON Support**: Unified data parsing. All files (messages, tickets, activity) now transparently handle both formatted JSON and Newline-Delimited JSON (NDJSON).
- **Hardened Error Handling**: Improved error context for filesystem operations across the analysis pipeline.

### Bug Fixes
- **UI Freeze and Hang Resolution**: Completely resolved a critical issue where the application would freeze during tab navigation or interactions with large datasets.
- **Background Threading**: Refactored `Channel`, `Support`, `Activity`, and `Gallery` data loaders to operate entirely on background threads. Data loading no longer blocks the main UI thread.
- **Lazy Caching System**: Implemented an advanced lazy caching system to skip redundant disk reads and reuse in-memory data for instant channel navigation.
- **Streaming Record Counts**: Optimized `data::utils::count_records` to use a stream-based count. Huge JSON and NDJSON files are no longer fully loaded into memory just to determine their length.
- **Universal Exit Guarantee**: Moved `Ctrl+C` interrupt handling directly into the application's root loop so users can safely exit even during intensive ops.
- Fixed potential memory blowouts when previewing channels with 100k+ messages.
- Resolved inconsistent date parsing between different export sections.
---

## [v0.1.2] - 2026-03-11

### Highlights
- Added dedicated Support Tickets and Activity Explorer flows in the TUI.
- Full mouse support for tab switching, menu selection, and list scrolling.

### Added
- **New App Screens**:
    - Support ticket list + ticket detail view.
    - Activity event list + event detail view.
- **Activity Explorer Controls**:
    - Real-time filtering by query (`/`), event type (`t`), source file (`y`), and date range (`[` / `]`).
    - Sort mode cycling (`o`) and filter clearing (`c`).
- **Memory-safe Loading**:
    - Activity explorer now reads file tails to handle massive log files without crashing.
- **Extended Metrics**:
    - Tracked avg comments per ticket, tickets by priority, and activity frequency by month.

### Changed
- Home menu structure expanded to include **Support** and **Activity** entries.
- Overview screen redesigned to surface richer support-ticket statistics.
- **Major Architecture Refactor**:
    - Split monolithic `main.rs` into specialized `app`, `input`, `ui`, and `data` modules.
    - Implemented dedicated data loaders for all major Discord data types.

### Fixed
- Stabilized `Tab` / `Shift+Tab` navigation across all screens.
- Navigation now correctly skips disabled sections with explanatory status messages.
- Improved cursor/scroll reset logic when switching between detailed views.

---

## [v0.1.1] - 2026-03-04

### Added
- **Advanced Attachment Deduplication**:
    - Implemented SHA-256 content hashing for all downloads (`sha2` crate).
    - Added a persistent content-hash index (`attachment_hash_index.json`) for cross-project deduplication.
    - Downloads now use a "stream-to-hash" temp file approach to prevent partial or corrupt downloads.
    - Added in-flight hash guards to prevent multiple workers from downloading the same content simultaneously.
- **Improved Progress Reporting**:
    - Downloader now reports detailed stats: saved, existing, dup-content, failed, and dup-url.
- **Regression Testing**: Added a comprehensive deduplication test suite in `src/downloader.rs`.

### Changed
- URL-path deduplication now executes as an initial fast-pass before content hashing.
- Existing local files are automatically indexed to improve future deduplication speed.
