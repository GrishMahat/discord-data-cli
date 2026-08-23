use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

use crate::app::{AppState, fmt_num};

const YEARS_PER_PAGE: usize = 6; // 3 columns x 2 rows
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// One cell = one day, one character wide so week columns stay aligned.
fn level_span(level: u32) -> Span<'static> {
    match level {
        0 => Span::styled("·", Style::default().fg(Color::DarkGray)),
        1 => Span::styled(
            "▪",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::DIM),
        ),
        2 => Span::styled("▪", Style::default().fg(Color::Green)),
        3 => Span::styled("▪", Style::default().fg(Color::LightGreen)),
        _ => Span::styled(
            "█",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
    }
}

/// Parse "YYYY-MM-DD" -> ordinal days since epoch + (y, m, d).
fn parse_day(s: &str) -> Option<(i64, i32, u32, u32)> {
    let mut it = s.split('-');
    let y: i32 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    Some((discord_data_cli_core::insights::day_ordinal(s)?, y, m, d))
}

/// Weekday index Monday=0..Sunday=6 for a date key "YYYY-MM-DD".
/// 1970-01-01 was a Thursday (=3), so offset ordinals by 3.
fn weekday_of(key: &str) -> u32 {
    let ord = discord_data_cli_core::insights::day_ordinal(key).unwrap_or(0);
    (((ord + 3) % 7 + 7) % 7) as u32
}

struct YearGrid {
    year: i32,
    /// One entry per ISO-ish week column (Mon-first), 7 weekday rows.
    weeks: Vec<[u64; 7]>,
    total: u64,
    active_days: usize,
}

/// Build the full-resolution weekly grid for one calendar year.
fn build_year_grid(by_day: &std::collections::BTreeMap<String, u64>, year: i32) -> YearGrid {
    let jan1 = format!("{year}-01-01");
    let dec31 = format!("{year}-12-31");
    let Some((start_ord, _, _, _)) = parse_day(&jan1) else {
        return YearGrid {
            year,
            weeks: Vec::new(),
            total: 0,
            active_days: 0,
        };
    };
    let end_ord = parse_day(&dec31).map(|(o, ..)| o).unwrap_or(start_ord);

    let lead = weekday_of(&jan1);
    let total_weeks = (((lead as i64 + end_ord - start_ord + 1) + 6) / 7).max(1) as usize;
    let mut weeks: Vec<[u64; 7]> = vec![[0; 7]; total_weeks];
    let mut last_known = start_ord.saturating_sub(1);

    for (key, count) in by_day
        .iter()
        .filter(|(d, _)| d.starts_with(&year.to_string()))
    {
        let Some((ord, ..)) = parse_day(key) else {
            continue;
        };
        let pos = lead as i64 + ord - start_ord;
        if pos < 0 {
            continue;
        }
        let col = (pos / 7) as usize;
        let row = weekday_of(key) as usize;
        if col < weeks.len() {
            weeks[col][row] = *count;
        }
        last_known = last_known.max(ord);
    }

    // A trailing partial future (or empty tail) of the latest year is trimmed
    // so the grid ends at the last known day.
    if last_known < end_ord {
        let keep = ((lead as i64 + last_known - start_ord + 1 + 6) / 7).max(1) as usize;
        weeks.truncate(keep);
    }

    let total: u64 = weeks.iter().flatten().sum();
    let active_days = weeks.iter().flatten().filter(|v| **v > 0).count();
    YearGrid {
        year,
        weeks,
        total,
        active_days,
    }
}

/// Quantile-based 5-level color mapping over all displayed values.
struct Levels {
    thresholds: [u64; 3],
}
impl Levels {
    fn new(values: &mut Vec<u64>) -> Self {
        values.retain(|v| *v > 0);
        values.sort_unstable();
        let q = |p: f32| -> u64 {
            if values.is_empty() {
                return 0;
            }
            values[(p * (values.len() - 1) as f32).round() as usize]
        };
        Self {
            thresholds: [q(0.4), q(0.7), q(0.9)],
        }
    }
    fn level(&self, v: u64) -> u32 {
        if v == 0 {
            return 0;
        }
        if v > self.thresholds[2] {
            4
        } else if v > self.thresholds[1] {
            3
        } else if v > self.thresholds[0] {
            2
        } else {
            1
        }
    }
}

pub(crate) fn draw_activity_map(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(data) = &app.last_data else {
        return;
    };
    let by_day = &data.messages.temporal.by_day;

    let mut years: Vec<i32> = by_day
        .keys()
        .filter_map(|d| d[..4].parse::<i32>().ok())
        .collect();
    years.sort_unstable();
    years.dedup();

    if years.is_empty() {
        frame.render_widget(
            Paragraph::new(" No daily message data — run Analyze Now first.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Activity Map ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    }

    let total_pages = years.len().div_ceil(YEARS_PER_PAGE);
    let page = app.map_page.min(total_pages - 1);
    let page_start = page * YEARS_PER_PAGE;
    let shown: Vec<YearGrid> = years[page_start..]
        .iter()
        .take(YEARS_PER_PAGE)
        .map(|y| build_year_grid(by_day, *y))
        .collect();

    // Consistent color scale across every minimap on the page (per-day counts).
    let mut per_day: Vec<u64> = shown
        .iter()
        .flat_map(|g| g.weeks.iter())
        .flat_map(|w| w.iter().copied())
        .collect();
    let levels = Levels::new(&mut per_day);

    // ---- Layout: header strip, two grid rows, footer legend.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // header
            Constraint::Length(11), // grid row 1
            Constraint::Length(11), // grid row 2
            Constraint::Min(2),     // legend / totals
        ])
        .split(area);

    let header_line = Line::from(vec![
        Span::styled(
            " Activity Map ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " {}–{} of {} years",
                shown.first().map(|g| g.year).unwrap_or(0),
                shown.last().map(|g| g.year).unwrap_or(0),
                years.len()
            ),
            dim(),
        ),
    ]);
    frame.render_widget(Paragraph::new(header_line), rows[0]);

    let grid_area = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(11), Constraint::Length(11)])
        .split(rows[1]);
    let cols_top = split_three(grid_area[0]);
    let cols_bottom = split_three(grid_area[1]);

    for (slot, area) in cols_top.iter().chain(cols_bottom.iter()).enumerate() {
        match shown.get(slot) {
            Some(grid) => draw_mini_year(frame, grid, &levels, *area),
            None => {
                frame.render_widget(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::DarkGray)),
                    *area,
                );
            }
        }
    }

    // ---- Footer: legend + totals + paging hint.
    let total_all: u64 = by_day.values().sum();
    let pages_hint = if total_pages > 1 {
        format!("  [← → Page {}/{}]", page + 1, total_pages)
    } else {
        String::new()
    };
    let legend = vec![
        Line::from(vec![
            Span::styled(" less ", dim()),
            level_span(0),
            level_span(1),
            level_span(2),
            level_span(3),
            level_span(4),
            Span::styled(" more   ", dim()),
            Span::styled(
                format!(
                    "All-time {} msgs · {} active days · streak {} days",
                    fmt_num(total_all),
                    data.insights.sessions.active_days,
                    data.insights.sessions.longest_daily_streak
                ),
                white(),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                " Showing {}–{} of {} years{}",
                shown.first().map(|g| g.year).unwrap_or(0),
                shown.last().map(|g| g.year).unwrap_or(0),
                years.len(),
                pages_hint
            ),
            dim(),
        )),
    ];
    frame.render_widget(
        Paragraph::new(legend).block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        rows[2],
    );
}

fn split_three(area: Rect) -> Vec<Rect> {
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area)
        .iter()
        .copied()
        .collect()
}

fn draw_mini_year(frame: &mut ratatui::Frame<'_>, grid: &YearGrid, levels: &Levels, area: Rect) {
    let title = format!(
        " {} · {} msgs · {} days ",
        grid.year,
        fmt_num(grid.total),
        grid.active_days
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let avail_cols = inner.width as usize;
    let full_weeks = grid.weeks.len();
    // Bucket multiple weeks per column when the panel is too narrow.
    let factor = full_weeks.div_ceil(avail_cols).max(1);
    let cols = full_weeks.div_ceil(factor);

    const DAYS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
    let show_day_names = avail_cols >= 14;

    let mut lines: Vec<Line> = Vec::new();
    // Month headers only fit on wide panels with full-resolution columns.
    if avail_cols >= 54 && full_weeks <= avail_cols {
        let jan1 = format!("{}-01-01", grid.year);
        let start_ord = discord_data_cli_core::insights::day_ordinal(&jan1).unwrap_or(0);
        let lead = weekday_of(&jan1) as i64;
        let mut cells: Vec<Option<u32>> = vec![None; full_weeks];
        for m in 1..=12u32 {
            let key = format!("{:04}-{:02}-01", grid.year, m);
            let Some(ord) = discord_data_cli_core::insights::day_ordinal(&key) else {
                continue;
            };
            let pos = lead + ord - start_ord;
            if pos < 0 {
                continue;
            }
            let col = (pos as usize).min(full_weeks - 1);
            if cells[col].is_none() {
                cells[col] = Some(m - 1);
            }
        }
        let mut header: Vec<Span> = Vec::new();
        for cell in cells {
            match cell {
                Some(m) => header.push(Span::styled(MONTHS[m as usize], dim())),
                None => header.push(Span::styled("   ", dim())),
            }
        }
        lines.push(Line::from(header));
    }

    for row in 0..7 {
        let mut spans: Vec<Span> = Vec::new();
        if show_day_names {
            spans.push(Span::styled(DAYS[row].to_string(), dim()));
        }
        for col in 0..cols {
            let mut bucket_max = 0u64;
            for k in 0..factor {
                if let Some(w) = grid.weeks.get(col * factor + k) {
                    bucket_max = bucket_max.max(w[row]);
                }
            }
            spans.push(level_span(levels.level(bucket_max)));
        }
        lines.push(Line::from(spans));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}
fn white() -> Style {
    Style::default().fg(Color::White)
}

#[cfg(test)]
mod tests {
    use super::*;
    use discord_data_cli_core::analyzer::AnalysisData;

    #[test]
    fn multi_year_wall_renders_all_panels() {
        let mut data = AnalysisData::default();
        for day in [
            "2021-03-07",
            "2022-06-14",
            "2023-11-02",
            "2024-01-05",
            "2025-09-19",
        ] {
            data.messages.temporal.by_day.insert(day.to_owned(), 42);
        }
        data.insights.sessions.active_days = 5;
        let app = crate::app::test_app_with_data(data);

        let backend = ratatui::backend::TestBackend::new(120, 34);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_activity_map(frame, &app, area);
            })
            .unwrap();

        let buffer = format!("{:?}", terminal.backend().buffer());
        assert!(buffer.contains("Activity Map"), "screen title missing");
        // All five years render as panel titles on a single page.
        for year in ["2021", "2022", "2023", "2024", "2025"] {
            assert!(buffer.contains(year), "year panel {year} missing");
        }
        assert!(buffer.contains("All-time"), "totals line missing");
        assert!(buffer.contains("▪"), "no grid cells rendered");
    }

    #[test]
    fn more_than_six_years_paginates() {
        let mut data = AnalysisData::default();
        for y in 2018..=2026 {
            data.messages
                .temporal
                .by_day
                .insert(format!("{y}-06-15"), 7);
        }
        let app = crate::app::test_app_with_data(data);

        let backend = ratatui::backend::TestBackend::new(120, 34);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_activity_map(frame, &app, area);
            })
            .unwrap();

        let buffer = format!("{:?}", terminal.backend().buffer());
        assert!(buffer.contains("Page 1/2"), "page hint missing: {buffer}");
        assert!(buffer.contains("2018"), "first page should start at 2018");
    }
}
