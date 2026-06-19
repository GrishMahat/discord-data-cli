use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::{
    analyzer,
    app::{AppState, fmt_num},
    ui::components::stat_line,
};

pub(crate) fn draw_overview(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let Some(data) = &app.last_data else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No analysis data loaded. Run Analyze Now first.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Overview ")
                    .border_style(Style::default().fg(Color::Cyan)),
            ),
            area,
        );
        return;
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(42),
            Constraint::Percentage(42),
            Constraint::Percentage(16),
        ])
        .split(area);

    let top_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(rows[0]);

    let mid_row = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[1]);

    draw_message_activity(frame, data, top_row[0]);
    draw_channel_types(frame, data, top_row[1]);
    draw_top_words(frame, data, mid_row[0]);
    draw_time_distribution(frame, data, mid_row[1]);
    draw_data_overview(frame, data, rows[2]);
}

fn draw_message_activity(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Message Activity ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let by_month: Vec<(u32, u64)> = data
        .messages
        .temporal
        .by_month
        .iter()
        .map(|(&k, &v)| (k, v))
        .collect();

    if !by_month.is_empty() && inner.width >= 12 {
        let max_val = by_month
            .iter()
            .map(|(_, v)| v)
            .copied()
            .max()
            .unwrap_or(1)
            .max(1);
        let total_bars = (inner.width.saturating_sub(2)).min(12) as usize;
        let values_per_bar = (by_month.len() / total_bars.max(1)).max(1);

        let mut spark_spans = Vec::new();
        let mut label_spans = Vec::new();
        let month_labels = [
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let label_step = (total_bars / 6).max(1);

        for bar_i in 0..total_bars {
            let seg_start = bar_i * values_per_bar;
            let seg_end = (seg_start + values_per_bar).min(by_month.len());
            let seg_max = by_month[seg_start..seg_end]
                .iter()
                .map(|(_, v)| v)
                .copied()
                .max()
                .unwrap_or(0);
            let frac = seg_max as f64 / max_val as f64;
            let spark_idx = if seg_max == 0 {
                0usize
            } else {
                ((frac * 7.0).round() as usize).min(7) + 1
            };
            let spark_chars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
            spark_spans.push(ratatui::text::Span::styled(
                spark_chars[spark_idx].to_string(),
                Style::default().fg(Color::Cyan),
            ));
            if bar_i % label_step == 0 || bar_i == 0 {
                let mid_idx = seg_start + (seg_end - seg_start) / 2;
                let mn = by_month[mid_idx.min(by_month.len() - 1)].0 as usize;
                label_spans.push(ratatui::text::Span::styled(
                    if mn <= 12 { month_labels[mn] } else { "" },
                    Style::default().fg(Color::DarkGray),
                ));
            }
        }
        lines.push(Line::from(spark_spans));
        lines.push(Line::from(label_spans));
    }

    lines.push(Line::from(""));
    lines.push(stat_line("Total", &fmt_num(data.messages.total)));
    lines.push(stat_line("Channels", &fmt_num(data.messages.channels)));
    lines.push(stat_line(
        "With text",
        &format!(
            "{} ({:.1}%)",
            fmt_num(data.messages.with_content),
            if data.messages.total > 0 {
                (data.messages.with_content as f64 / data.messages.total as f64) * 100.0
            } else {
                0.0
            }
        ),
    ));
    if let Some((peak_month, peak_count)) = by_month.iter().max_by_key(|(_, c)| *c) {
        let month_labels = [
            "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let lbl = month_labels.get(*peak_month as usize).unwrap_or(&"");
        lines.push(Line::styled(
            format!(" Peak month: {} ({} msgs)", lbl, fmt_num(*peak_count)),
            Style::default().fg(Color::DarkGray),
        ));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_channel_types(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Channel Types ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if data.messages.by_channel_type.is_empty() {
        return;
    }

    let type_display: Vec<(&str, u64)> = data
        .messages
        .by_channel_type
        .iter()
        .map(|(k, v)| {
            let label = match k.as_str() {
                "GUILD_TEXT" | "GUILD_ANNOUNCEMENT" | "GUILD_FORUM" | "GUILD_MEDIA" => "Guild",
                "DM" => "DM",
                "GROUP_DM" => "Group DM",
                "GUILD_PUBLIC_THREAD" | "GUILD_NEWS_THREAD" | "GUILD_PRIVATE_THREAD" => "Thread",
                "GUILD_VOICE" | "GUILD_STAGE_VOICE" => "Voice",
                _ => k.as_str(),
            };
            (label, *v)
        })
        .collect();

    let max_count = type_display
        .iter()
        .map(|(_, c)| c)
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    let bar_max = inner.width.saturating_sub(22).max(4);

    let mut lines = Vec::new();
    for (label, count) in &type_display {
        let bar_len = ((*count as f64 / max_count as f64) * bar_max as f64).round() as usize;
        let bar = "█".repeat(bar_len);
        lines.push(Line::from(vec![
            ratatui::text::Span::styled(
                format!(" {}", bar),
                Style::default().fg(Color::Cyan),
            ),
            ratatui::text::Span::styled(
                format!("  {:<9}", label),
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                fmt_num(*count),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_top_words(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Top Words ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let top_words = &data.messages.content.top_words;
    if top_words.is_empty() {
        return;
    }

    let max_count = top_words[0].1.max(1);
    let bar_max = inner.width.saturating_sub(22).max(4);
    let show = (inner.height.saturating_sub(1)).min(15) as usize;

    let mut lines = Vec::new();
    for (word, count) in top_words.iter().take(show) {
        let bar_len = ((*count as f64 / max_count as f64) * bar_max as f64).round() as usize;
        let bar = "█".repeat(bar_len);
        let short: String = word.chars().take(12).collect();
        lines.push(Line::from(vec![
            ratatui::text::Span::styled(
                format!(" {}", bar),
                Style::default().fg(Color::Cyan),
            ),
            ratatui::text::Span::styled(
                format!("  {:<12}", short),
                Style::default().fg(Color::White),
            ),
            ratatui::text::Span::styled(
                fmt_num(*count),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_time_distribution(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Time Distribution ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if data.messages.temporal.by_hour.is_empty() || inner.height < 3 || inner.width < 16 {
        return;
    }

    let max_val = data
        .messages
        .temporal
        .by_hour
        .values()
        .copied()
        .max()
        .unwrap_or(1)
        .max(1);
    let spark_chars = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    let mut lines = Vec::new();
    for block_start in (0u32..24).step_by(6) {
        let mut spark = String::new();
        for hour in block_start..(block_start + 6).min(24) {
            let count = data
                .messages
                .temporal
                .by_hour
                .get(&hour)
                .copied()
                .unwrap_or(0);
            let idx = if count == 0 {
                0
            } else {
                let frac = count as f64 / max_val as f64;
                ((frac * 7.0).round() as usize).min(7) + 1
            };
            spark.push(spark_chars[idx]);
        }
        lines.push(Line::from(vec![
            ratatui::text::Span::styled(
                format!(" {:02}:00  ", block_start),
                Style::default().fg(Color::DarkGray),
            ),
            ratatui::text::Span::styled(spark, Style::default().fg(Color::Cyan)),
        ]));
    }

    if let Some((peak_hour, peak_count)) = data
        .messages
        .temporal
        .by_hour
        .iter()
        .max_by_key(|(_, c)| *c)
    {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            format!(" Peak: {:02}:00 ({} msgs)", peak_hour, fmt_num(*peak_count)),
            Style::default().fg(Color::DarkGray),
        ));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_data_overview(frame: &mut ratatui::Frame<'_>, data: &analyzer::AnalysisData, area: Rect) {
    let sections = [
        ("Messages", data.folder_presence.get("messages").copied()),
        ("Servers", data.folder_presence.get("servers").copied()),
        ("Activity", data.folder_presence.get("activity").copied()),
        ("Support", data.folder_presence.get("support_tickets").copied()),
        ("Programs", data.folder_presence.get("programs").copied()),
        ("Account", data.folder_presence.get("account").copied()),
    ];

    let all_ok = sections.iter().all(|(_, ok)| ok.unwrap_or(false));

    let mut spans = Vec::new();
    for (name, ok) in &sections {
        let ok = ok.unwrap_or(false);
        let (check, color) = if ok {
            ("✓", Color::Green)
        } else {
            ("✗", Color::Red)
        };
        spans.push(ratatui::text::Span::styled(
            format!(" {} {} ", check, name),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Data Overview ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let status = if all_ok {
        "All data folders processed successfully."
    } else {
        "Some data folders were missing from this export."
    };

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(spans),
            Line::from(""),
            Line::styled(
                format!(" {}", status),
                Style::default().fg(if all_ok { Color::Green } else { Color::Yellow }),
            ),
        ]),
        inner,
    );
}
