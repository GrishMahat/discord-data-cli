# Discord Data Analyzer

[![GitHub License](https://img.shields.io/github/license/GrishMahat/discord-data-cli)](LICENSE)

A Rust CLI tool to analyze your Discord data export.

## Why Rust?

I originally started this project in Python, but hadn't touched Python in almost a year. I didn't want to relearn everything just to build this tool, so I decided to write it in Rust instead. It's not about speed or performance—Rust was simply the language I was actively using at the time, and it made sense to stick with what I knew.

Python would have been better for data analysis, but I simply didn't remember enough Python to use it effectively.

## Get Your Data

1. Open Discord and go to **User Settings** (gear icon next to your username)
2. Scroll down to **Data and Privacy**
3. Scroll to the bottom and click **Request your Data**
4. Confirm the request
5. Wait 1-2 days (Discord will email you when ready)
6. Download the data zip from your email
7. Extract the zip file

## Download Pre-built Binaries

Head to the [Releases](https://github.com/GrishMahat/discord-data-cli/releases) page to download pre-built binaries for:
- Windows (x64)
- macOS (Apple Silicon & Intel)
- Linux (x64)

Just extract, run, and follow the prompts.

## Build from Source

```bash
cargo build --release
```

The binary will be at `target/release/discord-analyzer`.

## First Run

```bash
cargo run
# or ./target/release/discord-analyzer
```

On first run, you'll be prompted to:
- Enter the path to your extracted Discord data
- Choose where to save results

Then use arrow keys to navigate the TTY interface and explore your data.

## Controls

- `w` / `s` - move up/down
- `enter` - select
- `b` - back
- `q` - quit
- Channel list: `1..5` switch filters, `u/d` page up/down

## License

GNU General Public License v3 - see [LICENSE](LICENSE)
