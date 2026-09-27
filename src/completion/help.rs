use crate::shell_single_quote as quote;
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub(super) struct Help {
    options: Vec<OptionSpec>,
    commands: Vec<(String, String)>,
}

#[derive(Debug)]
struct OptionSpec {
    flags: Vec<String>,
    value: Option<String>,
    optional: bool,
    description: String,
}

fn clean(text: &str) -> String {
    let mut escaped = false;
    text.chars()
        .filter(|c| {
            if *c == '\x1b' {
                escaped = true;
                return false;
            }
            if escaped {
                if c.is_ascii_alphabetic() {
                    escaped = false;
                }
                return false;
            }
            !c.is_control() || *c == '\n' || *c == '\t'
        })
        .collect()
}

fn split_columns(line: &str) -> (&str, &str) {
    for (i, _) in line.char_indices() {
        if line[i..].starts_with("  ") || line[i..].starts_with('\t') {
            return (&line[..i], line[i..].trim());
        }
    }
    (line, "")
}

pub(super) fn parse(text: &str) -> Help {
    let mut result = Help::default();
    let mut commands_section = false;
    let mut command_indent = None;
    for raw in clean(text).lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !raw.starts_with(char::is_whitespace) && line.ends_with(':') {
            commands_section = line.to_ascii_lowercase().contains("commands");
            command_indent = None;
            continue;
        }
        if !raw.starts_with(char::is_whitespace) {
            continue;
        }
        let (syntax, description) = split_columns(line);
        if syntax.starts_with('-') {
            let mut flags = Vec::new();
            for token in syntax.split(|c: char| c.is_whitespace() || c == ',' || c == '|') {
                let flag = token.split(['=', '[', '<']).next().unwrap_or("");
                if flag.starts_with('-')
                    && flag.len() > 1
                    && flag
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
                {
                    flags.push(flag.to_string());
                }
            }
            if flags.is_empty() {
                continue;
            }
            let value = syntax
                .find(['<', '['])
                .and_then(|i| {
                    let end = syntax[i..].find(['>', ']'])? + i;
                    Some(syntax[i + 1..end].trim_end_matches("...").to_string())
                })
                .or_else(|| {
                    syntax
                        .split([' ', '='])
                        .find(|s| {
                            !s.is_empty() && s.bytes().all(|c| c.is_ascii_uppercase() || c == b'_')
                        })
                        .map(str::to_string)
                });
            result.options.push(OptionSpec {
                flags,
                value,
                optional: syntax.contains('['),
                description: description.into(),
            });
        } else if commands_section {
            let indent = raw.len() - raw.trim_start().len();
            let expected = *command_indent.get_or_insert(indent);
            if indent != expected || description.is_empty() {
                continue;
            }
            let name = syntax
                .split_whitespace()
                .next()
                .unwrap_or("")
                .split('|')
                .next()
                .unwrap_or("");
            if name != "help"
                && name.bytes().next().is_some_and(|c| c.is_ascii_alphabetic())
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
            {
                result.commands.push((name.into(), description.into()));
            }
        }
    }
    result
}

pub(super) fn generate(command: &str) -> io::Result<String> {
    let start = Instant::now();
    let mut queue = VecDeque::from([Vec::<String>::new()]);
    let mut nodes = BTreeMap::new();
    while !queue.is_empty() {
        let batch: Vec<_> = (0..4).filter_map(|_| queue.pop_front()).collect();
        if nodes.len() + batch.len() > 128 || start.elapsed() > Duration::from_secs(120) {
            return Err(io::Error::other(
                "help scan exceeded its limit (128 scopes / 120 seconds)",
            ));
        }
        let results = std::thread::scope(|scope| {
            let workers: Vec<_> = batch
                .into_iter()
                .map(|path| {
                    scope.spawn(move || {
                        let mut args: Vec<_> = path.iter().map(String::as_str).collect();
                        args.push("--help");
                        let text = crate::prompt::helpers::run_cmd_inner(
                            command,
                            &args,
                            true,
                            Duration::from_secs(5),
                        )
                        .ok_or_else(|| {
                            io::Error::other(format!(
                                "help failed or timed out: {}",
                                path.join(" ")
                            ))
                        })?;
                        Ok::<_, io::Error>((path, parse(&text)))
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .unwrap_or_else(|_| Err(io::Error::other("help worker failed")))
                })
                .collect::<io::Result<Vec<_>>>()
        })?;
        for (path, help) in results {
            if path.is_empty() && help.options.is_empty() && help.commands.is_empty() {
                return Err(io::Error::other(
                    "no options found in --help; use a native generator or completion file",
                ));
            }
            if path.len() < 4 {
                for (name, _) in &help.commands {
                    let mut child = path.clone();
                    child.push(name.clone());
                    if !queue.contains(&child) && !nodes.contains_key(&child) {
                        queue.push_back(child);
                    }
                }
            }
            nodes.insert(path, help);
        }
    }
    Ok(render(command, &nodes))
}

fn escape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .flat_map(|c| {
            if "\\:[]".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .take(512)
        .collect()
}

fn function(command: &str, path: &[String]) -> String {
    format!(
        "_zsh_turbo_help_{command}_{}",
        path.iter()
            .map(|s| format!("{}_{s}", s.len()))
            .collect::<Vec<_>>()
            .join("_")
    )
}

fn render(command: &str, nodes: &BTreeMap<Vec<String>, Help>) -> String {
    let mut script = String::new();
    for (path, help) in nodes {
        script.push_str(&format!("{}() {{\n emulate -L zsh\n setopt extendedglob ${{_comp_options[@]}}\n local context state state_descr line\n local -a commands\n typeset -A opt_args\n _arguments -s -S -C", function(command, path)));
        let mut specs = Vec::new();
        for option in &help.options {
            for flag in &option.flags {
                let suffix = if option.value.is_some() {
                    if flag.starts_with("--") { "=" } else { "+" }
                } else {
                    ""
                };
                let mut spec = format!(
                    "({}){flag}{suffix}[{}]",
                    option.flags.join(" "),
                    escape(&option.description)
                );
                if let Some(value) = &option.value {
                    let lower = value.to_ascii_lowercase();
                    let action = if ["file", "path", "directory"]
                        .iter()
                        .any(|s| lower.contains(s))
                    {
                        "_files"
                    } else {
                        ""
                    };
                    spec.push_str(&format!(
                        "{}{}:{action}",
                        if option.optional { "::" } else { ":" },
                        escape(value)
                    ));
                }
                specs.push(spec);
            }
        }
        if help.commands.is_empty() {
            specs.push("*:argument:_files".into());
        } else {
            specs.extend(["1:command:->commands".into(), "*::arguments:->args".into()]);
        }
        for spec in specs {
            script.push_str(&format!(" \\\n  {}", quote(&spec)));
        }
        script.push('\n');
        if !help.commands.is_empty() {
            script.push_str(" case $state in\n commands)\n commands=(\n");
            for (name, desc) in &help.commands {
                script.push_str(&format!(
                    " {}\n",
                    quote(&format!("{name}:{}", escape(desc)))
                ));
            }
            script.push_str(" )\n _describe command commands ;;\n args)\n case ${words[1]} in\n");
            for (name, _) in &help.commands {
                let mut child = path.clone();
                child.push(name.clone());
                if nodes.contains_key(&child) {
                    script.push_str(&format!(
                        " {}) {} ;;\n",
                        quote(name),
                        function(command, &child)
                    ));
                }
            }
            script.push_str(" esac ;;\n esac\n");
        }
        script.push_str("}\n");
    }
    script.push_str(&format!(
        "compdef {} {}\n",
        function(command, &[]),
        quote(command)
    ));
    script
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_nested_commands_and_option_values() {
        let help = parse(
            "Usage: demo\nOptions:\n  -v, --verbose  Verbose output\n  --file <PATH>  Input file\n  --mode=MODE  Select mode\nCommands:\n  remote <name>  Manage remotes\n  help  Show help\n",
        );
        assert_eq!(help.options.len(), 3);
        assert_eq!(help.options[0].flags, ["-v", "--verbose"]);
        assert_eq!(help.options[1].value.as_deref(), Some("PATH"));
        assert_eq!(help.options[2].value.as_deref(), Some("MODE"));
        assert_eq!(help.commands.len(), 1);
    }
}
