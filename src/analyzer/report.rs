use super::structs::AnalysisData;
use anyhow::{Context, Result};
use std::{
    fs::{self, File},
    io::BufReader,
    path::Path,
};

pub fn read_data(results_dir: &Path) -> Result<Option<AnalysisData>> {
    let data_path = results_dir.join("data.json");
    if !data_path.exists() {
        return Ok(None);
    }
    let file = File::open(&data_path)
        .with_context(|| format!("failed to open {}", data_path.display()))?;
    let reader = BufReader::new(file);
    let parsed: AnalysisData =
        serde_json::from_reader(reader).with_context(|| "failed to parse data.json".to_owned())?;
    Ok(Some(parsed))
}

pub fn generate_markdown_report(stats: &AnalysisData) -> String {
    let mut out = String::new();
    out.push_str("# Discord Data Analysis Report\n\n");
    out.push_str(&format!("**Analyzed at:** {}\n", stats.meta.analyzed_at));
    if let Some(user) = &stats.account.username {
        out.push_str(&format!("**Account:** {}\n", user));
    }
    out.push_str("\n## Overview\n");
    out.push_str(&format!("- **Total Messages**: {}\n", stats.messages.total));
    out.push_str(&format!("- **Channels**: {}\n", stats.messages.channels));
    out.push_str(&format!(
        "- **Messages w/ Content**: {}\n",
        stats.messages.with_content
    ));
    out.push_str(&format!(
        "- **Messages w/ Attachments**: {}\n",
        stats.messages.with_attachments
    ));
    out.push_str(&format!(
        "- **Average Msg Length**: {:.1} chars\n",
        stats.messages.content.avg_length_chars
    ));

    out.push_str("\n## Top Channels\n");
    for (i, (ch, count)) in stats.messages.top_channels.iter().take(10).enumerate() {
        out.push_str(&format!("{}. **{}**: {} messages\n", i + 1, ch, count));
    }
    out.push_str("\n## Top Words\n");
    for (i, (word, count)) in stats.messages.content.top_words.iter().take(20).enumerate() {
        out.push_str(&format!("{}. **{}**: {} times\n", i + 1, word, count));
    }
    out.push_str("\n## Activity by Hour (UTC)\n");
    for (hour, count) in &stats.messages.temporal.by_hour {
        out.push_str(&format!("- **{:02}:00**: {} messages\n", hour, count));
    }
    out
}
