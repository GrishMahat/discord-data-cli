use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_window_width")]
    pub window_width: f32,
    #[serde(default = "default_window_height")]
    pub window_height: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub package_directory: String,
    pub results_directory: String,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub source_aliases: SourceAliases,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceAliases {
    #[serde(default = "default_account_aliases")]
    pub account: Vec<String>,
    #[serde(default = "default_activity_aliases")]
    pub activity: Vec<String>,
    #[serde(default = "default_activities_aliases")]
    pub activities: Vec<String>,
    #[serde(default = "default_messages_aliases")]
    pub messages: Vec<String>,
    #[serde(default = "default_programs_aliases")]
    pub programs: Vec<String>,
    #[serde(default = "default_servers_aliases")]
    pub servers: Vec<String>,
    #[serde(default = "default_support_tickets_aliases")]
    pub support_tickets: Vec<String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            package_directory: "./package{ID}".to_owned(),
            results_directory: "./results{ID}".to_owned(),
            ui: UiConfig::default(),
            source_aliases: SourceAliases::default(),
        }
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
        }
    }
}

impl Default for SourceAliases {
    fn default() -> Self {
        Self {
            account: default_account_aliases(),
            activity: default_activity_aliases(),
            activities: default_activities_aliases(),
            messages: default_messages_aliases(),
            programs: default_programs_aliases(),
            servers: default_servers_aliases(),
            support_tickets: default_support_tickets_aliases(),
        }
    }
}

const fn default_window_width() -> f32 {
    1320.0
}

const fn default_window_height() -> f32 {
    860.0
}

fn default_account_aliases() -> Vec<String> {
    vec!["account".to_owned()]
}

fn default_activity_aliases() -> Vec<String> {
    vec!["activity".to_owned()]
}

fn default_activities_aliases() -> Vec<String> {
    vec!["activities".to_owned()]
}

fn default_messages_aliases() -> Vec<String> {
    vec!["messages".to_owned()]
}

fn default_programs_aliases() -> Vec<String> {
    vec!["programs".to_owned()]
}

fn default_servers_aliases() -> Vec<String> {
    vec!["servers".to_owned()]
}

fn default_support_tickets_aliases() -> Vec<String> {
    vec![
        "support_tickets".to_owned(),
        "support tickets".to_owned(),
        "support-tickets".to_owned(),
    ]
}

impl AppConfig {
    pub fn package_path(&self, config_path: &Path, id: &str) -> PathBuf {
        resolve_template_path(&self.package_directory, config_path, id)
    }

    pub fn results_path(&self, config_path: &Path, id: &str) -> PathBuf {
        resolve_template_path(&self.results_directory, config_path, id)
    }
}

fn resolve_template_path(raw: &str, config_path: &Path, id: &str) -> PathBuf {
    let replaced = raw.replace("{ID}", id);
    let path = PathBuf::from(replaced);
    if path.is_absolute() {
        return path;
    }
    match config_path.parent() {
        Some(parent) => parent.join(path),
        None => path,
    }
}
