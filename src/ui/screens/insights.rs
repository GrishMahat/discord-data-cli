use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::app::{fmt_num, AppState};

pub(crate) fn draw_insights(frame: &mut ratatui::Frame<'_>, app: &AppState, area: Rect) {
    let has_any = app.last_data.as_ref().is_some_and(|d| {
        d.insights.billing.payments_total > 0
            || d.insights.sessions.active_days > 0
            || d.insights.social.dm_channels > 0
    });

    if !has_any {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::styled(
                    "  No insights yet.",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::from(""),
                Line::styled(
                    "  Run 'Analyze Now' to compute billing, voice quality,",
                    Style::default().fg(Color::DarkGray),
                ),
                Line::styled(
                    "  device profile, sessions and contact analytics.",
                    Style::default().fg(Color::DarkGray),
                ),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Insights ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
        return;
    }

    let Some(data) = &app.last_data else {
        return;
    };
    let ins = &data.insights;

    // Header strip of headline numbers.
    let headline = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(4)])
        .split(area);

    let head = Line::from(vec![
        pill(&format!(" Payments {}", ins.billing.payments_total), Color::Green),
        Span::raw(" "),
        pill(
            &format!(" Active days {} ", ins.sessions.active_days),
            Color::Cyan,
        ),
        Span::raw(" "),
        pill(
            &format!(
                " Voice {}h ",
                (ins.voice.connected_minutes / 60.0).round() as u64
            ),
            Color::Magenta,
        ),
        Span::raw(" "),
        pill(
            &format!(" Streak {}d ", ins.sessions.longest_daily_streak),
            Color::Yellow,
        ),
    ]);
    frame.render_widget(Paragraph::new(head), headline[0]);

    let mut lines: Vec<Line> = Vec::new();

    // ---- Personal records
    section(&mut lines, "Records");
    kv(
        &mut lines,
        "First message",
        ins.records.first_message_date.as_deref().unwrap_or("n/a"),
    );
    kv(
        &mut lines,
        "Last message",
        ins.records.last_message_date.as_deref().unwrap_or("n/a"),
    );
    let longest = match ins.records.longest_message_date.as_deref() {
        Some(d) => format!("{} chars on {}", ins.records.longest_message_chars, d),
        None => format!("{} chars", ins.records.longest_message_chars),
    };
    kv(&mut lines, "Longest message", &longest);
    if let Some((name, count)) = &ins.records.biggest_channel {
        kv(
            &mut lines,
            "Biggest channel",
            &format!("{name} ({count} msgs)"),
        );
    }
    if let Some((day, count)) = &ins.sessions.busiest_day {
        kv(&mut lines, "Busiest day", &format!("{day} ({count} events)"));
    }
    kv(
        &mut lines,
        "Streak record",
        &format!("{} days active in a row", ins.sessions.longest_daily_streak),
    );

    // ---- Billing timeline
    section(&mut lines, "Billing");
    if ins.billing.payments_total == 0 {
        kv(&mut lines, "Payments", "none on record");
    } else {
        let totals: Vec<String> = ins
            .billing
            .totals_by_currency
            .iter()
            .map(|(c, v)| format!("{v} {c}"))
            .collect();
        kv(&mut lines, "Total spent", &totals.join(" + "));
        let gateways: Vec<String> = ins
            .billing
            .by_gateway
            .iter()
            .map(|(g, c)| format!("{} x{}", gateway_label(*g), c))
            .collect();
        kv(&mut lines, "Gateways", &gateways.join(", "));
        kv(
            &mut lines,
            "Entitlements",
            &fmt_num(ins.billing.entitlements_count),
        );
        blank(&mut lines);
        for p in ins.billing.timeline.iter().take(8) {
            let amount = if p.refunded > 0 {
                format!("{} {} (refunded {})", p.amount, p.currency, p.refunded)
            } else {
                format!("{} {}", p.amount, p.currency)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("   {} ", truncate(&p.date, 10)), dim()),
                Span::styled(format!("{:<28} ", truncate(&p.description, 28)), white()),
                Span::styled(amount, cyan()),
            ]));
        }
        if ins.billing.timeline.len() > 8 {
            note(&mut lines, &format!("   …and {} more", ins.billing.timeline.len() - 8));
        }
    }

    // ---- Voice / RTC quality
    section(&mut lines, "Voice / RTC Quality");
    if ins.voice.connections == 0 && ins.voice.disconnects == 0 {
        note(&mut lines, "   No voice activity recorded.");
    } else {
        kv(
            &mut lines,
            "Connections",
            &format!(
                "{} ok · {} disconnects · {} reconnects",
                ins.voice.connections, ins.voice.disconnects, ins.voice.reconnects
            ),
        );
        kv(
            &mut lines,
            "Quality",
            &format!(
                "avg ping {:.0} ms · avg MOS {:.2} · avg connect {:.0} ms",
                ins.voice.avg_ping_ms, ins.voice.avg_mos, ins.voice.avg_connect_ms
            ),
        );
        let loss = if ins.voice.packets_sent > 0 {
            format!(
                "{:.2}%",
                ins.voice.packets_lost as f64 * 100.0 / ins.voice.packets_sent as f64
            )
        } else {
            "n/a".to_owned()
        };
        kv(
            &mut lines,
            "Packet loss",
            &format!("{} lost of {} sent ({})", ins.voice.packets_lost, ins.voice.packets_sent, loss),
        );
        kv(
            &mut lines,
            "Time connected",
            &format!("{:.1} h", ins.voice.connected_minutes / 60.0),
        );
        let types: Vec<String> = ins
            .voice
            .minutes_by_connection_type
            .iter()
            .filter(|(_, m)| **m >= 0.1)
            .map(|(t, m)| format!("{t} {:.1}h", m / 60.0))
            .collect();
        if !types.is_empty() {
            kv(&mut lines, "Networks", &types.join(", "));
        }
        let reasons: Vec<String> = ins
            .voice
            .disconnect_reasons
            .iter()
            .map(|(r, c)| format!("{r} x{c}"))
            .collect();
        if !reasons.is_empty() {
            kv(&mut lines, "Disconnects", &reasons.join(", "));
        }
    }

    // ---- Device / client profile
    section(&mut lines, "Devices & Clients");
    ranked_line(&mut lines, "OS", &ins.devices.by_os);
    ranked_line(&mut lines, "Clients", &ins.devices.by_client);
    ranked_line(&mut lines, "Channel", &ins.devices.by_release_channel);
    ranked_line(&mut lines, "ISP", &ins.devices.by_isp);

    // ---- Sessions
    section(&mut lines, "Sessions");
    kv(
        &mut lines,
        "Active days",
        &format!("{}", ins.sessions.active_days),
    );
    kv(
        &mut lines,
        "Range",
        &format!(
            "{} -> {}",
            ins.sessions.first_active_day.as_deref().unwrap_or("n/a"),
            ins.sessions.last_active_day.as_deref().unwrap_or("n/a")
        ),
    );
    kv(
        &mut lines,
        "Longest streak",
        &format!("{} days", ins.sessions.longest_daily_streak),
    );

    // ---- Contacts
    section(&mut lines, "Contacts");
    kv(
        &mut lines,
        "DMs",
        &format!(
            "{} channels · {} messages",
            ins.social.dm_channels, ins.social.dm_messages
        ),
    );
    for (i, contact) in ins.social.top_contacts.iter().enumerate().take(6) {
        let dates = match (&contact.first_date, &contact.last_date) {
            (Some(f), Some(l)) if f != l => format!("{f} → {l}"),
            (Some(f), _) => f.clone(),
            _ => "n/a".to_owned(),
        };
        lines.push(Line::from(vec![
            Span::styled(format!("   {}. ", i + 1), dim()),
            Span::styled(
                format!("{:<26} ", truncate(&contact.name, 26)),
                white(),
            ),
            Span::styled(format!("{:>6}  ", fmt_num(contact.messages)), cyan()),
            Span::styled(dates, dim()),
        ]));
    }
    if ins.social.group_dm_channels > 0 {
        kv(
            &mut lines,
            "Group DMs",
            &format!(
                "{} channels · {} messages",
                ins.social.group_dm_channels, ins.social.group_dm_messages
            ),
        );
    }

    // ---- Server engagement
    if !ins.servers_engaged.is_empty() {
        section(&mut lines, "Top Servers");
        for (i, server) in ins.servers_engaged.iter().enumerate().take(8) {
            lines.push(Line::from(vec![
                Span::styled(format!("   {}. ", i + 1), dim()),
                Span::styled(
                    format!("{:<30} ", truncate(&server.name, 30)),
                    white(),
                ),
                Span::styled(fmt_num(server.events), cyan()),
                Span::styled(
                    format!(" events · {} audit", server.audit_log_entries),
                    dim(),
                ),
            ]));
        }
    }

    // ---- Link intelligence
    section(&mut lines, "Link Intelligence");
    if data.messages.total > 0 {
        let pct = ins.links.messages_with_links as f64 * 100.0 / data.messages.total as f64;
        kv(
            &mut lines,
            "Links shared",
            &format!(
                "{} in {} msgs ({:.2}% of all messages)",
                ins.links.total_links, ins.links.messages_with_links, pct
            ),
        );
        let qpct = ins.links.question_messages as f64 * 100.0 / data.messages.total as f64;
        kv(
            &mut lines,
            "Questions asked",
            &format!("{} msgs ({:.2}%)", ins.links.question_messages, qpct),
        );
        let domains: Vec<String> = ins
            .links
            .top_domains
            .iter()
            .take(8)
            .map(|(d, c)| format!("{d} x{c}"))
            .collect();
        kv(&mut lines, "Top domains", &domains.join(", "));
    }

    // ---- Privacy posture
    section(&mut lines, "Privacy Posture");
    kv(&mut lines, "Email verified", yes_no(ins.privacy.email_verified));
    kv(&mut lines, "MFA enabled", yes_no(ins.privacy.mfa_enabled));
    kv(&mut lines, "Phone attached", yes_no(ins.privacy.has_phone));
    kv(
        &mut lines,
        "Payment methods",
        &if ins.privacy.has_payment_source {
            format!("{} on file", ins.privacy.payment_sources)
        } else {
            "none".to_owned()
        },
    );
    kv(
        &mut lines,
        "Data requests",
        &format!("{} filed", ins.privacy.data_access_requests),
    );
    if !ins.privacy.account_flags.is_empty() {
        note(
            &mut lines,
            &format!("   Flags: {}", ins.privacy.account_flags.join(", ")),
        );
    }

    let total_lines = lines.len();
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(
                        " Insights [↑↓ Scroll, line {}/{}] ",
                        (app.insights_scroll + 1).min(total_lines.max(1)),
                        total_lines
                    ))
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((app.insights_scroll as u16, 0)),
        headline[1],
    );
}

fn pill(text: &str, color: Color) -> Span<'static> {
    Span::styled(
        text.to_owned(),
        Style::default()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )
}

fn section(lines: &mut Vec<Line<'static>>, title: &str) {
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        title.to_owned(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )));
}

fn kv(lines: &mut Vec<Line<'static>>, key: &str, value: &str) {
    lines.push(Line::from(vec![
        Span::styled(format!("   {key:<16}"), dim()),
        Span::styled(value.to_owned(), white()),
    ]));
}

fn note(lines: &mut Vec<Line<'static>>, text: &str) {
    lines.push(Line::from(Span::styled(text.to_owned(), dim())));
}

fn blank(lines: &mut Vec<Line<'static>>) {
    lines.push(Line::from(""));
}

fn ranked_line(lines: &mut Vec<Line<'static>>, label: &str, map: &std::collections::BTreeMap<String, u64>) {
    if map.is_empty() {
        return;
    }
    let items: Vec<String> = map
        .iter()
        .take(4)
        .map(|(k, c)| format!("{k} x{c}"))
        .collect();
    kv(lines, label, &items.join(", "));
}

fn yes_no(v: bool) -> &'static str {
    if v { "yes" } else { "no" }
}

fn gateway_label(code: u32) -> &'static str {
    match code {
        1 => "Stripe",
        2 => "Braintree",
        3 => "Apple",
        4 => "Google",
        5 => "Adyen",
        7 => "Amazon",
        8 => "Orbs",
        _ => "other",
    }
}

fn dim() -> Style {
    Style::default().fg(Color::DarkGray)
}
fn white() -> Style {
    Style::default().fg(Color::White)
}
fn cyan() -> Style {
    Style::default().fg(Color::Cyan)
}

fn truncate(s: &str, n: usize) -> String {
    crate::data::utils::truncate_text(s, n)
}
