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
    if command == "git" {
        return parse_git_top_level(text);
    }
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
        // 大文字だけのプレースホルダや、具体的な引数・説明文の行は候補にしない
        let syntax = words.next().is_none_or(|next| {
            next.starts_with(['-', '[', '<', '{'])
                || next.starts_with("...")
                || !next.bytes().any(|c| c.is_ascii_lowercase())
        });
        if syntax
            && (is_command_name(name) || name == "help")
            && name.bytes().any(|c| c.is_ascii_lowercase())
            && !result.commands.iter().any(|(existing, _)| existing == name)
        {
            result.commands.push((name.to_owned(), String::new()));
        }
    }
    result
}

fn parse_git_top_level(text: &str) -> super::TopLevel {
    let mut result = super::TopLevel::default();
    let mut commands_section = false;
    let mut seen = std::collections::HashSet::new();
    for raw in clean(text).lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if !raw.starts_with(char::is_whitespace) {
            let heading = line.trim_end_matches(':').to_ascii_lowercase();
            // Git 本体の節だけを読み、設定や PATH で変わるエイリアス・外部拡張は除く。
            commands_section = heading == "main porcelain commands"
                || heading.starts_with("ancillary commands / ")
                || heading.starts_with("low-level commands / ")
                || heading == "interacting with others";
            continue;
        }
        if commands_section {
            let (name, description) = split_columns(line);
            if super::valid_command(name) && seen.insert(name.to_owned()) {
                result.commands.push((name.into(), description.into()));
            }
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
            // フラグを読み取れない行 (`--` だけなど) は、コマンドの行としても扱わない
            if let Some(option) = parse_option(syntax, description) {
                if !description.is_empty() {
                    wrapped = Some((column_of(description), Wrapped::Option));
                }
                result.options.push(option);
            }
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

/// オプションの行を、構文の列 (`-o, --output <FILE>` など) と説明の列から読む。
/// フラグを 1 つも読み取れなければ None
fn parse_option(syntax: &str, description: &str) -> Option<OptionSpec> {
    // `--[no-]color` の角括弧は、値ではなくフラグ名の省略表記。
    let value_syntax = syntax.replace("[no-]", "");
    let mut flags = Vec::new();
    for token in syntax.split(|c: char| c.is_whitespace() || c == ',' || c == '|') {
        let expanded = if let Some((head, tail)) = token.split_once("[no-]") {
            vec![format!("{head}{tail}"), format!("{head}no-{tail}")]
        } else {
            vec![token.to_owned()]
        };
        for token in expanded {
            let flag = token.split(['=', '[', '<']).next().unwrap_or("");
            if is_flag_name(flag) && !flags.iter().any(|existing| existing == flag) {
                flags.push(flag.to_string());
            }
        }
    }
    if flags.is_empty() {
        return None;
    }
    Some(OptionSpec {
        flags,
        value: option_value(&value_syntax),
        optional: value_syntax.contains('['),
        description: description.into(),
    })
}

fn is_flag_name(flag: &str) -> bool {
    flag.starts_with('-')
        && flag != "--"
        && flag.len() > 1
        && flag
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_".contains(&c))
}

/// 値の名前を、`<FILE>`・`[=N]` のような括弧の中か、大文字だけの語 (`MODE`) から読む
fn option_value(value_syntax: &str) -> Option<String> {
    value_syntax
        .find(['<', '['])
        .and_then(|i| {
            let end = value_syntax[i..].find(['>', ']'])? + i;
            Some(value_syntax[i + 1..end].trim_end_matches("...").to_string())
        })
        .or_else(|| {
            value_syntax
                .split([' ', '='])
                .find(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_uppercase() || c == b'_'))
                .map(str::to_string)
        })
}

pub(super) fn generate(command: &str, binary: &std::path::Path) -> io::Result<String> {
    let binary = binary
        .to_str()
        .ok_or_else(|| io::Error::other("executable path is not UTF-8"))?;
    generate_with(command, 128, Duration::from_secs(120), &|path, timeout| {
        let mut args: Vec<_> = path.iter().map(String::as_str).collect();
        args.push("--help");
        crate::prompt::helpers::run_cmd_any_status(binary, &args, &[("LC_ALL", "C")], timeout)
    })
}

fn generate_with(
    command: &str,
    scope_limit: usize,
    time_limit: Duration,
    run: &(impl Fn(&[String], Duration) -> Option<String> + Sync),
) -> io::Result<String> {
    let start = Instant::now();
    let mut queue = VecDeque::from([Vec::<String>::new()]);
    let mut nodes = BTreeMap::new();
    let mut scanned = 0;
    while !queue.is_empty() {
        let remaining = time_limit.saturating_sub(start.elapsed());
        if scanned >= scope_limit || remaining.is_zero() {
            break;
        }
        let timeout = remaining.min(Duration::from_secs(5));
        let batch: Vec<_> = (0..4.min(scope_limit - scanned))
            .filter_map(|_| queue.pop_front())
            .collect();
        scanned += batch.len();
        let results = std::thread::scope(|scope| {
            let workers: Vec<_> = batch
                .into_iter()
                .map(|path| {
                    scope.spawn(move || {
                        let help = run(&path, timeout).map(|text| parse(&text));
                        (path, help)
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| {
                    worker
                        .join()
                        .map_err(|_| io::Error::other("help worker failed"))
                })
                .collect::<io::Result<Vec<_>>>()
        })?;
        for (path, help) in results {
            let Some(help) = help else {
                if path.is_empty() {
                    return Err(io::Error::other("root help failed or timed out"));
                }
                continue;
            };
            if help.options.is_empty() && help.commands.is_empty() {
                if path.is_empty() {
                    return Err(io::Error::other(
                        "no options found in --help; use a native generator or completion file",
                    ));
                }
                continue;
            }
            if path.len() < 4 {
                for (name, _) in &help.commands {
                    let mut child = path.clone();
                    child.push(name.clone());
                    if scanned + queue.len() < scope_limit
                        && !queue.contains(&child)
                        && !nodes.contains_key(&child)
                    {
                        queue.push_back(child);
                    }
                }
            }
            nodes.insert(path, help);
        }
    }
    if nodes.is_empty() {
        return Err(io::Error::other("no help scopes scanned"));
    }
    Ok(render(command, &nodes))
}

fn escape(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(512)
        .flat_map(|c| {
            if "\\:[]".contains(c) {
                vec!['\\', c]
            } else {
                vec![c]
            }
        })
        .collect()
}

fn function(command: &str, path: &[String]) -> String {
    format!(
        "_zsh_turbo_help_{}_{command}_{}",
        command.len(),
        path.iter()
            .map(|s| format!("{}_{s}", s.len()))
            .collect::<Vec<_>>()
            .join("_")
    )
}

// 補完関数は候補を追加したときだけ 0 を返す。0 を返すと _main_complete が matcher-list の
// 残りの照合 (部分一致など) を試さないため、末尾の case の終了状態をそのまま返さない。
fn render(command: &str, nodes: &BTreeMap<Vec<String>, Help>) -> String {
    let mut script = String::new();
    for (path, help) in nodes {
        script.push_str(&format!("{}() {{\n emulate -L zsh\n setopt extendedglob ${{_comp_options[@]}}\n local context state state_descr line ret=1\n local -a commands\n typeset -A opt_args\n _arguments -s -S -C", function(command, path)));
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
        script.push_str(" && ret=0\n");
        if !help.commands.is_empty() {
            script.push_str(" case $state in\n commands)\n commands=(\n");
            for (name, desc) in &help.commands {
                script.push_str(&format!(
                    " {}\n",
                    quote(&format!("{name}:{}", escape(desc)))
                ));
            }
            script.push_str(
                " )\n _describe command commands && ret=0 ;;\n args)\n case ${words[1]} in\n",
            );
            for (name, _) in &help.commands {
                let mut child = path.clone();
                child.push(name.clone());
                if nodes.contains_key(&child) {
                    script.push_str(&format!(
                        " {}) {} && ret=0 ;;\n",
                        quote(name),
                        function(command, &child)
                    ));
                }
            }
            script.push_str(" esac ;;\n esac\n");
        }
        script.push_str(" return ret\n}\n");
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
    fn 子のヘルプが取れなくても親と他の子の補完は残す() {
        let script = generate_with("demo", 128, Duration::from_secs(1), &|path, _| match path
            .first()
            .map(String::as_str)
        {
            None => Some("Commands:\n  good  Usable\n  broken  Failed\n  empty  No help\n".into()),
            Some("good") => Some("Options:\n  --usable  Usable flag\n".into()),
            Some("empty") => Some(String::new()),
            _ => None,
        })
        .unwrap();
        assert!(script.contains("good:Usable"));
        assert!(script.contains("--usable[Usable flag]"));
        assert!(script.contains(&format!(
            "'good') {} && ret=0 ;;",
            function("demo", &["good".into()])
        )));
        assert!(!script.contains("'broken')"));
        assert!(!script.contains("'empty')"));
        for text in [None, Some(String::new())] {
            assert!(
                generate_with("demo", 128, Duration::from_secs(1), &|_, _| text.clone()).is_err()
            );
        }
    }

    #[test]
    fn 走査上限を超えても取得済みの補完を返す() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let calls = AtomicUsize::new(0);
        let script = generate_with("demo", 2, Duration::from_secs(1), &|path, timeout| {
            calls.fetch_add(1, Ordering::Relaxed);
            assert!(timeout <= Duration::from_secs(1));
            Some(if path.is_empty() {
                "Commands:\n  first  First command\n  second  Second command\n  third  Third command\n".into()
            } else {
                "Options:\n  --flag  Test flag\n".into()
            })
        })
        .unwrap();
        assert_eq!(calls.load(Ordering::Relaxed), 2);
        assert!(script.contains("third:Third command"));
        assert!(script.contains("--flag[Test flag]"));
        assert!(!script.contains("'second')"));
        assert!(
            generate_with("demo", 128, Duration::ZERO, &|_, _| panic!("must not run")).is_err()
        );
    }

    #[test]
    fn 時間上限に達したら取得済みの親の補完を返す() {
        let script = generate_with("demo", 128, Duration::from_millis(20), &|path, _| {
            assert!(path.is_empty());
            std::thread::sleep(Duration::from_millis(30));
            Some("Commands:\n  child  Child command\nOptions:\n  --flag  Test flag\n".into())
        })
        .unwrap();
        assert!(script.contains("child:Child command"));
        assert!(script.contains("--flag[Test flag]"));
        assert!(!script.contains("'child')"));
    }

    #[test]
    fn 否定形を含むオプション名を展開して値と区別する() {
        let help = parse(
            "Options:\n  --[no-]color  Colorize output\n  --[no-]mode[=MODE]  Select mode\n  --  End options\n",
        );
        assert_eq!(help.options.len(), 2);
        assert_eq!(help.options[0].flags, ["--color", "--no-color"]);
        assert!(help.options[0].value.is_none());
        assert!(!help.options[0].optional);
        assert_eq!(help.options[1].flags, ["--mode", "--no-mode"]);
        assert_eq!(help.options[1].value.as_deref(), Some("=MODE"));
        assert!(help.options[1].optional);
        let nodes = BTreeMap::from([(Vec::new(), help)]);
        let script = render("demo", &nodes);
        assert!(script.contains("(--color --no-color)--color[Colorize output]"));
        assert!(!script.contains("(--)--="));
    }

    #[test]
    fn オプションの行からフラグと値と説明を読む() {
        let option = |syntax: &str, description: &str| parse_option(syntax, description).unwrap();

        let color = option("--[no-]color", "Colorize output");
        assert_eq!(color.flags, ["--color", "--no-color"]);
        assert_eq!(color.value, None);
        assert!(!color.optional);
        assert_eq!(color.description, "Colorize output");

        let output = option("-o, --output <FILE>", "Write to FILE");
        assert_eq!(output.flags, ["-o", "--output"]);
        assert_eq!(output.value.as_deref(), Some("FILE"));
        assert!(!output.optional);
        assert_eq!(output.description, "Write to FILE");

        // 省略できる値は括弧の中をそのまま値の名前にする
        let level = option("--level[=N]", "");
        assert_eq!(level.flags, ["--level"]);
        assert_eq!(level.value.as_deref(), Some("=N"));
        assert!(level.optional);
        assert_eq!(level.description, "");

        let verbose = option("-v|--verbose", "Verbose output");
        assert_eq!(verbose.flags, ["-v", "--verbose"]);
        assert_eq!(verbose.value, None);
        assert!(!verbose.optional);

        assert_eq!(option("--mode MODE", "").value.as_deref(), Some("MODE"));
        assert_eq!(option("--file=PATH", "").value.as_deref(), Some("PATH"));
        assert_eq!(
            option("--include <GLOB...>", "").value.as_deref(),
            Some("GLOB")
        );
        assert_eq!(option("-h, -h, --help", "").flags, ["-h", "--help"]);
    }

    #[test]
    fn フラグを読み取れないオプションの行はnoneを返す() {
        for syntax in ["--", "-", "-<n>", "-é", "- name"] {
            assert!(parse_option(syntax, "Description").is_none(), "{syntax}");
        }
    }

    #[test]
    fn フラグを読み取れない行は折り返しを切りコマンドとしても読まない() {
        let options =
            parse("Options:\n  --ok  First\n        continued\n  --  Invalid\n        ignored\n");
        assert_eq!(options.options.len(), 1);
        assert_eq!(options.options[0].description, "First continued");
        // `--` の行でコマンドの字下げを決めない
        let commands = parse("Commands:\n  --  Separator\n    run  Run a script\n");
        assert_eq!(
            commands.commands,
            [("run".to_owned(), "Run a script".to_owned())]
        );
        assert!(commands.options.is_empty());
    }

    #[test]
    fn 説明の長さ上限でエスケープを途中で切らない() {
        for c in ['[', ']', ':', '\\'] {
            let description = format!("{}{c}ignored", "a".repeat(511));
            assert_eq!(escape(&description), format!("{}\\{c}", "a".repeat(511)));
        }
        assert_eq!(
            escape(&format!("{}[ignored", "a".repeat(512))),
            "a".repeat(512)
        );
    }

    #[test]
    fn 候補を足せたときだけ補完関数が0を返す() {
        let parent = parse("Options:\n  --config <FILE>  Config file\nCommands:\n  child  Child\n");
        let child = parse("Options:\n  --flag  Flag\n");
        let nodes = BTreeMap::from([(Vec::new(), parent), (vec!["child".to_string()], child)]);
        let script = render("demo", &nodes);
        // 末尾の case の終了状態 (一致しなければ 0) を返さず、各補完の成否だけを ret に集める
        assert_eq!(
            script
                .matches(" local context state state_descr line ret=1\n")
                .count(),
            2
        );
        assert_eq!(script.matches(" && ret=0\n").count(), 2);
        assert_eq!(script.matches(" return ret\n}\n").count(), 2);
        assert!(script.contains(" _describe command commands && ret=0 ;;\n"));
        assert!(script.contains(&format!(
            " 'child') {} && ret=0 ;;\n",
            function("demo", &["child".into()])
        )));
    }

    #[test]
    fn 別のcliとサブコマンドの関数名が衝突しない() {
        let first = function("a", &["b".into(), "c".into()]);
        let second = function("a_1_b", &["c".into()]);
        assert_ne!(first, second);
    }
    #[test]
    fn gitの全一覧から実行コマンドだけを説明付きで読む() {
        let help = parse_top_level(
            "git",
            "See 'git help <command>' for help\n\nMain Porcelain Commands\n   clean          Remove untracked files\n   clone          Clone a repository\n\nAncillary Commands / Manipulators\n   config         Read configuration\n\nInteracting with Others\n   svn            Work with Subversion\n\nLow-level Commands / Internal Helpers\n   check-ignore   Check ignore rules\n   help           Show help\n\nUser-facing repository, command and file interfaces\n   cli            Command-line conventions\n\nDeveloper-facing file formats, protocols and other interfaces\n   protocol-v2    Wire protocol\n\nExternal commands\n   lfs\n   custom.tool\n   bad;command\n\nCommand aliases\n   co             checkout\n   clone          duplicate\n",
        );
        assert_eq!(
            help.commands,
            [
                ("clean", "Remove untracked files"),
                ("clone", "Clone a repository"),
                ("config", "Read configuration"),
                ("svn", "Work with Subversion"),
                ("check-ignore", "Check ignore rules"),
                ("help", "Show help"),
            ]
            .map(|(name, text)| (name.into(), text.into()))
        );
        assert!(help.options.is_empty());
    }

    #[test]
    fn 使用例からcli自身のサブコマンドだけを重複なく読む() {
        let help = parse_top_level(
            "brew",
            "Example usage:\n  brew search TEXT|/REGEX/\n  brew install FORMULA|CASK...\n  brew update\n\nTroubleshooting:\n  brew config\n  brew install --verbose --debug FORMULA|CASK\n\nFurther help:\n  brew help [COMMAND]\n  man brew\n  https://docs.brew.sh\n  brew --version\n  brew <COMMAND>\n  brew $(echo injected)\n  brew install;echo\n  brewery remove\n  brew FORMULA\n  brew is a package manager\n  brew remove example-package\n",
        );
        assert_eq!(
            help.commands,
            ["search", "install", "update", "config", "help"]
                .map(|name| (name.to_owned(), String::new()))
        );
        assert!(help.options.is_empty());
        let described = parse_top_level(
            "demo",
            "Commands:\n  sync  Update the environment\nExamples:\n  demo sync\n  demo other\n",
        );
        assert_eq!(
            described.commands,
            [("sync".to_owned(), "Update the environment".to_owned())]
        );
    }

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
