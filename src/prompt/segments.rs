use crate::config::{Config, CustomSegment};
use crate::icons::Icons;
use crate::style::{Color, parse_color};
use std::env;
use std::ffi::OsString;
use std::path::Path;

use super::helpers::*;

pub struct Segment {
    pub text: String,
    pub icon: String,
    pub fg: Color,
    pub bg: Color,
}

/// 利用可能な全セグメント名（TUI の選択肢表示順）。
/// `make_segment` の match アームと1対1で対応させること。
pub const ALL_SEGMENT_NAMES: &[&str] = &[
    "dir",
    "git",
    "status",
    "duration",
    "time",
    "virtualenv",
    "kubecontext",
    "ssh",
    "package",
    "jobs",
    "node",
    "python",
    "rust",
    "go",
    "ruby",
    "java",
    "php",
    "swift",
    "dotnet",
    "aws",
    "gcloud",
    "terraform",
    "docker_context",
    "direnv",
    "nix_shell",
    "load",
    "battery",
    "disk_usage",
    "ram",
    "vi_mode",
    "proxy",
    "os_icon",
    "user",
    "host",
    "cpu_arch",
    "root_indicator",
    "dir_writable",
    "ip",
];

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
        "gcloud" => segment_gcloud(icons),
        "terraform" => segment_terraform(icons),
        "docker_context" => segment_docker_context(icons),
        "direnv" => segment_direnv(icons),
        "nix_shell" => segment_nix_shell(icons),
        "load" => segment_load(icons),
        "battery" => segment_battery(icons),
        "disk_usage" => segment_disk_usage(icons),
        "ram" => segment_ram(icons),
        "vi_mode" => segment_vi_mode(icons),
        "proxy" => segment_proxy(),
        "os_icon" => Some(segment_os_icon(icons)),
        "user" => segment_user(),
        "host" => segment_host(),
        "cpu_arch" => segment_cpu_arch(icons),
        "root_indicator" => segment_root_indicator(icons),
        "dir_writable" => segment_dir_writable(icons),
        "ip" => segment_ip(config, icons),
        _ => config
            .prompt
            .custom
            .iter()
            .find(|c| c.name == name)
            .and_then(segment_custom),
    }
}

// ─── 個別セグメント ─────────────────────────────────────────────

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
    // `git status` 1 回でブランチ・追跡情報・ファイル状態をまとめて取得する。
    let status = run_cmd("git", &["status", "--porcelain", "-b"])?;
    let mut lines = status.lines();

    // ブランチ行の形式: "## branch...origin/branch [ahead N, behind M]"
    let header = lines.next()?;
    let branch = parse_git_branch(header);
    if branch.is_empty() {
        return None;
    }

    let mut parts = Vec::new();
    let branch_icon = if icons.git_branch.is_empty() {
        String::new()
    } else {
        format!("{} ", icons.git_branch)
    };
    parts.push(format!("{branch_icon}{branch}"));

    let mut is_dirty = false;
    let mut staged = 0;
    let mut modified = 0;
    let mut untracked = 0;

    if config.prompt.git.show_status {
        for line in lines {
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

    // 追加の `git rev-list` を呼ばず、ヘッダーから ahead/behind を読む。
    if config.prompt.git.show_ahead_behind {
        let (ahead, behind) = parse_ahead_behind(header);
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

// ─── 環境セグメント ─────────────────────────────────────────────

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

// ─── クラウド・インフラ系セグメント ───────────────────────────

fn segment_aws(icons: &Icons) -> Option<Segment> {
    let profile = pick_aws_profile(|key| env::var(key).ok())?;
    Some(Segment {
        text: profile,
        icon: icons.aws.to_string(),
        fg: Color::Black,
        bg: Color::Ansi256(208),
    })
}

/// AWS プロファイルを決定する純粋関数。以下の優先順で評価する:
/// `AWS_SSO_PROFILE` > `AWS_VAULT` > `AWSUME_PROFILE` > `AWS_PROFILE` > `AWS_DEFAULT_PROFILE`。
/// 環境変数を `lookup` 経由で受け取ることでテスト可能にしてある。
fn pick_aws_profile<F>(lookup: F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    const KEYS: &[&str] = &[
        "AWS_SSO_PROFILE",
        "AWS_VAULT",
        "AWSUME_PROFILE",
        "AWS_PROFILE",
        "AWS_DEFAULT_PROFILE",
    ];
    for key in KEYS {
        if let Some(v) = lookup(key)
            && !v.is_empty()
        {
            return Some(v);
        }
    }
    None
}

const TERRAFORM_MARKERS: &[&str] = &["*.tf", "*.tf.json", "*.tofu", "*.tofu.json", ".terraform"];

fn segment_terraform(icons: &Icons) -> Option<Segment> {
    let cwd = env::current_dir().ok()?;
    if !is_terraform_project_dir(&cwd) {
        return None;
    }
    // OpenTofu (`tofu`) も terraform 互換のため、`terraform` がなければ `tofu` を試す。
    let workspace = run_cmd("terraform", &["workspace", "show"])
        .or_else(|| run_cmd("tofu", &["workspace", "show"]))?;
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

fn is_terraform_project_dir(cwd: &Path) -> bool {
    has_marker_file(cwd, TERRAFORM_MARKERS)
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

/// gcloud の active config からプロジェクト ID を取り出す。
/// `~/.config/gcloud/active_config` で設定名を確認し、
/// `~/.config/gcloud/configurations/config_<name>` の `project = ` 行を読む。
/// `gcloud` コマンドを呼び出さずにファイルベースで解決するためプロンプト描画コストが低い。
fn segment_gcloud(icons: &Icons) -> Option<Segment> {
    let base = gcloud_config_base(env::var_os("CLOUDSDK_CONFIG"))?;
    let active = std::fs::read_to_string(base.join("active_config")).ok()?;
    let config_name = active.trim();
    if config_name.is_empty() {
        return None;
    }
    let config_path = base
        .join("configurations")
        .join(format!("config_{config_name}"));
    let content = std::fs::read_to_string(&config_path).ok()?;
    let project = parse_gcloud_project(&content)?;
    Some(Segment {
        text: project,
        icon: icons.gcloud.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(33),
    })
}

fn gcloud_config_base(cloudsdk_config: Option<OsString>) -> Option<std::path::PathBuf> {
    if let Some(value) = cloudsdk_config
        && !value.is_empty()
    {
        return Some(std::path::PathBuf::from(value));
    }

    dirs::home_dir().map(|h| h.join(".config/gcloud"))
}

/// `direnv` で読み込まれた環境（`DIRENV_DIR` 設定時）を表示する。
fn segment_direnv(icons: &Icons) -> Option<Segment> {
    let dir = env::var("DIRENV_DIR").ok()?;
    build_direnv_segment(&dir, icons)
}

/// `DIRENV_DIR` の値からセグメントを組み立てる純粋関数。
/// 値は通常 `-/path/to/dir` の形のため、先頭の `-` を除去してから
/// ベース名を取り出す。env を読まないためテストしやすい。
fn build_direnv_segment(dir: &str, icons: &Icons) -> Option<Segment> {
    if dir.is_empty() {
        return None;
    }
    let path_str = dir.strip_prefix('-').unwrap_or(dir);
    let name = Path::new(path_str)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path_str)
        .to_string();
    Some(Segment {
        text: name,
        icon: icons.direnv.to_string(),
        fg: Color::Black,
        bg: Color::Yellow,
    })
}

/// Nix shell 環境（`IN_NIX_SHELL=pure|impure`）を表示する。
fn segment_nix_shell(icons: &Icons) -> Option<Segment> {
    let kind = env::var("IN_NIX_SHELL").ok()?;
    build_nix_shell_segment(&kind, icons)
}

/// `IN_NIX_SHELL` の値からセグメントを組み立てる純粋関数。env を読まない。
fn build_nix_shell_segment(kind: &str, icons: &Icons) -> Option<Segment> {
    if kind != "pure" && kind != "impure" {
        return None;
    }
    Some(Segment {
        text: kind.to_string(),
        icon: icons.nix_shell.to_string(),
        fg: Color::White,
        bg: Color::Ansi256(24),
    })
}

// ─── システムリソース系セグメント ──────────────────────────────

/// 負荷平均（loadavg）のしきい値。
/// 警告 70%、危険 90% で評価する。
const LOAD_WARNING_PCT: f64 = 70.0;
const LOAD_CRITICAL_PCT: f64 = 90.0;

/// 1 分ロードアベレージを表示する。
/// Linux は `/proc/loadavg` を直接読み、macOS/BSD は `sysctl` を使う。
/// 外部コマンド呼び出しは Linux なし、macOS のみ `sysctl` を 1 回。
fn segment_load(icons: &Icons) -> Option<Segment> {
    let load = read_load_average()?;
    let cpus = num_cpus();
    let pct = if cpus > 0 {
        100.0 * load / cpus as f64
    } else {
        0.0
    };
    let bg = if pct > LOAD_CRITICAL_PCT {
        Color::Red
    } else if pct > LOAD_WARNING_PCT {
        Color::Yellow
    } else {
        Color::Ansi256(22)
    };
    Some(Segment {
        text: format!("{load:.2}"),
        icon: icons.load.to_string(),
        fg: Color::White,
        bg,
    })
}

/// 1 分ロードアベレージを取得する。
/// Linux: `/proc/loadavg` の先頭フィールドを読む。
/// macOS/BSD: `sysctl -n vm.loadavg` から `{ a b c }` 形式の最初の値を取り出す。
fn read_load_average() -> Option<f64> {
    if cfg!(target_os = "linux") {
        let content = std::fs::read_to_string("/proc/loadavg").ok()?;
        return content.split_whitespace().next()?.parse().ok();
    }
    // macOS/BSD は sysctl 経由
    let out = run_cmd("sysctl", &["-n", "vm.loadavg"])?;
    parse_sysctl_loadavg(&out)
}

/// `{ 1.23 4.56 7.89 }` 形式の文字列から先頭の値を抽出する。
/// 先頭末尾の空白に対応するため `trim()` を先に行う。
fn parse_sysctl_loadavg(s: &str) -> Option<f64> {
    s.trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

/// 論理 CPU 数を取得する。失敗時は `1` で代用。
fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

/// バッテリー残量を表示する。
/// macOS: `pmset -g batt` を解析、Linux: `/sys/class/power_supply/BAT*/` を読む。
fn segment_battery(icons: &Icons) -> Option<Segment> {
    let info = read_battery_info()?;
    let icon = match info.state {
        BatteryState::Charging => icons.battery_charging,
        BatteryState::Full => icons.battery_full,
        BatteryState::Discharging if info.percent < 20 => icons.battery_low,
        _ => icons.battery_full,
    };
    let bg = match info.state {
        BatteryState::Charging => Color::Ansi256(28),
        BatteryState::Full => Color::Ansi256(22),
        BatteryState::Discharging if info.percent < 20 => Color::Red,
        BatteryState::Discharging if info.percent < 40 => Color::Yellow,
        _ => Color::Ansi256(238),
    };
    let fg = if matches!(bg, Color::Yellow) {
        Color::Black
    } else {
        Color::White
    };
    Some(Segment {
        text: format!("{}%", info.percent),
        icon: icon.to_string(),
        fg,
        bg,
    })
}

#[derive(Debug, PartialEq, Eq)]
enum BatteryState {
    Charging,
    Discharging,
    Full,
}

#[derive(Debug)]
struct BatteryInfo {
    percent: u32,
    state: BatteryState,
}

fn read_battery_info() -> Option<BatteryInfo> {
    if cfg!(target_os = "macos") {
        let out = run_cmd("pmset", &["-g", "batt"])?;
        return parse_pmset_battery(&out);
    }
    if cfg!(target_os = "linux") {
        return read_linux_battery();
    }
    None
}

/// `pmset -g batt` の出力からバッテリー情報を取り出す。
/// 例: " -InternalBattery-0 (id=...)\t100%; charged; 0:00 remaining present: true"
fn parse_pmset_battery(out: &str) -> Option<BatteryInfo> {
    let line = out.lines().find(|l| l.contains("InternalBattery"))?;
    // `%` を含まない行は異常出力扱いで無視する（id 内の数字を誤認しないため）。
    if !line.contains('%') {
        return None;
    }
    let percent = line
        .split('%')
        .next()?
        .rsplit(|c: char| !c.is_ascii_digit())
        .next()?
        .parse::<u32>()
        .ok()?;
    // 判定順に注意: "discharging" は "charging" を、"not charging" は "charging" を
    // 部分文字列に含むため、より specific なものから先に判定する。
    let state = if line.contains("charged") || line.contains("finishing charge") {
        BatteryState::Full
    } else if line.contains("discharging") {
        BatteryState::Discharging
    } else if line.contains("not charging") {
        // AC 接続・充電停止 (最適化充電/充電上限での保持) 状態。充電中ではない。
        BatteryState::Full
    } else {
        BatteryState::Charging
    };
    Some(BatteryInfo { percent, state })
}

/// Linux の `/sys/class/power_supply/` 配下からバッテリー情報を読む。
fn read_linux_battery() -> Option<BatteryInfo> {
    read_linux_battery_from(Path::new("/sys/class/power_supply"))
}

fn read_linux_battery_from(base: &Path) -> Option<BatteryInfo> {
    let mut entries: Vec<_> = std::fs::read_dir(base).ok()?.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("BAT") {
            continue;
        }
        let path = entry.path();
        let capacity = match std::fs::read_to_string(path.join("capacity"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok())
        {
            Some(capacity) => capacity,
            None => continue,
        };
        let status = match std::fs::read_to_string(path.join("status")) {
            Ok(status) => status.trim().to_string(),
            Err(_) => continue,
        };
        let state = match status.as_str() {
            "Charging" => BatteryState::Charging,
            "Full" | "Not charging" => BatteryState::Full,
            _ => BatteryState::Discharging,
        };
        return Some(BatteryInfo {
            percent: capacity,
            state,
        });
    }
    None
}

/// カレントディレクトリの属するファイルシステムの使用率を表示する。
fn segment_disk_usage(icons: &Icons) -> Option<Segment> {
    let cwd = env::current_dir().ok()?;
    let out = run_cmd("df", &["-P", cwd.to_str()?])?;
    let pct = parse_df_used_pct(&out)?;
    let (fg, bg) = if pct >= 90 {
        (Color::White, Color::Red)
    } else if pct >= 75 {
        (Color::Black, Color::Yellow)
    } else {
        return None;
    };
    Some(Segment {
        text: format!("{pct}%"),
        icon: icons.disk.to_string(),
        fg,
        bg,
    })
}

/// `df -P <path>` の 2 行目から使用率パーセントを抽出する。
/// Filesystem 名に空白を含む場合 (macOS の `map auto_home` 等) に列位置がずれるため、
/// 固定位置ではなく `%` 終端のフィールドを探す。
fn parse_df_used_pct(out: &str) -> Option<u32> {
    let line = out.lines().nth(1)?;
    line.split_whitespace()
        .find_map(|f| f.strip_suffix('%')?.parse::<u32>().ok())
}

/// メモリ使用率を表示する。
/// macOS: `vm_stat`、Linux: `/proc/meminfo` を使う。
fn segment_ram(icons: &Icons) -> Option<Segment> {
    let used_pct = read_ram_used_pct()?;
    let (fg, bg) = if used_pct >= 90 {
        (Color::White, Color::Red)
    } else if used_pct >= 75 {
        (Color::Black, Color::Yellow)
    } else {
        return None;
    };
    Some(Segment {
        text: format!("{used_pct}%"),
        icon: icons.ram.to_string(),
        fg,
        bg,
    })
}

fn read_ram_used_pct() -> Option<u32> {
    if cfg!(target_os = "linux") {
        let content = std::fs::read_to_string("/proc/meminfo").ok()?;
        return parse_meminfo_used_pct(&content);
    }
    if cfg!(target_os = "macos") {
        let vm = run_cmd("vm_stat", &[])?;
        let total_bytes = run_cmd("sysctl", &["-n", "hw.memsize"])?
            .trim()
            .parse::<u64>()
            .ok()?;
        return parse_vm_stat_used_pct(&vm, total_bytes);
    }
    None
}

/// `/proc/meminfo` から `(MemTotal-MemAvailable)/MemTotal*100` を計算する。
fn parse_meminfo_used_pct(content: &str) -> Option<u32> {
    let mut total = 0u64;
    let mut available = 0u64;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            total = rest.split_whitespace().next()?.parse().ok()?;
        } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
            available = rest.split_whitespace().next()?.parse().ok()?;
        }
        if total > 0 && available > 0 {
            break;
        }
    }
    if total == 0 {
        return None;
    }
    let used = total.saturating_sub(available);
    Some(((used * 100) / total) as u32)
}

/// `vm_stat` 出力から使用率を計算する。`pages free + inactive + speculative` を空きとみなす。
/// `purgeable` は `inactive` のサブセットになりうるため重複加算を避ける。
fn parse_vm_stat_used_pct(vm: &str, total_bytes: u64) -> Option<u32> {
    let page_size = vm
        .lines()
        .next()?
        .split_whitespace()
        .find_map(|w| w.parse::<u64>().ok())
        .unwrap_or(4096);
    let mut free_pages = 0u64;
    for line in vm.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("pages free")
            || lower.starts_with("pages inactive")
            || lower.starts_with("pages speculative")
        {
            let value = line
                .split(':')
                .nth(1)?
                .trim()
                .trim_end_matches('.')
                .parse::<u64>()
                .ok()?;
            free_pages = free_pages.saturating_add(value);
        }
    }
    let free_bytes = free_pages.saturating_mul(page_size);
    if total_bytes == 0 {
        return None;
    }
    let used_bytes = total_bytes.saturating_sub(free_bytes);
    Some(((used_bytes * 100) / total_bytes) as u32)
}

/// vi モードを表示する。zsh 側から `ZSH_TURBO_VI_MODE` 環境変数で
/// `insert`/`normal`/`visual` を渡してもらう。zsh の `KEYMAP` から
/// `viins`/`vicmd`/`visual` を判別して設定する想定。
fn segment_vi_mode(icons: &Icons) -> Option<Segment> {
    let mode = env::var("ZSH_TURBO_VI_MODE").ok()?;
    build_vi_mode_segment(&mode, icons)
}

/// HTTP/HTTPS/ALL プロキシ環境変数が設定されている場合に "proxy" を表示する。
/// プロキシ設定を表示する。`http_proxy` / `HTTP_PROXY` の両方を見る。
fn segment_proxy() -> Option<Segment> {
    const KEYS: &[&str] = &[
        "http_proxy",
        "HTTP_PROXY",
        "https_proxy",
        "HTTPS_PROXY",
        "all_proxy",
        "ALL_PROXY",
    ];
    for key in KEYS {
        if let Ok(v) = env::var(key)
            && !v.is_empty()
        {
            return Some(Segment {
                text: "proxy".into(),
                icon: String::new(),
                fg: Color::White,
                bg: Color::Ansi256(94),
            });
        }
    }
    None
}

/// vi モード文字列からセグメントを組み立てる純粋関数。
fn build_vi_mode_segment(mode: &str, icons: &Icons) -> Option<Segment> {
    let (text, icon, fg, bg) = match mode {
        "insert" | "viins" | "main" => (
            icons.vi_insert.to_string(),
            String::new(),
            Color::Black,
            Color::Ansi256(34),
        ),
        "normal" | "vicmd" => (
            icons.vi_normal.to_string(),
            String::new(),
            Color::Black,
            Color::Ansi256(214),
        ),
        "visual" | "vivis" | "vivli" => (
            icons.vi_visual.to_string(),
            String::new(),
            Color::White,
            Color::Ansi256(99),
        ),
        _ => return None,
    };
    Some(Segment { text, icon, fg, bg })
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

/// CPU アーキテクチャを表示する。
/// Linux では `/proc/sys/kernel/arch` を、macOS では `uname -m` を読み取り、
/// 外部コマンド呼び出しを最小化する。判定不能な場合はセグメントを出さない。
fn segment_cpu_arch(icons: &Icons) -> Option<Segment> {
    let text = read_cpu_arch()?;
    Some(Segment {
        text,
        icon: icons.cpu_arch.to_string(),
        fg: Color::Black,
        bg: Color::Ansi256(178),
    })
}

fn read_cpu_arch() -> Option<String> {
    if let Ok(v) = std::fs::read_to_string("/proc/sys/kernel/arch") {
        let s = v.trim();
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    run_cmd("uname", &["-m"])
}

/// root ユーザーで実行中のときだけ目印を表示する。
/// 一般ユーザーでは何も表示しない。
fn segment_root_indicator(icons: &Icons) -> Option<Segment> {
    if !is_root_user() {
        return None;
    }
    Some(Segment {
        text: String::new(),
        icon: icons.root.to_string(),
        fg: Color::Yellow,
        bg: Color::Ansi256(236),
    })
}

fn is_root_user() -> bool {
    is_root_user_with(env::var("USER").ok().as_deref(), || run_cmd("id", &["-u"]))
}

/// テストしやすいよう環境変数とフォールバックを注入可能にした純粋ロジック。
fn is_root_user_with<F>(user_env: Option<&str>, fallback: F) -> bool
where
    F: FnOnce() -> Option<String>,
{
    if let Some(name) = user_env {
        return name == "root";
    }
    matches!(fallback().as_deref(), Some("0"))
}

/// 書き込み不可なディレクトリにいるときだけ警告を出す。
fn segment_dir_writable(icons: &Icons) -> Option<Segment> {
    let cwd = env::current_dir().ok()?;
    if is_writable_dir(&cwd) {
        return None;
    }
    Some(Segment {
        text: String::new(),
        icon: icons.lock.to_string(),
        fg: Color::Yellow,
        bg: Color::Ansi256(160),
    })
}

#[cfg(unix)]
fn is_writable_dir(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes());
    let Ok(c_path) = c_path else {
        return true; // パスに NUL が含まれるなど判定不能な場合は警告を出さない。
    };
    // 安全性: C 文字列ポインタは生存期間内であり、`access(2)` は副作用がない。
    unsafe { libc_access(c_path.as_ptr(), 0o2) == 0 }
}

#[cfg(not(unix))]
fn is_writable_dir(_path: &Path) -> bool {
    true
}

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "access"]
    fn libc_access(pathname: *const std::ffi::c_char, mode: std::ffi::c_int) -> std::ffi::c_int;
}

/// ローカル IP アドレスを表示する。
/// `ZSH_TURBO_IP_INTERFACE` で対象インターフェースを限定できる。
/// 未指定時は macOS で `en0`、Linux でルーティングスコープ `global` の
/// 最初の IPv4 アドレスを採用する。
fn segment_ip(config: &Config, icons: &Icons) -> Option<Segment> {
    let iface = env::var("ZSH_TURBO_IP_INTERFACE").ok();
    let iface = iface.as_deref().unwrap_or(&config.prompt.ip_interface);
    let ip = find_local_ip((!iface.is_empty()).then_some(iface))?;
    Some(Segment {
        text: ip,
        icon: icons.network.to_string(),
        fg: Color::Black,
        bg: Color::Ansi256(38),
    })
}

#[cfg(target_os = "macos")]
fn find_local_ip(iface: Option<&str>) -> Option<String> {
    let target = iface.unwrap_or("en0");
    let output = run_cmd("ipconfig", &["getifaddr", target])?;
    if output.is_empty() {
        return None;
    }
    Some(output)
}

#[cfg(not(target_os = "macos"))]
fn find_local_ip(iface: Option<&str>) -> Option<String> {
    let args: Vec<&str> = if let Some(name) = iface {
        vec!["-4", "-o", "addr", "show", "dev", name]
    } else {
        vec!["-4", "-o", "addr", "show", "scope", "global"]
    };
    let output = run_cmd("ip", &args)?;
    parse_ip_addr(&output)
}

#[cfg(not(target_os = "macos"))]
fn parse_ip_addr(text: &str) -> Option<String> {
    for line in text.lines() {
        let mut iter = line.split_whitespace();
        while let Some(token) = iter.next() {
            if token == "inet"
                && let Some(addr) = iter.next()
            {
                let ip = addr.split('/').next()?;
                if !ip.is_empty() {
                    return Some(ip.to_string());
                }
            }
        }
    }
    None
}

// ─── カスタムコマンドセグメント ───────────────────────────────

fn segment_custom(custom: &CustomSegment) -> Option<Segment> {
    if !check_custom_condition(&custom.when) {
        return None;
    }
    let text = run_cmd_stdout("sh", &["-c", &custom.command])?;
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

// ─── Git 用ヘルパー（非公開） ─────────────────────────────────

/// `git status -b` のヘッダー行からブランチ名を取り出す。
/// "## main...origin/main [ahead 1, behind 2]" → "main"
/// "## main" → "main"
/// "## HEAD (no branch)" → "HEAD"
/// "## No commits yet on main" → "main" (初期化直後の unborn ブランチ)
fn parse_git_branch(header: &str) -> String {
    let rest = header.strip_prefix("## ").unwrap_or(header);
    // 初期化直後でまだコミットがない場合、ブランチ名は末尾に出る。
    if let Some(branch_part) = rest.strip_prefix("No commits yet on ") {
        return branch_part
            .split_whitespace()
            .next()
            .unwrap_or(branch_part)
            .to_string();
    }
    // `...` があれば追跡先より前まで、なければ行全体を使う。
    let branch = if let Some(idx) = rest.find("...") {
        &rest[..idx]
    } else {
        // " [ahead N]" のような付加情報は除外する。
        rest.split_whitespace().next().unwrap_or(rest)
    };
    branch.to_string()
}

/// `git status -b` のヘッダー行から ahead/behind 件数を取り出す。
/// "## main...origin/main [ahead 1, behind 2]" → (1, 2)
fn parse_ahead_behind(header: &str) -> (usize, usize) {
    let mut ahead = 0;
    let mut behind = 0;
    if let Some(bracket_start) = header.find('[')
        && let Some(bracket_end) = header[bracket_start..].find(']')
    {
        let info = &header[bracket_start + 1..bracket_start + bracket_end];
        for part in info.split(',') {
            let part = part.trim();
            if let Some(n) = part.strip_prefix("ahead ") {
                ahead = n.trim().parse().unwrap_or(0);
            } else if let Some(n) = part.strip_prefix("behind ") {
                behind = n.trim().parse().unwrap_or(0);
            }
        }
    }
    (ahead, behind)
}

fn get_stash_count() -> Option<usize> {
    let text = run_cmd("git", &["stash", "list"])?;
    Some(text.lines().count())
}

/// gcloud の configuration ファイルから `project` の値を抽出する。
/// 形式は INI ライクで、`[core]` セクション直下の `project = my-project` 行を想定。
fn parse_gcloud_project(content: &str) -> Option<String> {
    let mut in_core = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(section) = line.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            in_core = section.eq_ignore_ascii_case("core");
            continue;
        }
        if !in_core {
            continue;
        }
        if let Some((key, value)) = line.split_once('=')
            && key.trim().eq_ignore_ascii_case("project")
        {
            let v = value.trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ALL_SEGMENT_NAMES のテスト ───────────────────────────────

    #[test]
    fn all_segment_names_に重複がない() {
        let mut seen = std::collections::HashSet::new();
        for name in ALL_SEGMENT_NAMES {
            assert!(seen.insert(name), "セグメント名が重複している: {name}");
        }
    }

    #[test]
    fn all_segment_names_はカスタムセグメント分岐に落ちない() {
        // 組み込みセグメント名が make_segment の match アームから漏れると、
        // `_`（カスタムセグメント検索）に落ちる。全名に sentinel を返す custom を
        // 仕込み、その出力が現れないことで ALL_SEGMENT_NAMES と match アームの
        // 同期を検証する。環境依存で Some/None が揺れるのは正常。
        let mut config = Config::default();
        config.prompt.custom = ALL_SEGMENT_NAMES
            .iter()
            .map(|name| CustomSegment {
                name: (*name).to_string(),
                command: "echo sentinel-not-builtin".into(),
                icon: String::new(),
                fg: String::new(),
                bg: String::new(),
                when: "always".into(),
            })
            .collect();
        let icons = Icons::for_level(crate::icons::FontLevel::Ascii);
        for name in ALL_SEGMENT_NAMES {
            if let Some(seg) = make_segment(name, &config, &icons, 0, 0, 0) {
                assert_ne!(
                    seg.text, "sentinel-not-builtin",
                    "組み込みセグメント {name} がカスタム分岐に落ちている"
                );
            }
        }
    }

    // ── parse_git_branch のテスト ────────────────────────────────

    #[test]
    fn parse_git_branch_with_tracking() {
        assert_eq!(parse_git_branch("## main...origin/main [ahead 1]"), "main");
    }

    #[test]
    fn parse_git_branch_no_branch() {
        assert_eq!(parse_git_branch("## HEAD (no branch)"), "HEAD");
    }

    #[test]
    fn parse_git_branch_simple() {
        assert_eq!(parse_git_branch("## develop"), "develop");
    }

    #[test]
    fn parse_git_branch_with_tracking_no_divergence() {
        assert_eq!(
            parse_git_branch("## feature/foo...origin/feature/foo"),
            "feature/foo"
        );
    }

    #[test]
    fn parse_git_branch_bare_header() {
        // `## ` がなくても壊れず、そのままブランチ名を返す。
        assert_eq!(parse_git_branch("main"), "main");
    }

    // ── parse_ahead_behind のテスト ──────────────────────────────

    #[test]
    fn parse_ahead_behind_both() {
        assert_eq!(
            parse_ahead_behind("## main...origin/main [ahead 3, behind 2]"),
            (3, 2)
        );
    }

    #[test]
    fn parse_ahead_behind_none() {
        assert_eq!(parse_ahead_behind("## main...origin/main"), (0, 0));
    }

    #[test]
    fn parse_ahead_behind_ahead_only() {
        assert_eq!(
            parse_ahead_behind("## main...origin/main [ahead 1]"),
            (1, 0)
        );
    }

    #[test]
    fn parse_ahead_behind_behind_only() {
        assert_eq!(
            parse_ahead_behind("## main...origin/main [behind 5]"),
            (0, 5)
        );
    }

    #[test]
    fn parse_ahead_behind_no_brackets() {
        assert_eq!(parse_ahead_behind("## develop"), (0, 0));
    }

    // ── check_custom_condition のテスト ─────────────────────────

    #[test]
    fn check_custom_condition_空文字列はtrue() {
        assert!(check_custom_condition(""));
    }

    #[test]
    fn check_custom_condition_alwaysはtrue() {
        assert!(check_custom_condition("always"));
    }

    #[test]
    fn check_custom_condition_envプレフィックスで存在する変数() {
        // PATH は常に設定されている
        assert!(check_custom_condition("env:PATH"));
    }

    #[test]
    fn check_custom_condition_envプレフィックスで存在しない変数() {
        assert!(!check_custom_condition("env:ZSH_TURBO_NONEXISTENT_VAR_XYZ"));
    }

    #[test]
    fn check_custom_condition_fileプレフィックスで存在しないファイル() {
        assert!(!check_custom_condition("file:/no_such_file_xyz_999"));
    }

    #[test]
    fn check_custom_condition_不明な条件はtrue() {
        // "always" でも "env:" でも "file:" でもない値は true を返す
        assert!(check_custom_condition("unknown_condition"));
    }

    // ── Terraform/OpenTofu プロジェクト判定のテスト ─────────────

    #[test]
    fn is_terraform_project_dir_は_tf_jsonを検出する() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tf.json"), "{}").unwrap();

        assert!(is_terraform_project_dir(dir.path()));
    }

    #[test]
    fn is_terraform_project_dir_は_tofuファイルを検出する() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tofu"), "").unwrap();

        assert!(is_terraform_project_dir(dir.path()));
    }

    #[test]
    fn is_terraform_project_dir_は_tofu_jsonを検出する() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("main.tofu.json"), "{}").unwrap();

        assert!(is_terraform_project_dir(dir.path()));
    }

    // ── segment_custom のテスト ─────────────────────────────────

    #[test]
    fn segment_custom_標準出力を表示する() {
        let custom = CustomSegment {
            name: "test".into(),
            command: "printf 'ok\\n'".into(),
            icon: "*".into(),
            fg: "white".into(),
            bg: "24".into(),
            when: "always".into(),
        };

        let seg = segment_custom(&custom).expect("stdout があればセグメントを返す");
        assert_eq!(seg.text, "ok");
        assert_eq!(seg.icon, "*");
    }

    #[test]
    fn segment_custom_標準エラーだけならnone() {
        let custom = CustomSegment {
            name: "test".into(),
            command: "printf 'hidden\\n' >&2".into(),
            icon: String::new(),
            fg: "white".into(),
            bg: "24".into(),
            when: "always".into(),
        };

        assert!(segment_custom(&custom).is_none());
    }

    #[test]
    fn segment_custom_タイムアウトでnone() {
        let custom = CustomSegment {
            name: "test".into(),
            command: "sleep 10".into(),
            icon: String::new(),
            fg: "white".into(),
            bg: "24".into(),
            when: "always".into(),
        };
        let start = std::time::Instant::now();

        assert!(segment_custom(&custom).is_none());
        assert!(
            start.elapsed() < std::time::Duration::from_secs(2),
            "カスタムセグメントがタイムアウトせずに待ち続けている"
        );
    }

    // ── parse_git_branch: 追加の境界テスト ─────────────────────

    #[test]
    fn parse_git_branch_detached_head() {
        // Detached HEAD 状態
        assert_eq!(parse_git_branch("## HEAD (no branch)"), "HEAD");
    }

    #[test]
    fn parse_git_branch_スラッシュ付きブランチ() {
        assert_eq!(
            parse_git_branch("## feature/my-branch...origin/feature/my-branch [ahead 2]"),
            "feature/my-branch"
        );
    }

    #[test]
    fn parse_git_branch_unborn_branch() {
        // git init 直後 (まだコミットがない) の状態
        assert_eq!(parse_git_branch("## No commits yet on main"), "main");
    }

    #[test]
    fn parse_git_branch_unborn_branch_with_slash() {
        // unborn ブランチでもスラッシュ付き名を扱える
        assert_eq!(
            parse_git_branch("## No commits yet on feature/foo"),
            "feature/foo"
        );
    }

    // ── parse_ahead_behind: 追加の境界テスト ───────────────────

    #[test]
    fn parse_ahead_behind_不正な数値はゼロ() {
        assert_eq!(
            parse_ahead_behind("## main...origin/main [ahead abc, behind xyz]"),
            (0, 0)
        );
    }

    #[test]
    fn parse_ahead_behind_空ブラケット() {
        assert_eq!(parse_ahead_behind("## main...origin/main []"), (0, 0));
    }

    // ── segment_duration のフォーマットテスト ─────────────────────

    #[test]
    fn segment_duration_2秒未満はnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(segment_duration(0, &icons).is_none());
        assert!(segment_duration(1999, &icons).is_none());
    }

    #[test]
    fn segment_duration_秒表示() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_duration(2000, &icons).unwrap();
        assert!(
            seg.text.contains("2.0s"),
            "2秒は '2.0s' と表示されるべき: {}",
            seg.text
        );
    }

    #[test]
    fn segment_duration_小数点表示() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_duration(5500, &icons).unwrap();
        assert!(
            seg.text.contains("5.5s"),
            "5.5秒は '5.5s' と表示されるべき: {}",
            seg.text
        );
    }

    #[test]
    fn segment_duration_分表示() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_duration(60_000, &icons).unwrap();
        assert!(
            seg.text.contains("1m0s"),
            "60秒は '1m0s' と表示されるべき: {}",
            seg.text
        );
    }

    #[test]
    fn segment_duration_分秒表示() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_duration(90_000, &icons).unwrap();
        assert!(
            seg.text.contains("1m30s"),
            "90秒は '1m30s' と表示されるべき: {}",
            seg.text
        );
    }

    #[test]
    fn segment_duration_大きな値() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_duration(3_661_000, &icons).unwrap();
        assert!(
            seg.text.contains("61m1s"),
            "3661秒は '61m1s' と表示されるべき: {}",
            seg.text
        );
    }

    // ── segment_status のテスト ──────────────────────────────────

    #[test]
    fn segment_status_成功時はnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(segment_status(0, &icons).is_none());
    }

    #[test]
    fn segment_status_失敗時はエラーコードを含む() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_status(1, &icons).unwrap();
        assert!(
            seg.text.contains("1"),
            "終了コード 1 が含まれるべき: {}",
            seg.text
        );
    }

    #[test]
    fn segment_status_シグナル終了() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_status(130, &icons).unwrap();
        assert!(
            seg.text.contains("130"),
            "終了コード 130 が含まれるべき: {}",
            seg.text
        );
    }

    // ── segment_jobs のテスト ────────────────────────────────────

    #[test]
    fn segment_jobs_ゼロはnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(segment_jobs(0, &icons).is_none());
    }

    #[test]
    fn segment_jobs_正の値はジョブ数を含む() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = segment_jobs(3, &icons).unwrap();
        assert_eq!(seg.text, "3");
    }

    // ── gcloud_config_base のテスト ──────────────────────────────

    #[test]
    fn gcloud_config_base_指定値を優先する() {
        let base = gcloud_config_base(Some(OsString::from("/tmp/gcloud-config"))).unwrap();
        assert_eq!(base, std::path::PathBuf::from("/tmp/gcloud-config"));
    }

    #[test]
    fn gcloud_config_base_空文字列は_defaultへフォールバックする() {
        let base = gcloud_config_base(Some(OsString::new())).unwrap();
        assert!(
            base.ends_with(".config/gcloud"),
            "空の CLOUDSDK_CONFIG は ~/.config/gcloud へフォールバックする必要がある: {base:?}"
        );
        assert!(
            !base.as_os_str().is_empty(),
            "空の CLOUDSDK_CONFIG から空パスを採用してはならない"
        );
    }

    // ── parse_gcloud_project のテスト ────────────────────────────

    #[test]
    fn parse_gcloud_project_coreセクションのprojectを取り出す() {
        let content = "[core]\nproject = my-project\naccount = me@example.com\n";
        assert_eq!(
            parse_gcloud_project(content),
            Some("my-project".to_string())
        );
    }

    #[test]
    fn parse_gcloud_project_別セクションのprojectは無視() {
        // [compute] セクションの project は無視され、[core] のものだけ採用される。
        let content = "[compute]\nproject = compute-only\n[core]\nproject = real-project\n";
        assert_eq!(
            parse_gcloud_project(content),
            Some("real-project".to_string())
        );
    }

    #[test]
    fn parse_gcloud_project_コメント行を無視() {
        let content = "# active project\n[core]\n; another comment\nproject = with-comments\n";
        assert_eq!(
            parse_gcloud_project(content),
            Some("with-comments".to_string())
        );
    }

    #[test]
    fn parse_gcloud_project_セクションがなければnone() {
        // [core] セクションが存在しないので None を返す。
        let content = "project = floating\n";
        assert!(parse_gcloud_project(content).is_none());
    }

    #[test]
    fn parse_gcloud_project_project未指定はnone() {
        let content = "[core]\naccount = me@example.com\n";
        assert!(parse_gcloud_project(content).is_none());
    }

    #[test]
    fn parse_gcloud_project_空値はnone() {
        let content = "[core]\nproject =\n";
        assert!(parse_gcloud_project(content).is_none());
    }

    #[test]
    fn parse_gcloud_project_セクション名は大文字小文字を区別しない() {
        let content = "[Core]\nProject = case-mix\n";
        assert_eq!(parse_gcloud_project(content), Some("case-mix".to_string()));
    }

    // ── pick_aws_profile のテスト ────────────────────────────────

    #[test]
    fn pick_aws_profile_未設定はnone() {
        assert!(pick_aws_profile(|_| None).is_none());
    }

    #[test]
    fn pick_aws_profile_aws_profileを採用() {
        let v = pick_aws_profile(|k| match k {
            "AWS_PROFILE" => Some("dev".to_string()),
            _ => None,
        });
        assert_eq!(v.as_deref(), Some("dev"));
    }

    #[test]
    fn pick_aws_profile_aws_default_profileのフォールバック() {
        let v = pick_aws_profile(|k| match k {
            "AWS_DEFAULT_PROFILE" => Some("default-profile".to_string()),
            _ => None,
        });
        assert_eq!(v.as_deref(), Some("default-profile"));
    }

    #[test]
    fn pick_aws_profile_aws_sso_profileを優先() {
        // 全部設定されていても AWS_SSO_PROFILE が最優先
        let v = pick_aws_profile(|k| match k {
            "AWS_SSO_PROFILE" => Some("sso".to_string()),
            "AWS_VAULT" => Some("vault".to_string()),
            "AWSUME_PROFILE" => Some("awsume".to_string()),
            "AWS_PROFILE" => Some("p".to_string()),
            "AWS_DEFAULT_PROFILE" => Some("d".to_string()),
            _ => None,
        });
        assert_eq!(v.as_deref(), Some("sso"));
    }

    #[test]
    fn pick_aws_profile_aws_vaultはsso無しで採用() {
        let v = pick_aws_profile(|k| match k {
            "AWS_VAULT" => Some("vault".to_string()),
            "AWSUME_PROFILE" => Some("awsume".to_string()),
            "AWS_PROFILE" => Some("p".to_string()),
            _ => None,
        });
        assert_eq!(v.as_deref(), Some("vault"));
    }

    #[test]
    fn pick_aws_profile_awsume_profileの優先順位() {
        let v = pick_aws_profile(|k| match k {
            "AWSUME_PROFILE" => Some("awsume".to_string()),
            "AWS_PROFILE" => Some("p".to_string()),
            _ => None,
        });
        assert_eq!(v.as_deref(), Some("awsume"));
    }

    #[test]
    fn pick_aws_profile_空文字は次の候補にフォールバック() {
        // 空文字列を返した場合は次の候補を見る (環境変数として "" が設定されている場合)
        let v = pick_aws_profile(|k| match k {
            "AWS_SSO_PROFILE" => Some(String::new()),
            "AWS_PROFILE" => Some("real".to_string()),
            _ => None,
        });
        assert_eq!(v.as_deref(), Some("real"));
    }

    // ── build_direnv_segment のテスト ────────────────────────────

    #[test]
    fn build_direnv_segment_空文字列はnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(build_direnv_segment("", &icons).is_none());
    }

    #[test]
    fn build_direnv_segment_ハイフン付きパスからベース名を抽出() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_direnv_segment("-/tmp/my-project", &icons).unwrap();
        assert_eq!(seg.text, "my-project");
    }

    #[test]
    fn build_direnv_segment_ハイフンなしパスでも動作() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_direnv_segment("/tmp/another", &icons).unwrap();
        assert_eq!(seg.text, "another");
    }

    #[test]
    fn build_direnv_segment_末尾スラッシュは除去される() {
        // PathBuf::file_name は末尾スラッシュを無視するため "dir" を返す
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_direnv_segment("-/tmp/dir/", &icons).unwrap();
        assert_eq!(seg.text, "dir");
    }

    // ── build_nix_shell_segment のテスト ─────────────────────────

    #[test]
    fn build_nix_shell_segment_空文字列はnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(build_nix_shell_segment("", &icons).is_none());
    }

    #[test]
    fn build_nix_shell_segment_pureを表示() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_nix_shell_segment("pure", &icons).unwrap();
        assert_eq!(seg.text, "pure");
    }

    #[test]
    fn build_nix_shell_segment_impureを表示() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_nix_shell_segment("impure", &icons).unwrap();
        assert_eq!(seg.text, "impure");
    }

    #[test]
    fn build_nix_shell_segment_想定外の値はnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(build_nix_shell_segment("garbage", &icons).is_none());
        // 大文字の "Pure" は受け付けない（大文字・小文字を厳格に判定する）。
        assert!(build_nix_shell_segment("Pure", &icons).is_none());
    }

    // ── parse_sysctl_loadavg のテスト ─────────────────────────────

    #[test]
    fn parse_sysctl_loadavg_標準形式() {
        assert_eq!(parse_sysctl_loadavg("{ 1.23 4.56 7.89 }"), Some(1.23));
    }

    #[test]
    fn parse_sysctl_loadavg_整数値() {
        assert_eq!(parse_sysctl_loadavg("{ 0 0 0 }"), Some(0.0));
    }

    #[test]
    fn parse_sysctl_loadavg_前後空白あり() {
        assert_eq!(parse_sysctl_loadavg("  { 2.50 1.20 0.80 }  "), Some(2.50));
    }

    #[test]
    fn parse_sysctl_loadavg_中括弧なし() {
        // 中括弧がなくても先頭の数字を取り出せる
        assert_eq!(parse_sysctl_loadavg("0.5 0.4 0.3"), Some(0.5));
    }

    #[test]
    fn parse_sysctl_loadavg_空文字列() {
        assert_eq!(parse_sysctl_loadavg(""), None);
    }

    #[test]
    fn parse_sysctl_loadavg_数値以外() {
        assert_eq!(parse_sysctl_loadavg("{ abc def }"), None);
    }

    // ── parse_pmset_battery のテスト ─────────────────────────────

    #[test]
    fn parse_pmset_battery_charged_100() {
        let out = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=4456547)\t100%; charged; 0:00 remaining present: true\n";
        let info = parse_pmset_battery(out).unwrap();
        assert_eq!(info.percent, 100);
        assert_eq!(info.state, BatteryState::Full);
    }

    #[test]
    fn parse_pmset_battery_discharging() {
        let out =
            " -InternalBattery-0 (id=4456547)\t76%; discharging; 3:24 remaining present: true";
        let info = parse_pmset_battery(out).unwrap();
        assert_eq!(info.percent, 76);
        assert_eq!(info.state, BatteryState::Discharging);
    }

    #[test]
    fn parse_pmset_battery_charging() {
        let out = " -InternalBattery-0 (id=4456547)\t50%; charging; 1:30 remaining present: true";
        let info = parse_pmset_battery(out).unwrap();
        assert_eq!(info.percent, 50);
        assert_eq!(info.state, BatteryState::Charging);
    }

    #[test]
    fn parse_pmset_battery_finishing_charge() {
        let out =
            " -InternalBattery-0 (id=4456547)\t99%; finishing charge; 0:01 remaining present: true";
        let info = parse_pmset_battery(out).unwrap();
        assert_eq!(info.percent, 99);
        assert_eq!(info.state, BatteryState::Full);
    }

    #[test]
    fn parse_pmset_battery_no_internal_battery() {
        // バッテリーがない場合は None
        let out = "Now drawing from 'AC Power'\n";
        assert!(parse_pmset_battery(out).is_none());
    }

    #[test]
    fn parse_pmset_battery_percent_なしは異常入力としてnone() {
        // % が含まれない異常出力では id 内の数字を percent と誤認しないこと
        let out = " -InternalBattery-0 (id=4456547) charged present: true";
        assert!(parse_pmset_battery(out).is_none());
    }

    #[test]
    fn read_linux_battery_from_は不完全なデバイスを飛ばす() {
        let tmp = tempfile::tempdir().unwrap();
        let incomplete = tmp.path().join("BAT0");
        let valid = tmp.path().join("BAT1");
        std::fs::create_dir(&incomplete).unwrap();
        std::fs::create_dir(&valid).unwrap();
        std::fs::write(incomplete.join("status"), "Unknown\n").unwrap();
        std::fs::write(valid.join("capacity"), "73\n").unwrap();
        std::fs::write(valid.join("status"), "Charging\n").unwrap();

        let info = read_linux_battery_from(tmp.path()).expect("有効なBAT1を取得する");
        assert_eq!(info.percent, 73);
        assert_eq!(info.state, BatteryState::Charging);
    }

    // ── parse_df_used_pct のテスト ────────────────────────────────

    #[test]
    fn parse_df_used_pct_macos形式() {
        let out = "Filesystem 1024-blocks Used Available Capacity Mounted on\n/dev/disk3s1 1953480400 506316192 1428388824 26% /\n";
        assert_eq!(parse_df_used_pct(out), Some(26));
    }

    #[test]
    fn parse_df_used_pct_linux形式() {
        let out = "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/sda1 20486368 5238100 14186668 27% /\n";
        assert_eq!(parse_df_used_pct(out), Some(27));
    }

    #[test]
    fn parse_df_used_pct_ヘッダーのみはnone() {
        let out = "Filesystem 1024-blocks Used Available Capacity Mounted on\n";
        assert!(parse_df_used_pct(out).is_none());
    }

    #[test]
    fn parse_df_used_pct_空文字列() {
        assert!(parse_df_used_pct("").is_none());
    }

    #[test]
    fn parse_df_used_pct_使用率100() {
        let out = "Filesystem 1K-blocks Used Available Use% Mounted on\n/dev/sda1 1000 1000 0 100% /full\n";
        assert_eq!(parse_df_used_pct(out), Some(100));
    }

    #[test]
    fn parse_df_used_pct_filesystem名に空白を含む場合も正しく抽出する() {
        // macOS の autofs は Filesystem 名が `map auto_home` のため列位置がずれる
        let out = "Filesystem    512-blocks Used Available Capacity  Mounted on\nmap auto_home          0    0         0   100%    /System/Volumes/Data/home\n";
        assert_eq!(parse_df_used_pct(out), Some(100));
    }

    #[test]
    fn parse_pmset_battery_not_charging_は充電中扱いしない() {
        // 最適化充電/充電上限による「AC 接続・充電停止」状態 (現代の MacBook で恒常的)
        let out = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=39780451)\t80%; AC attached; not charging present: true\n";
        let info = parse_pmset_battery(out).unwrap();
        assert_eq!(info.percent, 80);
        assert_eq!(info.state, BatteryState::Full);
    }

    // ── parse_meminfo_used_pct のテスト ──────────────────────────

    #[test]
    fn parse_meminfo_used_pct_標準ケース() {
        // total=10GB, available=2GB → 使用率 80%
        let content =
            "MemTotal:       10000000 kB\nMemFree:         500000 kB\nMemAvailable:   2000000 kB\n";
        assert_eq!(parse_meminfo_used_pct(content), Some(80));
    }

    #[test]
    fn parse_meminfo_used_pct_空きが多い() {
        let content = "MemTotal:       10000000 kB\nMemAvailable:   8000000 kB\n";
        assert_eq!(parse_meminfo_used_pct(content), Some(20));
    }

    #[test]
    fn parse_meminfo_used_pct_total無しはnone() {
        let content = "MemAvailable:   8000000 kB\n";
        assert!(parse_meminfo_used_pct(content).is_none());
    }

    #[test]
    fn parse_meminfo_used_pct_available無しは100() {
        // available が 0 とみなされ、すべて使用中扱い
        let content = "MemTotal:       10000000 kB\n";
        assert_eq!(parse_meminfo_used_pct(content), Some(100));
    }

    // ── parse_vm_stat_used_pct のテスト ──────────────────────────

    #[test]
    fn parse_vm_stat_used_pct_標準ケース() {
        // total=64GB (64*1024*1024*1024), page=16384
        // free=100000 pages, inactive=200000 pages, speculative=50000
        // 合計 free pages = 350000, free bytes = 350000 * 16384 = 5.7GB
        // used = ~58GB → 約 91%
        let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
                  Pages free:                              100000.\n\
                  Pages active:                           1000000.\n\
                  Pages inactive:                          200000.\n\
                  Pages speculative:                        50000.\n\
                  Pages wired down:                        500000.\n\
                  Pages purgeable:                          10000.\n";
        let total = 64u64 * 1024 * 1024 * 1024;
        let pct = parse_vm_stat_used_pct(vm, total).unwrap();
        assert!((90..=92).contains(&pct), "想定 ~91% だが {pct}%");
    }

    #[test]
    fn parse_vm_stat_used_pct_purgeableは加算されない() {
        // 同じ vm_stat に対し、purgeable がない/あるで結果が変わらないこと
        let vm_with_purgeable = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\n\
                  Pages free:                              1000.\n\
                  Pages purgeable:                         9999.\n";
        let vm_without = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\n\
                  Pages free:                              1000.\n";
        let total = 1024u64 * 1024 * 1024;
        assert_eq!(
            parse_vm_stat_used_pct(vm_with_purgeable, total),
            parse_vm_stat_used_pct(vm_without, total),
            "purgeable の有無で結果が変わってはいけない"
        );
    }

    #[test]
    fn parse_vm_stat_used_pct_total_zeroはnone() {
        let vm = "Mach Virtual Memory Statistics: (page size of 4096 bytes)\n\
                  Pages free:                              100.\n";
        assert!(parse_vm_stat_used_pct(vm, 0).is_none());
    }

    #[test]
    fn parse_vm_stat_used_pct_空入力はnone() {
        assert!(parse_vm_stat_used_pct("", 1024).is_none());
    }

    // ── build_vi_mode_segment のテスト ───────────────────────────

    #[test]
    fn build_vi_mode_segment_insert() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_vi_mode_segment("insert", &icons).unwrap();
        assert_eq!(seg.text, "I");
    }

    #[test]
    fn build_vi_mode_segment_normal() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_vi_mode_segment("normal", &icons).unwrap();
        assert_eq!(seg.text, "N");
    }

    #[test]
    fn build_vi_mode_segment_visual() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        let seg = build_vi_mode_segment("visual", &icons).unwrap();
        assert_eq!(seg.text, "V");
    }

    #[test]
    fn build_vi_mode_segment_zsh_keymap名でも動作() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        // zsh の KEYMAP 値そのものを渡しても解釈できる
        assert_eq!(build_vi_mode_segment("viins", &icons).unwrap().text, "I");
        assert_eq!(build_vi_mode_segment("vicmd", &icons).unwrap().text, "N");
        assert_eq!(build_vi_mode_segment("vivis", &icons).unwrap().text, "V");
    }

    #[test]
    fn build_vi_mode_segment_未知の値はnone() {
        let icons = Icons::for_level(crate::icons::FontLevel::Unicode);
        assert!(build_vi_mode_segment("", &icons).is_none());
        assert!(build_vi_mode_segment("garbage", &icons).is_none());
    }

    // ── num_cpus のテスト ────────────────────────────────────────

    #[test]
    fn num_cpus_は1以上を返す() {
        assert!(num_cpus() >= 1, "num_cpus は最低 1 を返すべき");
    }

    // ── read_cpu_arch / segment_cpu_arch のテスト ───────────────

    #[test]
    fn read_cpu_arch_は実行環境のアーキ名を返す() {
        // macOS なら "arm64"/"x86_64"、Linux なら同等の値が返る
        let arch = read_cpu_arch().expect("CPU arch should be detected on supported platforms");
        assert!(!arch.is_empty(), "アーキ名は空でないべき");
        // 改行や前後空白が混じらず単一トークンで返る
        assert!(!arch.contains('\n'), "改行を含むべきでない: {arch:?}");
    }

    #[test]
    fn segment_cpu_arch_アイコンとテキストを設定する() {
        let icons = Icons::for_level(crate::icons::FontLevel::Ascii);
        let seg = segment_cpu_arch(&icons).expect("対応プラットフォームではセグメントが返るべき");
        assert!(!seg.text.is_empty(), "text が空");
        assert_eq!(seg.icon, "arch", "ascii アイコンが設定されていない");
    }

    // ── is_root_user_with のテスト ───────────────────────────────

    #[test]
    fn is_root_user_with_userがrootならtrue() {
        // 環境変数 USER=root をシミュレートし、フォールバックは呼ばれないことを確認する。
        assert!(is_root_user_with(Some("root"), || {
            panic!("USER=root のときは fallback を呼ぶべきでない")
        }));
    }

    #[test]
    fn is_root_user_with_userが非rootならfallbackせずfalse() {
        assert!(!is_root_user_with(Some("nonroot"), || Some(
            "0".to_string()
        )));
        assert!(!is_root_user_with(Some("nonroot"), || Some(
            "1000".to_string()
        )));
    }

    #[test]
    fn is_root_user_with_userが空文字ならfallbackせずfalse() {
        assert!(!is_root_user_with(Some(""), || Some("0".to_string())));
    }

    #[test]
    fn is_root_user_with_user未設定でfallback() {
        assert!(is_root_user_with(None, || Some("0".to_string())));
        assert!(!is_root_user_with(None, || None));
    }

    // ── segment_dir_writable のテスト ───────────────────────────

    // cargo test はスレッド並列で走るが、カレントディレクトリはプロセス共通のため
    // cwd を変更するテスト同士を直列化する (無同期だと相互に上書きしてフレークする)。
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn segment_dir_writable_書き込み可能ディレクトリではnone() {
        let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        // 一時ディレクトリは現在ユーザーで書き込み可能
        let dir = tempfile::tempdir().unwrap();
        let prev = env::current_dir().ok();
        env::set_current_dir(dir.path()).unwrap();
        let icons = Icons::for_level(crate::icons::FontLevel::Ascii);
        let result = segment_dir_writable(&icons);
        if let Some(p) = prev {
            let _ = env::set_current_dir(p);
        }
        assert!(result.is_none(), "書き込み可能なら None");
    }

    #[cfg(unix)]
    #[test]
    fn segment_dir_writable_書き込み不可ディレクトリで警告() {
        use std::os::unix::fs::PermissionsExt;
        let _guard = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("ro");
        std::fs::create_dir(&sub).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
        let prev = env::current_dir().ok();
        env::set_current_dir(&sub).unwrap();
        let icons = Icons::for_level(crate::icons::FontLevel::Ascii);
        let result = segment_dir_writable(&icons);
        if let Some(p) = prev {
            let _ = env::set_current_dir(p);
        }
        // 後始末のため書き込み権限を戻してから drop させる
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).ok();
        assert!(
            result.is_some(),
            "書き込み不可ディレクトリではセグメントが返るべき"
        );
        assert_eq!(result.unwrap().icon, "lock");
    }

    // ── parse_ip_addr のテスト (Linux 形式) ─────────────────────

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn parse_ip_addr_標準形式() {
        let sample = "2: eth0    inet 192.168.1.10/24 brd 192.168.1.255 scope global eth0\n";
        assert_eq!(parse_ip_addr(sample), Some("192.168.1.10".to_string()));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn parse_ip_addr_複数行で最初の候補を返す() {
        let sample = "\
2: eth0    inet 10.0.0.1/24 scope global eth0
3: wlan0   inet 192.168.1.50/24 scope global wlan0
";
        assert_eq!(parse_ip_addr(sample), Some("10.0.0.1".to_string()));
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn parse_ip_addr_空文字列はnone() {
        assert_eq!(parse_ip_addr(""), None);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn parse_ip_addr_inet以外を無視する() {
        let sample = "lo  link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00\n";
        assert_eq!(parse_ip_addr(sample), None);
    }

    // ── is_writable_dir のテスト ────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn is_writable_dir_書き込み可能ディレクトリでtrue() {
        // tempdir は呼び出し元ユーザーで書き込み可能。
        let dir = tempfile::tempdir().unwrap();
        assert!(is_writable_dir(dir.path()));
    }

    #[cfg(unix)]
    #[test]
    fn is_writable_dir_書き込み不可ディレクトリでfalse() {
        use std::os::unix::fs::PermissionsExt;
        // root は DAC を無視するためスキップする (ensure_writable と同じ理由)。
        if env::var("USER").as_deref() == Ok("root") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("ro");
        std::fs::create_dir(&sub).unwrap();
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = is_writable_dir(&sub);
        // tempdir 削除のため書き込み権限を戻す。
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).ok();
        assert!(!result, "書き込み不可ディレクトリでは false を返すべき");
    }

    #[cfg(unix)]
    #[test]
    fn is_writable_dir_nul文字を含むパスは警告を出さずtrue() {
        // パス文字列に NUL バイトが含まれると CString::new が失敗するため、
        // 偽の "書き込み不可" 警告を出さないよう true を返す挙動を確認する。
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let bytes = b"/tmp/zsh-turbo-\0-dummy";
        let p = std::path::PathBuf::from(OsStr::from_bytes(bytes));
        assert!(
            is_writable_dir(&p),
            "NUL を含むパスは判定不能扱いで警告を出さないこと"
        );
    }
}
