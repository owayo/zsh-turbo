use crate::shell_single_quote;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod cache;
mod help;
mod subcommands;

pub use subcommands::{TopLevel, top_level};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    #[default]
    Help,
    Generator,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Registration {
    pub command: String,
    pub enabled: bool,
    pub source: Source,
    pub generator: Vec<String>,
    pub file: String,
    pub watch_files: Vec<String>,
}

impl Default for Registration {
    fn default() -> Self {
        Self {
            command: String::new(),
            enabled: true,
            source: Source::Help,
            generator: Vec::new(),
            file: String::new(),
            watch_files: Vec::new(),
        }
    }
}

pub fn valid_command(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-+.".contains(&c))
        && name != "zsh-turbo"
}

pub fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".cache")
        });
    base.join("zsh-turbo/managed-completions")
}

pub fn refresh_all(entries: &[Registration], force: bool) -> Vec<String> {
    entries
        .iter()
        .filter(|e| e.enabled)
        .map(|entry| {
            let result = cache::refresh(entry, &cache_dir(), force);
            match result {
                Ok(state) => format!("{}: {state}", entry.command),
                Err(error) => format!("{}: {error}", entry.command),
            }
        })
        .collect()
}

pub fn status(entry: &Registration) -> String {
    cache::status(entry, &cache_dir())
}

pub fn shell_init(entries: &[Registration]) -> String {
    let entries: Vec<_> = entries
        .iter()
        .filter(|e| e.enabled && valid_command(&e.command))
        .collect();
    if entries.is_empty() {
        return String::new();
    }
    let mut script = format!(
        "\ntypeset -g _ZSH_TURBO_COMPLETION_CACHE={}\ntypeset -ga _ZSH_TURBO_COMPLETION_COMMANDS=(",
        shell_single_quote(&cache_dir().to_string_lossy())
    );
    for entry in entries {
        script.push_str(&format!("{} ", shell_single_quote(&entry.command)));
    }
    script.push_str(")\n");
    script.push_str(include_str!("../shell/completion.zsh"));
    script
}

pub fn expand_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        return dirs::home_dir().unwrap_or_default().join(rest);
    }
    PathBuf::from(path)
}

pub fn validate(entries: &[Registration]) -> Result<(), &'static str> {
    let mut names = std::collections::HashSet::new();
    for entry in entries {
        if !valid_command(&entry.command) || !names.insert(&entry.command) {
            return Err("Use a unique command name (letters, digits, -, _, +, .).");
        }
        if entry.source == Source::Generator && entry.generator.first().is_none_or(|s| s.is_empty())
        {
            return Err(
                "Generator must be a JSON array containing an executable and its arguments.",
            );
        }
        if entry.source == Source::File && entry.file.trim().is_empty() {
            return Err("Choose a completion file.");
        }
    }
    Ok(())
}

pub fn refresh_saved_async() {
    if let Ok(executable) = std::env::current_exe()
        && let Ok(mut child) = std::process::Command::new(executable)
            .arg("completion-refresh")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
    {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

#[cfg(test)]
mod tests;
