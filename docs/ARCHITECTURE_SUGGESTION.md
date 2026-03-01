# Architecture Suggestion (Proposal Only)

This is a **proposal** for a cleaner, scalable file structure.
No implementation is included in this document.

## Goals

- Reduce `main.rs` and single-file growth risk.
- Make naming explicit by domain and responsibility.
- Keep data parsing, app state, input, and rendering independent.
- Make future features (support/activity/search/export) additive.

## Suggested Top-Level Layout

```text
src/
  main.rs
  lib.rs

  app/
    mod.rs
    state.rs                # AppState + screen routing state
    actions.rs              # High-level transitions (open_x, refresh_x)
    availability.rs         # Feature enable/disable rules + reason text
    keys.rs                 # Keybinding maps per screen

  domain/
    mod.rs
    analysis.rs             # analyzer-facing summary models
    support.rs              # support ticket models
    activity.rs             # activity event models
    messages.rs             # message/channel models

  data/
    mod.rs
    loaders/
      mod.rs
      support_loader.rs     # support source loading/parsing
      activity_loader.rs    # activity tail-reading/loading
      messages_loader.rs    # message index + previews
    normalize.rs            # shared value/string/date normalization helpers

  ui/
    mod.rs
    layout.rs               # shared layout split helpers
    theme.rs                # styles/colors
    screens/
      mod.rs
      home.rs
      overview.rs
      support_list.rs
      support_detail.rs
      activity_list.rs
      activity_detail.rs
      channels.rs
      message_view.rs
      settings.rs
      setup.rs
    widgets/
      mod.rs
      stats_panel.rs
      filter_bar.rs
      list_table.rs

  input/
    mod.rs
    mouse.rs                # mouse routing + hit-testing
    key.rs                  # key routing entry
    handlers/
      mod.rs
      home.rs
      support.rs
      activity.rs
      channels.rs
      message_view.rs
      settings.rs
      setup.rs

  services/
    mod.rs
    analysis_service.rs     # run analysis + progress channel
    download_service.rs     # attachment download flow

  util/
    mod.rs
    format.rs               # fmt_num, truncate, duration formatting
    time.rs                 # date extraction/normalization
    fs.rs                   # path/source directory helpers
```

## Naming Changes to Consider

- `support_activity.rs` -> split into:
  - `data/loaders/support_loader.rs`
  - `data/loaders/activity_loader.rs`
- `app_state.rs` -> split into:
  - `app/state.rs`
  - `app/actions.rs`
  - `app/availability.rs`
- `input.rs` -> split into:
  - `input/key.rs`
  - `input/mouse.rs`
  - `input/handlers/*`
- `ui.rs` -> split into:
  - `ui/screens/*`
  - `ui/widgets/*`
  - `ui/layout.rs`

## Migration Plan (Non-breaking, Incremental)

1. Introduce `lib.rs` and module folders while keeping old file paths working.
2. Move pure helpers first (`format`, `normalize`, `fs`).
3. Move `ui` screen-by-screen (home/overview first).
4. Move input handlers by screen.
5. Split data loaders (`support`, `activity`, `messages`).
6. Move availability rules and app actions.
7. Remove old aggregate files once all calls are redirected.

## Notes

- Keep behavior identical during migration; each step should compile and pass checks.
- Prefer small PR-sized moves to keep regressions isolated.
