use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub prompt: PromptConfig,
    #[serde(default)]
    pub suggest: SuggestConfig,
    #[serde(default)]
    pub style: StyleConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptConfig {
    #[serde(default = "default_prompt_style")]
    pub prompt_style: String,
    #[serde(default = "default_font_level")]
    pub font_level: String,
    #[serde(default = "default_left_segments")]
    pub left_segments: Vec<String>,
    #[serde(default = "default_right_segments")]
    pub right_segments: Vec<String>,
    #[serde(default)]
    pub dir: DirConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub transient: bool,
    #[serde(default)]
    pub custom: Vec<CustomSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSegment {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_custom_fg")]
    pub fg: String,
    #[serde(default = "default_custom_bg")]
    pub bg: String,
    #[serde(default = "default_when_always")]
    pub when: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirConfig {
    #[serde(default = "default_truncation_length")]
    pub truncation_length: usize,
    #[serde(default = "default_truncation_symbol")]
    pub truncation_symbol: String,
    #[serde(default = "default_home_symbol")]
    pub home_symbol: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub show_ahead_behind: bool,
    #[serde(default = "default_true")]
    pub show_stash: bool,
    #[serde(default = "default_true")]
    pub show_status: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_highlight_color")]
    pub highlight_color: String,
    #[serde(default = "default_max_suggestions")]
    pub max_suggestions: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StyleConfig {
    #[serde(default = "default_primary_color")]
    pub primary_color: String,
    #[serde(default = "default_success_color")]
    pub success_color: String,
    #[serde(default = "default_error_color")]
    pub error_color: String,
    #[serde(default = "default_muted_color")]
    pub muted_color: String,
}

fn default_prompt_style() -> String {
    "lean".into()
}
fn default_font_level() -> String {
    "unicode".into()
}
fn default_left_segments() -> Vec<String> {
    vec!["dir".into(), "git".into()]
}
fn default_right_segments() -> Vec<String> {
    vec!["duration".into(), "status".into(), "time".into()]
}
fn default_truncation_length() -> usize {
    3
}
fn default_truncation_symbol() -> String {
    "\u{2026}".into()
}
fn default_home_symbol() -> String {
    "~".into()
}
fn default_true() -> bool {
    true
}
fn default_custom_fg() -> String {
    "white".into()
}
fn default_custom_bg() -> String {
    "238".into()
}
fn default_when_always() -> String {
    "always".into()
}
fn default_strategy() -> String {
    "history".into()
}
fn default_highlight_color() -> String {
    "bright_black".into()
}
fn default_max_suggestions() -> usize {
    1
}
fn default_primary_color() -> String {
    "blue".into()
}
fn default_success_color() -> String {
    "green".into()
}
fn default_error_color() -> String {
    "red".into()
}
fn default_muted_color() -> String {
    "bright_black".into()
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            prompt_style: default_prompt_style(),
            font_level: default_font_level(),
            left_segments: default_left_segments(),
            right_segments: default_right_segments(),
            dir: DirConfig::default(),
            git: GitConfig::default(),
            transient: false,
            custom: Vec::new(),
        }
    }
}

impl Default for DirConfig {
    fn default() -> Self {
        Self {
            truncation_length: default_truncation_length(),
            truncation_symbol: default_truncation_symbol(),
            home_symbol: default_home_symbol(),
        }
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            show_ahead_behind: true,
            show_stash: true,
            show_status: true,
        }
    }
}

impl Default for SuggestConfig {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            highlight_color: default_highlight_color(),
            max_suggestions: default_max_suggestions(),
        }
    }
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            primary_color: default_primary_color(),
            success_color: default_success_color(),
            error_color: default_error_color(),
            muted_color: default_muted_color(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    // Prefer ~/.config/zsh-turbo for CLI tool convention
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
        });
    base.join("zsh-turbo")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load_config() -> Config {
    let path = config_path();
    if path.exists() {
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("zsh-turbo: warning: failed to read {}: {e}", path.display());
                return Config::default();
            }
        };
        match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "zsh-turbo: warning: failed to parse {}: {e}",
                    path.display()
                );
                Config::default()
            }
        }
    } else {
        Config::default()
    }
}

pub fn save_config(config: &Config) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let content = toml::to_string_pretty(config).unwrap_or_default();
    std::fs::write(config_path(), content)
}
