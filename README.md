# Discord Data Analyzer

[![GitHub License](https://img.shields.io/github/license/GrishMahat/discord-data-cli)](LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/GrishMahat/discord-data-cli)](https://github.com/GrishMahat/discord-data-cli/releases/latest)
[![Build](https://img.shields.io/github/actions/workflow/status/GrishMahat/discord-data-cli/release.yml)](https://github.com/GrishMahat/discord-data-cli/actions)

A terminal UI tool to analyze your Discord data export — built in Rust.

```
┌─ Discord Data Analyzer ────────────────────────────────────────┐
│  YourName#0000                           msgs  12,451          │
│  Ready                                   channels  183         │
├─ Home ──  Overview ──  Channels ──  Settings ──────────────────┤
│                                                                 │
│  ┌─ Menu ────────────────────┐  ┌─ Quick Stats ──────────────┐ │
│  │  1  Analyze Now           │  │  Messages      12,451      │ │
│  │  2  Overview              │  │  Channels         183      │ │
│  │  3  Download Attachments  │  │  With text      94.2%      │ │
│  │  4  Messages (All)        │  │  Avg length       42 ch    │ │
│  │  5  DMs                   │  │  Emoji          3,821      │ │
│  │  6  Group DMs             │  │  Servers           27      │ │
│  │  7  Public Threads        │  │                            │ │
│  │  8  Voice Channels        │  │  Peak 21:00    843 msgs    │ │
│  │  9  Settings              │  └────────────────────────────┘ │
│  │  10 Quit                  │                                 │
│  └───────────────────────────┘                                 │
└────────────────────────────────────────────────────────────────┘
```

## Features

- **Message stats** — total count, channels, emoji, attachments, average length, top words
- **Temporal analysis** — messages by hour-of-day, day-of-week, and month; earliest and latest dates
- **Channel browser** — filter by DMs, Group DMs, Public Threads, and Voice; read message previews
- **Overview dashboard** — server count, audit logs, support tickets, activity events
- **Attachment downloader** — fetch all media files from your message history
- **Session persistence** — resumes your last session automatically on next launch
- **Zero dependencies at runtime** — single static binary, no install required

## Download

Head to the [Releases](https://github.com/GrishMahat/discord-data-cli/releases/latest) page and grab the binary for your platform:

| Platform | File |
|---|---|
| Windows (x64) | `discord-analyzer-windows-x64.exe` |
| macOS (Apple Silicon) | `discord-analyzer-macos-arm64` |
| macOS (Intel) | `discord-analyzer-macos-intel` |
| Linux (x64) | `discord-analyzer-linux-x64` |

On macOS/Linux, mark the binary as executable before running:

```bash
chmod +x discord-analyzer-*
./discord-analyzer-macos-arm64
```

On macOS you may need to allow the binary in **System Settings > Privacy & Security**.

## Get Your Discord Data

1. Open Discord and go to **User Settings** (gear icon next to your username)
2. Scroll down to **Privacy & Safety**
3. Click **Request all of my Data**
4. Confirm the request
5. Wait 1–2 days — Discord will email you a download link
6. Download and extract the zip file

## Usage

Run the binary from your terminal:

```bash
./discord-analyzer
```

On first run, a setup wizard walks you through:

1. **Export path** — paste the path to your extracted Discord data folder
2. **Results directory** — where to save analysis output (defaults to inside the export folder)
3. **Profile ID** — optional, for managing multiple exports

Then select **Analyze Now** from the home menu. Analysis runs in the background with a live progress bar.

## Controls

| Key | Action |
|---|---|
| `w` / `s` or arrow keys | Move up / down |
| `Enter` | Select |
| `b` | Go back |
| `q` | Quit |
| `1`–`5` | Switch channel filters (All / DMs / Groups / Threads / Voice) |
| `u` / `d` | Page up / down in channel list |
| `←` / `→` | Adjust settings values |

## Build from Source

Requires [Rust](https://rustup.rs) stable.

```bash
git clone https://github.com/GrishMahat/discord-data-cli.git
cd discord-data-cli
cargo build --release
./target/release/discord-analyzer
```

## Why Rust?

I originally started this in Python but hadn't touched it in almost a year. Rust was what I was actively using, so I built it in Rust instead. It's not about performance — it just made sense to use the language I actually knew.

## License

GNU General Public License v3 — see [LICENSE](LICENSE)
