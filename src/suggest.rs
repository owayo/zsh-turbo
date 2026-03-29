use std::collections::HashSet;
use std::path::PathBuf;

/// Search strategy for suggestions
pub enum Strategy {
    Prefix,
    Substring,
    Fuzzy,
}

impl Strategy {
    pub fn from_str(s: &str) -> Self {
        match s {
            "substring" => Self::Substring,
            "fuzzy" => Self::Fuzzy,
            _ => Self::Prefix,
        }
    }
}

/// Get a single suggestion using the specified strategy.
/// Falls back through strategies: prefix → substring → fuzzy.
pub fn get_suggestion(
    query: &str,
    history_file: Option<&str>,
    strategy: &Strategy,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }

    let history_path = resolve_history_path(history_file)?;
    let content = std::fs::read(&history_path).ok()?;
    let text = String::from_utf8_lossy(&content);

    // Always try prefix first (fastest)
    for line in text.lines().rev() {
        let cmd = parse_history_line(line).trim();
        if !cmd.is_empty() && cmd.starts_with(query) && cmd != query {
            return Some(cmd.to_string());
        }
    }

    // If strategy allows, try substring
    if matches!(strategy, Strategy::Substring | Strategy::Fuzzy) {
        for line in text.lines().rev() {
            let cmd = parse_history_line(line).trim();
            if !cmd.is_empty() && cmd != query && cmd.contains(query) {
                return Some(cmd.to_string());
            }
        }
    }

    // If strategy allows, try fuzzy
    if matches!(strategy, Strategy::Fuzzy) {
        for line in text.lines().rev() {
            let cmd = parse_history_line(line).trim();
            if !cmd.is_empty() && cmd != query && fuzzy_match(query, cmd) {
                return Some(cmd.to_string());
            }
        }
    }

    None
}

pub fn get_completions(prefix: &str, history_file: Option<&str>, max: usize) -> Vec<String> {
    if prefix.is_empty() {
        return Vec::new();
    }

    let history_path = match resolve_history_path(history_file) {
        Some(p) => p,
        None => return Vec::new(),
    };

    let content = match std::fs::read(&history_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let text = String::from_utf8_lossy(&content);
    let mut seen = HashSet::new();
    let mut results = Vec::new();

    for line in text.lines().rev() {
        let cmd = parse_history_line(line).trim();
        if !cmd.is_empty()
            && cmd.starts_with(prefix)
            && cmd != prefix
            && seen.insert(cmd.to_string())
        {
            results.push(cmd.to_string());
            if results.len() >= max {
                break;
            }
        }
    }

    results
}

/// Fuzzy match: all characters in query appear in target in order.
fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for qc in query.chars() {
        let qc_lower = qc.to_ascii_lowercase();
        loop {
            match target_chars.next() {
                Some(tc) if tc.to_ascii_lowercase() == qc_lower => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

fn resolve_history_path(history_file: Option<&str>) -> Option<PathBuf> {
    history_file
        .map(PathBuf::from)
        .or_else(|| std::env::var("HISTFILE").ok().map(PathBuf::from))
        .or_else(|| dirs::home_dir().map(|h| h.join(".zsh_history")))
}

/// Parse zsh extended history format: ": 1234567890:0;actual command"
fn parse_history_line(line: &str) -> &str {
    if line.starts_with(": ")
        && let Some(idx) = line.find(';')
    {
        return &line[idx + 1..];
    }
    line
}
