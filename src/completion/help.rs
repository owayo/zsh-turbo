use crate::shell_single_quote as quote;
use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::time::{Duration, Instant};

#[derive(Debug, Default)]
pub(super) struct Help {
    options: Vec<OptionSpec>,
    commands: Vec<(String, String)>,
}

impl Help {
    /// サブコマンドと、フラグごとに分けたオプションを (名前, 説明) の組で返す
    pub(super) fn into_top_level(self) -> super::TopLevel {
        let options = self
            .options
            .into_iter()
            .flat_map(|option| {
                let description = option.description;
                option
                    .flags
                    .into_iter()
                    .map(move |flag| (flag, description.clone()))
            })
            .collect();
        super::TopLevel {
            commands: self.commands,
            options,
        }
    }
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

fn is_command_name(name: &str) -> bool {
    name != "help"
        && name.bytes().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
}

/// 折り返された説明の続きを足す先
#[derive(Clone, Copy)]
enum Wrapped {
    Command,
    Option,
}

/// 通常の一覧がないヘルプでは、字下げした `CLI サブコマンド ...` の使用例を読む。
pub(super) fn parse_top_level(command: &str, text: &str) -> super::TopLevel {
    let mut result = parse(text).into_top_level();
    if !result.commands.is_empty() {
        return result;
    }
    for raw in clean(text).lines() {
        if !raw.starts_with([' ', '\t']) {
            continue;
        }
        let mut words = raw.split_whitespace();
        if words.next() != Some(command) {
            continue;
        }
        let Some(name) = words.next() else {
            continue;
        };
        if (is_command_name(name) || name == "help")
            && !result.commands.iter().any(|(existing, _)| existing == name)
        {
            result.commands.push((name.to_owned(), String::new()));
        }
    }
    result
}

pub(super) fn parse(text: &str) -> Help {
    let mut result = Help::default();
    let mut commands_section = false;
    let mut command_indent = None;
    // npm の `All commands:` のように名前だけをカンマ区切りで並べる節
    let mut name_list = false;
    // 直前の行の説明が始まる列。同じ列から始まる次の行は、折り返された説明の続き
    let mut wrapped: Option<(usize, Wrapped)> = None;
    for raw in clean(text).lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let indented = raw.starts_with(char::is_whitespace);
        let indent = raw.len() - raw.trim_start().len();
        // yarn v1 は `Commands:` の見出し自体を字下げする
        let indented_commands =
            indented && line.to_ascii_lowercase().ends_with("commands:") && !line.contains("  ");
        if (!indented && line.ends_with(':')) || indented_commands {
            commands_section = line.to_ascii_lowercase().contains("commands");
            command_indent = None;
            name_list = false;
            wrapped = None;
            continue;
        }
        if !indented {
            continue;
        }
        if let Some((column, target)) = wrapped.take()
            && indent == column
        {
            let last = match target {
                Wrapped::Command => result.commands.last_mut().map(|(_, text)| text),
                Wrapped::Option => result
                    .options
                    .last_mut()
                    .map(|option| &mut option.description),
            };
            if let Some(text) = last {
                text.push(' ');
                text.push_str(line);
            }
            wrapped = Some((column, target));
            continue;
        }
        // 説明の列の位置 (説明は行末までの部分)
        let column_of = |description: &str| raw.trim_end().len() - description.len();
        // yarn v1 の `- add` のような箇条書き
        if commands_section
            && let Some(name) = line.strip_prefix("- ")
            && is_command_name(name)
        {
            result.commands.push((name.into(), String::new()));
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
            if !description.is_empty() {
                wrapped = Some((column_of(description), Wrapped::Option));
            }
            result.options.push(OptionSpec {
                flags,
                value,
                optional: syntax.contains('['),
                description: description.into(),
            });
        } else if commands_section {
            let expected = *command_indent.get_or_insert(indent);
            if indent != expected {
                continue;
            }
            if description.is_empty() && (name_list || line.contains(',')) {
                name_list = true;
                result.commands.extend(
                    line.split(',')
                        .map(str::trim)
                        .filter(|name| is_command_name(name))
                        .map(|name| (name.to_owned(), String::new())),
                );
                continue;
            }
            if description.is_empty() {
                continue;
            }
            // bun のように使用例の列を挟む表では、最後の広い空白の後ろを説明とする
            // (文中の 2 つの空白は区切りとみなさない)
            let description = description
                .rsplit("   ")
                .next()
                .unwrap_or(description)
                .trim();
            let name = syntax
                .split_whitespace()
                .next()
                .unwrap_or("")
                .split('|')
                .next()
                .unwrap_or("");
            if is_command_name(name) {
                wrapped = Some((column_of(description), Wrapped::Command));
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

    #[test]
    fn npmのカンマ区切りとyarnの箇条書きのコマンド一覧を読む() {
        let npm = parse(
            "npm <command>\n\nUsage:\n\nnpm install        install all the dependencies\n\nAll commands:\n\n    access, adduser, audit,\n    ci, help, install,\n    whoami\n\nSpecify configs in the ini-formatted file:\n    /home/you/.npmrc\n",
        );
        let names: Vec<_> = npm.commands.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(
            names,
            ["access", "adduser", "audit", "ci", "install", "whoami"]
        );

        let yarn = parse(
            "\n  Usage: yarn [command] [flags]\n\n  Options:\n\n    -v, --version  output the version number\n  Commands:\n\n    - access\n    - add\n    - help\n\n  Run `yarn help COMMAND` for more information on specific commands.\n",
        );
        let names: Vec<_> = yarn
            .commands
            .iter()
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(names, ["access", "add"]);
        assert_eq!(yarn.options.len(), 1);
    }

    #[test]
    fn 使用例の列を除き折り返された説明をつなげる() {
        // bun: 名前・使用例・説明の 3 列。深い字下げの行は別の使用例
        let bun = parse(
            "Usage: bun <command>\n\nCommands:\n  run       ./my-script.ts       Execute a file with Bun\n            lint                 Run a package.json script\n  test                           Run unit tests with Bun\n",
        );
        assert_eq!(
            bun.commands,
            [
                ("run".to_owned(), "Execute a file with Bun".to_owned()),
                ("test".to_owned(), "Run unit tests with Bun".to_owned())
            ]
        );
        // mise・uv: 説明の列にそろえて折り返す
        let wrapped = parse(
            "Commands:\n  activate      Print the script to activate mise in an\n                interactive shell\n  cache         Manage the mise cache\n\nOptions:\n  -n, --no-cache  Avoid reading the cache,\n                  instead using a temporary directory\n  -q  Quiet.  Less output\n",
        );
        assert_eq!(
            wrapped.commands[0].1,
            "Print the script to activate mise in an interactive shell"
        );
        assert_eq!(wrapped.commands[1].1, "Manage the mise cache");
        assert_eq!(
            wrapped.options[0].description,
            "Avoid reading the cache, instead using a temporary directory"
        );
        // 文中の 2 つの空白は列の区切りとみなさない
        assert_eq!(wrapped.options[1].description, "Quiet.  Less output");
    }
}
