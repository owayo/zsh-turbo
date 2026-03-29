use crate::config::{Config, CustomSegment};
use crate::icons::Icons;
use crate::style::{Color, parse_color};
use std::env;
use std::path::Path;
use std::process::Command;

use super::helpers::*;

pub struct Segment {
    pub text: String,
    pub icon: String,
    pub fg: Color,
    pub bg: Color,
}

pub fn make_segment(
    name: &str,
    config: &Config,
    icons: &Icons,
    last_status: i32,
    duration_ms: u64,
    jobs: usize,
) -> Option<Segment> {
    match name {
        "dir" => Some(segment_dir(config, icons)),
        "git" => segment_git(config, icons),
        "status" => segment_status(last_status, icons),
        "duration" => segment_duration(duration_ms, icons),
        "time" => Some(segment_time(icons)),
        "virtualenv" => segment_virtualenv(icons),
        "kubecontext" => segment_kubecontext(icons),
        "ssh" => segment_ssh(icons),
        "package" => segment_package(icons),
        "jobs" => segment_jobs(jobs, icons),
        "node" => segment_lang(
            icons.node,
            &["package.json", "node_modules"],
            "node",
            &["--version"],
            Color::White,
            Color::Ansi256(22),
        ),
        "python" => segment_lang(
            icons.python,
            &[
                "requirements.txt",
                "pyproject.toml",
                "Pipfile",
                ".python-version",
            ],
            "python3",
            &["--version"],
            Color::White,
            Color::Ansi256(24),
        ),
        "rust" => segment_lang(
            icons.rust,
            &["Cargo.toml"],
            "rustc",
            &["--version"],
            Color::White,
            Color::Ansi256(124),
        ),
        "go" => segment_lang(
            icons.go_lang,
            &["go.mod"],
            "go",
            &["version"],
            Color::Black,
            Color::Ansi256(30),
        ),
        "ruby" => segment_lang(
            icons.ruby,
            &["Gemfile", ".ruby-version", "Rakefile"],
            "ruby",
            &["--version"],
            Color::White,
            Color::Ansi256(124),
        ),
        "java" => segment_lang(
            icons.java,
            &["pom.xml", "build.gradle", ".java-version"],
            "java",
            &["-version"],
            Color::White,
            Color::Ansi256(166),
        ),
        "php" => segment_lang(
            icons.php,
            &["composer.json", ".php-version"],
            "php",
            &["--version"],
            Color::White,
            Color::Ansi256(99),
        ),
        "swift" => segment_lang(
            icons.swift,
            &["Package.swift", ".swift-version"],
            "swift",
            &["--version"],
            Color::White,
            Color::Ansi256(166),
        ),
        "dotnet" => segment_lang(
            icons.dotnet,
            &["*.csproj", "*.sln", "global.json"],
            "dotnet",
            &["--version"],
            Color::White,
            Color::Ansi256(99),
        ),
        "aws" => segment_aws(icons),
        "terraform" => segment_terraform(icons),
        "docker_context" => segment_docker_context(icons),
        "os_icon" => Some(segment_os_icon(icons)),
        "user" => segment_user(),
        "host" => segment_host(),
        _ => config
            .prompt
            .custom
            .iter()
            .find(|c| c.name == name)
            .and_then(segment_custom),
    }
}

// ─── Individual Segments ────────────────────────────────────────

fn segment_dir(config: &Config, icons: &Icons) -> Segment {
    let cwd = env::current_dir().unwrap_or_default();
    let home = dirs::home_dir().unwrap_or_default();

    let display = if cwd.starts_with(&home) {
        let relative = cwd.strip_prefix(&home).unwrap_or(&cwd);
        if relative.as_os_str().is_empty() {
            config.prompt.dir.home_symbol.clone()
        } else {
            format!("{}/{}", config.prompt.dir.home_symbol, relative.display())
        }
    } else {
        cwd.display().to_string()
    };

    let text = truncate_path(
        &display,
        config.prompt.dir.truncation_length,
        &config.prompt.dir.truncation_symbol,
    );

    Segment {
        text,
        icon: icons.dir.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(24),
    }
}

fn segment_git(config: &Config, icons: &Icons) -> Option<Segment> {
    let branch = run_cmd("git", &["rev-parse", "--abbrev-ref", "HEAD"])?;

    let mut parts = Vec::new();
    let branch_icon = if icons.git_branch.is_empty() {
        String::new()
    } else {
        format!("{} ", icons.git_branch)
    };
    parts.push(format!("{branch_icon}{branch}"));

    let mut is_dirty = false;

    if config.prompt.git.show_status {
        let status = run_cmd("git", &["status", "--porcelain", "-b"]).unwrap_or_default();
        let mut staged = 0;
        let mut modified = 0;
        let mut untracked = 0;

        for line in status.lines().skip(1) {
            if line.len() < 2 {
                continue;
            }
            let bytes = line.as_bytes();
            let index = bytes[0] as char;
            let worktree = bytes[1] as char;
            if index != ' ' && index != '?' {
                staged += 1;
            }
            if worktree != ' ' && worktree != '?' {
                modified += 1;
            }
            if index == '?' {
                untracked += 1;
            }
        }

        is_dirty = staged > 0 || modified > 0 || untracked > 0;
        if staged > 0 {
            parts.push(format!("{}{staged}", icons.git_staged));
        }
        if modified > 0 {
            parts.push(format!("{}{modified}", icons.git_modified));
        }
        if untracked > 0 {
            parts.push(format!("{}{untracked}", icons.git_untracked));
        }
    }

    if config.prompt.git.show_ahead_behind
        && let Some((ahead, behind)) = get_ahead_behind()
    {
        if ahead > 0 {
            parts.push(format!("{}{ahead}", icons.git_ahead));
        }
        if behind > 0 {
            parts.push(format!("{}{behind}", icons.git_behind));
        }
    }

    if config.prompt.git.show_stash
        && let Some(count) = get_stash_count()
        && count > 0
    {
        parts.push(format!("{}{count}", icons.git_stash));
    }

    let text = parts.join(" ");
    let bg = if is_dirty {
        Color::Ansi256(178)
    } else {
        Color::Ansi256(70)
    };
    Some(Segment {
        text,
        icon: String::new(),
        fg: Color::Black,
        bg,
    })
}

fn segment_status(last_status: i32, icons: &Icons) -> Option<Segment> {
    if last_status == 0 {
        return None;
    }
    Some(Segment {
        text: format!("{} {last_status}", icons.error),
        icon: String::new(),
        fg: Color::White,
        bg: Color::Red,
    })
}

fn segment_duration(duration_ms: u64, icons: &Icons) -> Option<Segment> {
    if duration_ms < 2000 {
        return None;
    }
    let text = if duration_ms >= 60_000 {
        let mins = duration_ms / 60_000;
        let secs = (duration_ms % 60_000) / 1000;
        format!("{mins}m{secs}s")
    } else {
        let secs = duration_ms as f64 / 1000.0;
        format!("{secs:.1}s")
    };
    let icon = if icons.duration.is_empty() {
        String::new()
    } else {
        format!("{} ", icons.duration)
    };
    Some(Segment {
        text: format!("{icon}{text}"),
        icon: String::new(),
        fg: Color::White,
        bg: Color::Ansi256(238),
    })
}

fn segment_time(icons: &Icons) -> Segment {
    let now = chrono::Local::now();
    let icon = if icons.time_icon.is_empty() {
        String::new()
    } else {
        format!("{} ", icons.time_icon)
    };
    Segment {
        text: format!("{icon}{}", now.format("%H:%M:%S")),
        icon: String::new(),
        fg: Color::White,
        bg: Color::Ansi256(236),
    }
}

fn segment_lang(
    icon: &str,
    markers: &[&str],
    cmd: &str,
    args: &[&str],
    fg: Color,
    bg: Color,
) -> Option<Segment> {
    let cwd = env::current_dir().ok()?;
    if !has_marker_file(&cwd, markers) {
        return None;
    }
    let version_text = run_cmd(cmd, args)?;
    let version = extract_version(&version_text);
    Some(Segment {
        text: version,
        icon: icon.to_string(),
        fg,
        bg,
    })
}

// ─── Environment Segments ───────────────────────────────────────

fn segment_virtualenv(icons: &Icons) -> Option<Segment> {
    let venv = env::var("VIRTUAL_ENV")
        .ok()
        .or_else(|| env::var("CONDA_DEFAULT_ENV").ok())?;
    let name = Path::new(&venv).file_name()?.to_str()?.to_string();
    if name == "base" {
        return None;
    }
    Some(Segment {
        text: name,
        icon: icons.virtualenv.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(24),
    })
}

fn segment_kubecontext(icons: &Icons) -> Option<Segment> {
    let ctx = run_cmd("kubectl", &["config", "current-context"])?;
    if ctx.is_empty() {
        return None;
    }
    Some(Segment {
        text: ctx,
        icon: icons.kubernetes.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(21),
    })
}

fn segment_ssh(icons: &Icons) -> Option<Segment> {
    env::var("SSH_CONNECTION").ok()?;
    let user = env::var("USER").unwrap_or_default();
    let host = run_cmd("hostname", &["-s"]).unwrap_or_default();
    Some(Segment {
        text: format!("{user}@{host}"),
        icon: icons.ssh_icon.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(166),
    })
}

fn segment_package(icons: &Icons) -> Option<Segment> {
    let cwd = env::current_dir().ok()?;
    let pkg_path = find_up(&cwd, "package.json")?;
    let content = std::fs::read_to_string(pkg_path).ok()?;
    let name = extract_json_value(&content, "name")?;
    let version = extract_json_value(&content, "version")?;
    Some(Segment {
        text: format!("{name}@{version}"),
        icon: icons.package.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(22),
    })
}

fn segment_jobs(count: usize, icons: &Icons) -> Option<Segment> {
    if count == 0 {
        return None;
    }
    Some(Segment {
        text: count.to_string(),
        icon: icons.jobs.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(238),
    })
}

// ─── Cloud & Infrastructure Segments ────────────────────────────

fn segment_aws(icons: &Icons) -> Option<Segment> {
    let profile = env::var("AWS_PROFILE")
        .or_else(|_| env::var("AWS_DEFAULT_PROFILE"))
        .ok()?;
    Some(Segment {
        text: profile,
        icon: icons.aws.to_string(),
        fg: Color::Black,
        bg: Color::Ansi256(208),
    })
}

fn segment_terraform(icons: &Icons) -> Option<Segment> {
    let cwd = env::current_dir().ok()?;
    if !has_marker_file(&cwd, &["*.tf", ".terraform"]) {
        return None;
    }
    let workspace = run_cmd("terraform", &["workspace", "show"])?;
    if workspace.is_empty() || workspace == "default" {
        return None;
    }
    Some(Segment {
        text: workspace,
        icon: icons.terraform.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(99),
    })
}

fn segment_docker_context(icons: &Icons) -> Option<Segment> {
    let ctx = env::var("DOCKER_CONTEXT")
        .ok()
        .or_else(|| run_cmd("docker", &["context", "show"]))?;
    if ctx.is_empty() || ctx == "default" {
        return None;
    }
    Some(Segment {
        text: ctx,
        icon: icons.docker.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(25),
    })
}

fn segment_os_icon(icons: &Icons) -> Segment {
    Segment {
        text: String::new(),
        icon: icons.os_icon.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(236),
    }
}

fn segment_user() -> Option<Segment> {
    let user = env::var("USER").ok()?;
    if user != "root" && env::var("SSH_CONNECTION").is_err() {
        return None;
    }
    let fg = if user == "root" {
        Color::Yellow
    } else {
        Color::White
    };
    Some(Segment {
        text: user,
        icon: String::new(),
        fg,
        bg: Color::Ansi256(238),
    })
}

fn segment_host() -> Option<Segment> {
    env::var("SSH_CONNECTION").ok()?;
    let host = run_cmd("hostname", &["-s"]).unwrap_or_default();
    if host.is_empty() {
        return None;
    }
    Some(Segment {
        text: host,
        icon: String::new(),
        fg: Color::White,
        bg: Color::Ansi256(24),
    })
}

// ─── Custom Command Segment ─────────────────────────────────────

fn segment_custom(custom: &CustomSegment) -> Option<Segment> {
    if !check_custom_condition(&custom.when) {
        return None;
    }
    let output = Command::new("sh")
        .args(["-c", &custom.command])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some(Segment {
        text,
        icon: custom.icon.clone(),
        fg: parse_color(&custom.fg),
        bg: parse_color(&custom.bg),
    })
}

fn check_custom_condition(when: &str) -> bool {
    if when.is_empty() || when == "always" {
        return true;
    }
    if let Some(var) = when.strip_prefix("env:") {
        return env::var(var).is_ok();
    }
    if let Some(file) = when.strip_prefix("file:") {
        return Path::new(file).exists()
            || env::current_dir()
                .map(|d| d.join(file).exists())
                .unwrap_or(false);
    }
    true
}

// ─── Git Helpers (private) ──────────────────────────────────────

fn get_ahead_behind() -> Option<(usize, usize)> {
    let text = run_cmd(
        "git",
        &["rev-list", "--left-right", "--count", "HEAD...@{upstream}"],
    )?;
    let parts: Vec<&str> = text.split('\t').collect();
    if parts.len() == 2 {
        Some((parts[0].parse().unwrap_or(0), parts[1].parse().unwrap_or(0)))
    } else {
        None
    }
}

fn get_stash_count() -> Option<usize> {
    let text = run_cmd("git", &["stash", "list"])?;
    Some(text.lines().count())
}
