#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const MAX_PROJECT_FILE_BYTES: u64 = 1024 * 1024;

pub fn candidates(query: &str, max: usize) -> Vec<String> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    candidates_in(query, &cwd, max)
}

fn candidates_in(query: &str, cwd: &Path, max: usize) -> Vec<String> {
    if max == 0 || query.chars().any(char::is_control) {
        return Vec::new();
    }
    let Some((command, rest)) = query.split_once(char::is_whitespace) else {
        return Vec::new();
    };
    let rest = rest.trim_start_matches(char::is_whitespace);
    let (names, prefix) = match command {
        "make" => (make_targets(cwd), rest),
        "pnpm" | "bun" | "yarn" => (
            package_scripts(cwd, command == "pnpm"),
            after_keyword(rest, "run").unwrap_or(rest),
        ),
        "npm" => {
            let Some(prefix) =
                after_keyword(rest, "run").or_else(|| after_keyword(rest, "run-script"))
            else {
                return Vec::new();
            };
            (package_scripts(cwd, false), prefix)
        }
        "uv" => {
            let Some(prefix) = after_keyword(rest, "run") else {
                return Vec::new();
            };
            (uv_scripts(cwd), prefix)
        }
        "deno" => {
            let Some(prefix) = after_keyword(rest, "task") else {
                return Vec::new();
            };
            (deno_tasks(cwd), prefix)
        }
        "mise" => {
            let Some(prefix) = after_keyword(rest, "run").or_else(|| after_keyword(rest, "r"))
            else {
                return Vec::new();
            };
            (mise_tasks(cwd), prefix)
        }
        "just" => (just_recipes(cwd), rest),
        "task" => (taskfile_tasks(cwd), rest),
        _ => return Vec::new(),
    };
    matching(query, prefix, names, max)
}

fn after_keyword<'a>(rest: &'a str, keyword: &str) -> Option<&'a str> {
    let after = rest.strip_prefix(keyword)?;
    if !after.starts_with(char::is_whitespace) {
        return None;
    }
    Some(after.trim_start_matches(char::is_whitespace))
}

fn matching(query: &str, prefix: &str, names: Vec<String>, max: usize) -> Vec<String> {
    if prefix.chars().any(char::is_whitespace) || prefix.starts_with('-') {
        return Vec::new();
    }
    let head = &query[..query.len() - prefix.len()];
    names
        .into_iter()
        .filter(|name| name.starts_with(prefix) && name != prefix)
        .take(max)
        .map(|name| format!("{head}{name}"))
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
            candidates_in("pnpm de", tmp.path(), 10),
            ["pnpm deploy", "pnpm dev"]
        );
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
            candidates_in("yarn de", tmp.path(), 10),
            ["yarn deploy", "yarn dev"]
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
}
