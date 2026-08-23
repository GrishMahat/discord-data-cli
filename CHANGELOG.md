# Changelog

All notable changes to this project will be documented in this file.

## [v0.3.0] - 2026-08-23

### Highlights
- **Global Message Search**: New dedicated search screen — full-text search across every channel with streaming results, match highlighting, and type/content filters. Open it with `/` from any main screen, the sidebar, or the Support & Activity "Search" tab.
- **Insights Engine**: A new analysis pass turns the raw export into human-readable analytics — billing timeline, privacy posture, DM contacts, link intelligence, voice/RTC quality, device profile, sessionization, and personal records — rendered on a new scrollable Insights screen and written to `results-rs/summaries/*.json`.
- **Compare Two Exports**: Every analysis now saves a snapshot of headline stats. The new Compare screen diffs any snapshot against the previous one (messages, servers, DMs, links, voice, active days, top channel/contact/word) with green/red deltas.
- **Activity Heatmap**: GitHub-style contribution grid of daily message activity on a new Activity Map screen, with quantile color levels, month labels, and `←`/`→` year navigation.
- **Multi-Year Activity Wall**: The Activity Map now shows up to six year-minimaps at once (3×2 grid), each with per-year message totals and active-day counts, all sharing one color scale for instant cross-year comparison. More than six years? `←`/`→` pages through the rest.
- **Split-Pane Channel Browser**: The flat channel list is now a two-pane browser — channels on the left, a background-loaded live message preview plus a per-channel Stats panel (first/last dates, attachments, emoji, top words) on the right.
- **Headless Analysis**: New `--analyze` CLI flag runs the full analysis without the TUI, prints an insight digest, and exits (cron/scheduled re-analysis friendly).

### Features
- **Message Search** (`src/ui/screens/search.rs`):
    - Query input with paste support; Enter runs the search, results stream in as files are scanned.
    - Filter pills for channel type (All/DMs/Groups/Threads) and content (Any/Attachments/Links); changing filters re-runs automatically.
    - Results show kind label, channel title, timestamp, and a snippet with the matched text highlighted; Enter opens the message's channel in full context.
    - Cancel-safe via generation counter; capped at 400 matches with a "(capped)" note.
- **Insights Screen** (`src/ui/screens/insights.rs`):
    - Headline pills (payments, active days, voice hours, streak record) over scrollable sections: Records, Billing, Voice/RTC Quality, Devices & Clients, Sessions, Contacts, Top Servers, Link Intelligence, Privacy Posture.
    - Privacy posture stores presence flags only (MFA, verified, phone attached, payment sources, data-access requests) — no emails, phone numbers or IPs are persisted.
- **Voice/RTC Quality**: connections vs disconnects/reconnects, average ping/MOS/connect time, packet loss, connected/speaking/listening minutes, minutes per network type, top media relay hosts, disconnect reasons.
- **Billing Timeline**: payment history (amount/currency/gateway/refunds), totals per currency, gateway breakdown, entitlement and virtual-currency transaction counts.
- **Personal Records**: first/last message dates, longest message ever (chars + date), biggest channel, busiest telemetry day, longest daily streak.
- **Contacts**: ranked DM contacts and group DMs with message counts and first → last interaction dates.
- **Server Engagement**: servers ranked by telemetry events per guild (resolved against `Servers/index.json`) with audit-log entry counts.
- **Settings Redesign**: grouped form layout (General / Display / Downloads / Privacy) with path entries opening the Setup wizard, adjustable preview length, auto-download toggle, full mouse and scroll support.
- **Home Dashboard Polish**: Top Channels now render proportional bars; Quick Actions (`R` Re-analyze, `D` Download, `E` Export) are real working key bindings.
- **Channel Sort Orders**: The channel browser can sort by most messages (default), name (A-Z), or most recently active — cycle with `O`. Counts render as activity-tier badges (bright for busy channels, ghosted for empty ones).
- **Gallery Count Badges**: Every category tab shows its file count inline.
- **Broader Terminal Image Support**: In-terminal image viewing now works beyond iTerm — Kitty, sixel, and plain Unicode half-block terminals all render directly, falling back to the OS opener only if rendering fails.

### Performance & Efficiency
- **Single Deduplicated Telemetry Pass**: The four Activity subfolders (analytics/modeling/reporting/tns) contain overlapping copies of the same events (identical `event_id`). All aggregation now happens in one global pass with id-based deduplication (~1.4M duplicates skipped on the reference export), fixing ~4x inflated activity totals.
- **Signature-Cached Aggregates**: The multi-GB event scan runs only when input mtimes/sizes change (signature also mixes an aggregate schema version). Cold scan ≈60s over 5.9 GB; warm analyses complete in well under a second.
- **Streaming Search Reader**: New `data::utils::stream_records` parses JSON arrays/NDJSON incrementally instead of loading whole files.
- **Per-Day Extraction**: Daily message counts ride the existing per-channel mtime cache (`CHANNEL_CACHE_VERSION` bumps force a single cheap reparse when extraction logic changes).

### UI & Experience
- Sidebar expanded to Dashboard / Analyze Now / Overview / Insights / Compare / Activity Map / Support / Activity / Search / Channels / Gallery / Download / Settings / Quit, with disabled-state reasons preserved.
- Live preview pane shows a spinner while loading and pins to the live tail; `,` / `.` or mouse wheel page upward through history.
- Status-bar help strings updated for every new screen.

### Architecture & Under-the-hood
- New modules: `src/insights/`, `src/compare/`, plus screens/handlers for search, insights, compare, and activity map.
- Analyzer pipeline is now 10 steps, with the Activity step producing both `stats.activity` and the insight aggregates from one scan; the old per-file `activity_cache` was removed in favor of the shared signature-gated cache.
- Per-channel cache carries a `cache_version`; per-file aggregates carry `EVENT_AGGREGATE_VERSION`.
- Analysis writes `summaries/*.json` and appends to `snapshots/` during the Writing step; snapshot writes are skipped when nothing changed.

---
## [v0.2.0] - 2026-03-21

### Architecture & Refactoring
- **Modular UI Screens**: Extracted all screen rendering functions from monolithic `render.rs` into dedicated modules (`overview.rs`, `channel_list.rs`, `messages.rs`, `support.rs`, `activity.rs`, `settings.rs`, `download.rs`) for improved maintainability and code organization.

### Features
- **Terminal Image Viewer**: Pressing Enter on an image file in the Gallery now renders it directly in the terminal via iTerm/Sixel graphics protocol (viuer). Non-image files still open with the OS default app. Falls back to `open::that()` when terminal image support is unavailable.
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
