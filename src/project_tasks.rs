#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const MAX_PROJECT_FILE_BYTES: u64 = 1024 * 1024;
/// 入力欄の下の一覧に送るタスクの上限。入力中は候補の最大数の行まで表示し、残りは ↓ のメニューで選ぶ
pub const LIST_LIMIT: usize = 256;

pub fn candidates(query: &str, max: usize) -> Vec<String> {
    labeled_candidates(query, max)
        .into_iter()
        .map(|(_, command)| command)
        .collect()
}

/// タスク名と、採用後のコマンド全体の組を返す (一覧表示用)。
pub fn labeled_candidates(query: &str, max: usize) -> Vec<(String, String)> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    labeled_in(query, &cwd, max)
}

#[cfg(test)]
fn candidates_in(query: &str, cwd: &Path, max: usize) -> Vec<String> {
    labeled_in(query, cwd, max)
        .into_iter()
        .map(|(_, command)| command)
        .collect()
}

/// タスクを実行するコマンド。zsh 側は利用記録の前にこの名前で始まる行だけを選ぶ
pub const COMMANDS: &[&str] = &[
    "make", "just", "task", "npm", "pnpm", "bun", "yarn", "uv", "deno", "mise",
];

/// make・just・task はターゲットやレシピを直接引数に取る
fn takes_task_directly(command: &str) -> bool {
    matches!(command, "make" | "just" | "task")
}

/// スクリプト・タスクの前に `run` などのサブコマンドが要る CLI と、そのサブコマンド。
/// 名前だけを入力した段階では、CLI 自身のサブコマンドを一覧にする (`subcommand_query`)。
fn run_keywords(command: &str) -> Option<&'static [&'static str]> {
    Some(match command {
        "npm" => &["run", "run-script"],
        "pnpm" | "bun" | "yarn" | "uv" => &["run"],
        "deno" => &["task"],
        "mise" => &["run", "r"],
        _ => return None,
    })
}

/// `command` がカレントディレクトリの定義ファイルから読むタスク名 (名前順)
fn task_names(command: &str, cwd: &Path) -> Option<Vec<String>> {
    Some(match command {
        "make" => make_targets(cwd),
        "just" => just_recipes(cwd),
        "task" => taskfile_tasks(cwd),
        "npm" | "bun" | "yarn" => package_scripts(cwd, false),
        "pnpm" => package_scripts(cwd, true),
        "uv" => uv_scripts(cwd),
        "deno" => deno_tasks(cwd),
        "mise" => mise_tasks(cwd),
        _ => return None,
    })
}

/// カレントディレクトリに `command` のタスク `name` が定義されているか
pub fn defines(command: &str, name: &str, cwd: &Path) -> bool {
    task_names(command, cwd).is_some_and(|names| names.iter().any(|n| n == name))
}

/// 入力中の行のコマンド名 (最初の語)
pub fn command_of(query: &str) -> &str {
    query
        .split_once(char::is_whitespace)
        .map_or(query, |(command, _)| command)
}

/// 実行した行が単純なタスクの呼び出しなら (コマンド名, タスク名) を返す。
/// 利用記録と履歴の集計で同じ規則を使う。タスク名は定義ファイルに書ける名前だけで、
/// ほかの語 (`VAR=$HOME` など) は照合に使わないため展開を含んでもよい。
/// 語の区切りが変わるクォート・エスケープ、別のコマンドをつなぐ演算子、
/// ディレクトリや定義ファイルを変えるオプション、複数の対象を含む行は、
/// どのタスクか確定できないため対象外にする。
pub fn invocation(line: &str) -> Option<(&str, &str)> {
    const SYNTAX: &[char] = &[';', '&', '|', '<', '>', '(', ')', '`', '\'', '"', '\\'];
    if line.contains(SYNTAX) || line.chars().any(char::is_control) {
        return None;
    }
    let mut words = line.split(' ').filter(|word| !word.is_empty());
    let command = words.next()?;
    let name = if command == "make" {
        let mut target = None;
        for word in words {
            if word.starts_with('-') {
                if changes_makefile(word) {
                    return None;
                }
            } else if !word.contains('=') && target.replace(word).is_some() {
                return None;
            }
        }
        target?
    } else if takes_task_directly(command) {
        // just・task はオプションを前に置く形を記録しない (定義ファイルや作業ディレクトリを変え得る)
        words.next().filter(|word| !word.starts_with('-'))?
    } else {
        let keyword = words.next()?;
        if !run_keywords(command)?.contains(&keyword) {
            return None;
        }
        words.next().filter(|word| !word.starts_with('-'))?
    };
    safe_name(name).then_some((command, name))
}

/// make の `-C dir`・`-f file` など、読む Makefile を変えるオプション
fn changes_makefile(option: &str) -> bool {
    match option.strip_prefix("--") {
        Some(long) => matches!(
            long.split('=').next(),
            Some("directory" | "file" | "makefile")
        ),
        None => option[1..].contains(['C', 'f']),
    }
}

fn labeled_in(query: &str, cwd: &Path, max: usize) -> Vec<(String, String)> {
    if max == 0 || query.chars().any(char::is_control) {
        return Vec::new();
    }
    let (command, rest) = query.split_once(char::is_whitespace).unwrap_or((query, ""));
    let rest = rest.trim_start_matches(char::is_whitespace);
    if takes_task_directly(command) {
        let head = if query == command {
            format!("{command} ")
        } else {
            query[..query.len() - rest.len()].to_owned()
        };
        let names = task_names(command, cwd).unwrap_or_default();
        return matching(&head, rest, names, max);
    }
    let Some(prefix) = run_keywords(command)
        .into_iter()
        .flatten()
        .find_map(|keyword| after_keyword(rest, keyword))
    else {
        return Vec::new();
    };
    let names = task_names(command, cwd).unwrap_or_default();
    let head = if prefix.is_empty() && !query.ends_with(char::is_whitespace) {
        // `npm run` のように、サブコマンド直後でも候補を表示する
        format!("{query} ")
    } else {
        query[..query.len() - prefix.len()].to_owned()
    };
    matching(&head, prefix, names, max)
}

/// `uv` や `npm i` のように、`run` 等を要する CLI の名前か 2 語目を入力中なら
/// (CLI 名, 候補の前に残す部分, 入力中の語) を返す。一覧には CLI のサブコマンドを出す。
pub fn subcommand_query(query: &str) -> Option<(&str, String, &str)> {
    if query.chars().any(char::is_control) {
        return None;
    }
    let (command, rest) = query.split_once(char::is_whitespace).unwrap_or((query, ""));
    run_keywords(command)?;
    let word = rest.trim_start_matches(char::is_whitespace);
    if word.chars().any(char::is_whitespace) {
        return None;
    }
    let head = if query == command {
        format!("{command} ")
    } else {
        query[..query.len() - word.len()].to_owned()
    };
    Some((command, head, word))
}

fn after_keyword<'a>(rest: &'a str, keyword: &str) -> Option<&'a str> {
    let after = rest.strip_prefix(keyword)?;
    if !after.is_empty() && !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some(after.trim_start_matches(char::is_whitespace))
}

fn matching(head: &str, prefix: &str, names: Vec<String>, max: usize) -> Vec<(String, String)> {
    if prefix.chars().any(char::is_whitespace) || prefix.starts_with('-') {
        return Vec::new();
    }
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix) && name != prefix)
        .take(max)
        .map(|name| {
            let command = format!("{head}{name}");
            (name, command)
        })
        .collect()
}

fn read_small(path: &Path) -> Option<String> {
    if path.metadata().ok()?.len() > MAX_PROJECT_FILE_BYTES {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '+' | '/'))
}

fn make_targets(cwd: &Path) -> Vec<String> {
    let Some(text) = ["GNUmakefile", "makefile", "Makefile"]
        .into_iter()
        .find_map(|name| read_small(&cwd.join(name)))
    else {
        return Vec::new();
    };
    let mut targets = Vec::new();
    for line in text.lines() {
        if line.starts_with('\t') {
            continue;
        }
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((heads, rest)) = line.split_once(':') else {
            continue;
        };
        if heads.contains(['=', '$', '%'])
            || heads.starts_with('.')
            || rest.trim_start().starts_with(":=")
            || rest.trim_start().starts_with('=')
        {
            continue;
        }
        targets.extend(
            heads
                .split_whitespace()
                .filter(|name| safe_name(name))
                .map(str::to_owned),
        );
    }
    targets.sort_unstable();
    targets.dedup();
    targets
}

fn package_scripts(cwd: &Path, pnpm: bool) -> Vec<String> {
    let Some(text) = read_small(&cwd.join("package.json")) else {
        return Vec::new();
    };
    let Ok(document) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = document
        .get("scripts")
        .and_then(serde_json::Value::as_object)
    else {
        return Vec::new();
    };
    let mut names: Vec<String> = scripts
        .iter()
        .filter(|(name, value)| {
            value.is_string() && safe_name(name) && (!pnpm || !name.starts_with('.'))
        })
        .map(|(name, _)| name.clone())
        .collect();
    names.sort_unstable();
    names
}

fn uv_scripts(cwd: &Path) -> Vec<String> {
    let Some(text) = read_small(&cwd.join("pyproject.toml")) else {
        return Vec::new();
    };
    let Ok(document) = toml::from_str::<toml::Value>(&text) else {
        return Vec::new();
    };
    let Some(scripts) = document
        .get("project")
        .and_then(|project| project.get("scripts"))
        .and_then(toml::Value::as_table)
    else {
        return Vec::new();
    };
    let mut names: Vec<String> = scripts
        .iter()
        .filter(|(name, value)| value.is_str() && safe_name(name))
        .map(|(name, _)| name.clone())
        .collect();
    names.sort_unstable();
    names
}

fn deno_tasks(cwd: &Path) -> Vec<String> {
    let mut names = package_scripts(cwd, false);
    if let Some((file, text)) = ["deno.json", "deno.jsonc"]
        .into_iter()
        .find_map(|file| read_small(&cwd.join(file)).map(|text| (file, text)))
    {
        let document = if file == "deno.jsonc" {
            json5::from_str::<serde_json::Value>(&text).ok()
        } else {
            serde_json::from_str::<serde_json::Value>(&text).ok()
        };
        if let Some(tasks) = document
            .as_ref()
            .and_then(|document| document.get("tasks"))
            .and_then(serde_json::Value::as_object)
        {
            names.extend(
                tasks
                    .iter()
                    .filter(|(name, value)| safe_name(name) && value.is_string())
                    .map(|(name, _)| name.clone()),
            );
        }
    }
    names.sort_unstable();
    names.dedup();
    names
}

fn mise_tasks(cwd: &Path) -> Vec<String> {
    let mut names = Vec::new();
    if let Some(text) = ["mise.toml", ".mise.toml"]
        .into_iter()
        .find_map(|file| read_small(&cwd.join(file)))
        && let Ok(document) = toml::from_str::<toml::Value>(&text)
        && let Some(tasks) = document.get("tasks").and_then(toml::Value::as_table)
    {
        names.extend(tasks.keys().filter(|name| safe_name(name)).cloned());
    }
    for dir in [
        "mise-tasks",
        ".mise-tasks",
        "mise/tasks",
        ".mise/tasks",
        ".config/mise/tasks",
    ] {
        collect_mise_file_tasks(&cwd.join(dir), "", 0, &mut names);
    }
    names.sort_unstable();
    names.dedup();
    names
}

fn collect_mise_file_tasks(dir: &Path, prefix: &str, depth: usize, names: &mut Vec<String>) {
    if depth > 5 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten().take(1024) {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if !safe_name(&name) || name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let task_name = if prefix.is_empty() {
            name.clone()
        } else if name == "_default" {
            prefix.to_owned()
        } else {
            format!("{prefix}:{name}")
        };
        if kind.is_dir() {
            collect_mise_file_tasks(&entry.path(), &task_name, depth + 1, names);
        } else if kind.is_file() && mise_file_is_executable(&entry.path()) {
            names.push(task_name);
        }
    }
}

#[cfg(unix)]
fn mise_file_is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn mise_file_is_executable(_path: &Path) -> bool {
    true
}

fn just_recipes(cwd: &Path) -> Vec<String> {
    let Some(text) = ["justfile", "Justfile", ".justfile"]
        .into_iter()
        .find_map(|file| read_small(&cwd.join(file)))
    else {
        return Vec::new();
    };
    let mut names = Vec::new();
    let mut private = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.starts_with('[') {
            if let Some(attributes) = line.strip_prefix('[').and_then(|line| line.split_once(']')) {
                private |= attributes
                    .0
                    .split(',')
                    .any(|attribute| attribute.trim() == "private");
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with(char::is_whitespace) {
            continue;
        }
        if let Some((head, tail)) = line.split_once(':') {
            let name = head.split_whitespace().next().unwrap_or("");
            if !tail.starts_with('=') && !private && !name.starts_with('_') && safe_name(name) {
                names.push(name.to_owned());
            }
        }
        private = false;
    }
    names.sort_unstable();
    names.dedup();
    names
}

fn taskfile_tasks(cwd: &Path) -> Vec<String> {
    let Some(text) = [
        "Taskfile.yml",
        "taskfile.yml",
        "Taskfile.yaml",
        "taskfile.yaml",
        "Taskfile.dist.yml",
        "taskfile.dist.yml",
        "Taskfile.dist.yaml",
        "taskfile.dist.yaml",
    ]
    .into_iter()
    .find_map(|file| read_small(&cwd.join(file))) else {
        return Vec::new();
    };
    let Ok(document) = serde_yaml_ng::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    let Some(tasks) = document.get("tasks").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    let mut names: Vec<String> = tasks
        .iter()
        .filter(|(name, value)| {
            safe_name(name)
                && !value
                    .get("internal")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
        })
        .map(|(name, _)| name.clone())
        .collect();
    names.sort_unstable();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_directory_task_files_supply_only_matching_commands() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("Makefile"),
            ".PHONY: all\nall: build\nbuild test: deps\n\t@echo ignored: x\nVAR := value\n%.o: %.c\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("package.json"),
            r#"{"scripts":{"dev":"vite","deploy":"ship",".hidden":"helper","bad name":"noop"}}"#,
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("pyproject.toml"),
            "[project.scripts]\nhello = 'pkg:main'\nhello-world = 'pkg:other'\n[tool.hatch.envs.default.scripts]\nnot-a-project-script = 'nope'\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("deno.jsonc"),
            "{ // local tasks\n \"tasks\": {\"check\": \"deno check .\", \"deploy\": \"deno run main.ts\",},\n}",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("mise.toml"),
            "[tasks]\ncheck = 'cargo check'\n[tasks.deploy]\nrun = 'echo deployed'\n",
        )
        .unwrap();
        std::fs::create_dir_all(tmp.path().join(".mise/tasks/test")).unwrap();
        let file_task = tmp.path().join(".mise/tasks/test/_default");
        std::fs::write(&file_task, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        std::fs::set_permissions(&file_task, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            tmp.path().join("justfile"),
            "build:\n  echo build\n[private]\nsecret:\n  echo hidden\n[no-cd, private]\nsecond-secret:\n  echo hidden\ncheck arg:\n  echo check\n_hidden:\n  echo hidden\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("Taskfile.yml"),
            "version: '3'\ntasks:\n  build:\n    cmds: ['echo build']\n  deploy:\n    cmds: ['echo deploy']\n  hidden:\n    internal: true\n    cmds: ['echo hidden']\n",
        )
        .unwrap();
        assert_eq!(candidates_in("make bu", tmp.path(), 10), ["make build"]);
        assert_eq!(
            candidates_in("make ", tmp.path(), 10),
            ["make all", "make build", "make test"]
        );
        assert_eq!(
            candidates_in("make", tmp.path(), 10),
            ["make all", "make build", "make test"]
        );
        assert_eq!(
            candidates_in("pnpm run", tmp.path(), 10),
            ["pnpm run deploy", "pnpm run dev"]
        );
        assert_eq!(
            candidates_in("npm run", tmp.path(), 10),
            ["npm run .hidden", "npm run deploy", "npm run dev"]
        );
        assert_eq!(
            candidates_in("uv run", tmp.path(), 10),
            ["uv run hello", "uv run hello-world"]
        );
        assert_eq!(
            candidates_in("deno task", tmp.path(), 10),
            [
                "deno task .hidden",
                "deno task check",
                "deno task deploy",
                "deno task dev"
            ]
        );
        assert_eq!(
            candidates_in("mise run", tmp.path(), 10),
            ["mise run check", "mise run deploy", "mise run test"]
        );
        // `run` 等を要する CLI は名前だけ・2 語目ではタスクを出さない (サブコマンドの一覧に任せる)
        for query in [
            "npm", "pnpm", "bun", "yarn", "uv", "deno", "mise", "pnpm de", "npm i", "uv sy",
        ] {
            assert!(candidates_in(query, tmp.path(), 10).is_empty(), "{query}");
        }
        assert_eq!(
            candidates_in("pnpm run de", tmp.path(), 10),
            ["pnpm run deploy", "pnpm run dev"]
        );
        assert_eq!(
            candidates_in("bun run de", tmp.path(), 10),
            ["bun run deploy", "bun run dev"]
        );
        assert_eq!(
            candidates_in("uv run he", tmp.path(), 10),
            ["uv run hello", "uv run hello-world"]
        );
        assert_eq!(
            candidates_in("npm run de", tmp.path(), 10),
            ["npm run deploy", "npm run dev"]
        );
        assert_eq!(
            candidates_in("yarn run de", tmp.path(), 10),
            ["yarn run deploy", "yarn run dev"]
        );
        assert_eq!(
            candidates_in("deno task ch", tmp.path(), 10),
            ["deno task check"]
        );
        assert_eq!(
            candidates_in("mise run ch", tmp.path(), 10),
            ["mise run check"]
        );
        assert_eq!(candidates_in("mise r te", tmp.path(), 10), ["mise r test"]);
        assert_eq!(candidates_in("just ch", tmp.path(), 10), ["just check"]);
        assert_eq!(candidates_in("task bu", tmp.path(), 10), ["task build"]);
        assert!(candidates_in("just se", tmp.path(), 10).is_empty());
        assert!(candidates_in("just second", tmp.path(), 10).is_empty());
        assert!(candidates_in("task hi", tmp.path(), 10).is_empty());
        assert!(candidates_in("uv he", tmp.path(), 10).is_empty());
        assert!(candidates_in("echo make bu", tmp.path(), 10).is_empty());
        assert!(candidates_in("make -j", tmp.path(), 10).is_empty());
        assert!(candidates_in("make bu other", tmp.path(), 10).is_empty());
        // 一覧にはタスク名を表示し、採用時はサブコマンドを含むコマンド全体に置き換える
        assert_eq!(
            labeled_in("npm run", tmp.path(), 10)[1],
            ("deploy".to_owned(), "npm run deploy".to_owned())
        );
        assert_eq!(
            labeled_in("mise r te", tmp.path(), 10),
            [("test".to_owned(), "mise r test".to_owned())]
        );
    }

    #[test]
    fn run等を要するcliの名前と2語目はサブコマンドの入力として返す() {
        assert_eq!(subcommand_query("uv"), Some(("uv", "uv ".to_owned(), "")));
        assert_eq!(subcommand_query("uv "), Some(("uv", "uv ".to_owned(), "")));
        assert_eq!(
            subcommand_query("npm  i"),
            Some(("npm", "npm  ".to_owned(), "i"))
        );
        assert_eq!(
            subcommand_query("mise --"),
            Some(("mise", "mise ".to_owned(), "--"))
        );
        assert_eq!(subcommand_query("uv sync "), None);
        assert_eq!(subcommand_query("make"), None);
        assert_eq!(subcommand_query("uvx"), None);
        assert_eq!(subcommand_query("uv\ta"), None);
    }

    #[test]
    fn no_project_file_does_not_supply_tasks() {
        let tmp = tempfile::tempdir().unwrap();
        for query in [
            "make ",
            "pnpm ",
            "bun ",
            "uv run ",
            "npm run ",
            "yarn ",
            "deno task ",
            "mise run ",
            "just ",
            "task ",
        ] {
            assert!(candidates_in(query, tmp.path(), 10).is_empty());
        }
    }

    #[test]
    fn 単純なタスクの呼び出しだけをコマンド名とタスク名に分ける() {
        for (line, expected) in [
            ("make install", ("make", "install")),
            ("make install PREFIX=~/.local", ("make", "install")),
            ("make CC=$HOME/bin/cc install", ("make", "install")),
            ("make -j8 install", ("make", "install")),
            ("make  install", ("make", "install")),
            (" make install", ("make", "install")),
            ("just build release", ("just", "build")),
            ("task build -- -v", ("task", "build")),
            ("npm run build -- --watch", ("npm", "build")),
            ("npm run-script build", ("npm", "build")),
            ("pnpm run dev", ("pnpm", "dev")),
            ("bun run dev", ("bun", "dev")),
            ("yarn run dev", ("yarn", "dev")),
            ("uv run hello", ("uv", "hello")),
            ("deno task check", ("deno", "check")),
            ("mise run test:unit", ("mise", "test:unit")),
            ("mise r test", ("mise", "test")),
        ] {
            assert_eq!(invocation(line), Some(expected), "{line}");
        }
        for line in [
            "make",
            "make install test",
            "make -C sub install",
            "make -sC sub install",
            "make --directory=sub install",
            "make -f other.mk install",
            "make --makefile other.mk install",
            "make $TARGET",
            "make 'install'",
            "make install && make test",
            "make install > log",
            "make install\nmake test",
            "make\tinstall",
            "cd sub && make install",
            "makefoo install",
            "just --justfile other build",
            "just",
            "npm install",
            "npm run",
            "npm run -s build",
            "pnpm -C sub run dev",
            "mise tasks",
        ] {
            assert_eq!(invocation(line), None, "{line}");
        }
    }

    #[test]
    fn 記録対象のコマンドはすべてタスク定義を読める() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("Makefile"), "build:\n").unwrap();
        for command in COMMANDS {
            assert!(task_names(command, tmp.path()).is_some(), "{command}");
        }
        assert!(task_names("cargo", tmp.path()).is_none());
        assert!(defines("make", "build", tmp.path()));
        assert!(!defines("make", "missing", tmp.path()));
        assert!(!defines("just", "build", tmp.path()));
        assert_eq!(command_of("make"), "make");
        assert_eq!(command_of("npm run de"), "npm");
    }
}
