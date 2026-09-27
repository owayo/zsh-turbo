use std::fs::{self, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

const START: &str = "# >>> zsh-turbo font >>>";
const END: &str = "# <<< zsh-turbo font <<<";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
static JAPANESE: OnceLock<bool> = OnceLock::new();

fn tr<'a>(en: &'a str, ja: &'a str) -> &'a str {
    if *JAPANESE.get_or_init(crate::tui::interface_is_japanese) {
        ja
    } else {
        en
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalKind {
    Ghostty,
    Cmux,
    MacTerminal,
    ITerm,
}

impl TerminalKind {
    fn name(self) -> &'static str {
        match self {
            Self::Ghostty => "Ghostty",
            Self::Cmux => "cmux",
            Self::MacTerminal => "Terminal.app",
            Self::ITerm => "iTerm2",
        }
    }
}

pub fn run() -> io::Result<()> {
    if !io::stdin().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            tr(
                "font setup requires an interactive terminal",
                "フォント設定には対話可能な端末が必要です",
            ),
        ));
    }
    println!("{}", tr("Terminal font setup", "端末フォント設定"));
    let detected = match std::env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "ghostty" => 1,
        "cmux" => 2,
        "apple_terminal" => 3,
        "iterm.app" => 4,
        _ => 1,
    };
    println!("1) Ghostty  2) cmux  3) Terminal.app  4) iTerm2");
    let terminal = match choose_number(tr("Terminal", "端末"), detected, 4)? {
        1 => TerminalKind::Ghostty,
        2 => TerminalKind::Cmux,
        3 => TerminalKind::MacTerminal,
        _ => TerminalKind::ITerm,
    };

    let fonts = installed_fonts();
    println!(
        "{}",
        tr(
            "Font: 1) Keep terminal default  2) MesloLGS NF  3) Choose an installed family",
            "フォント: 1) 端末の既定値  2) MesloLGS NF  3) インストール済みのフォントを選択"
        )
    );
    let family = match choose_number(tr("Font", "フォント"), 1, 3)? {
        1 => None,
        2 => Some("MesloLGS NF".to_string()),
        _ => {
            let query = ask(tr(
                "Filter installed fonts (blank lists first 30)",
                "フォント名で絞り込み（空欄で先頭 30 件）",
            ))?;
            let matches: Vec<&str> = fonts
                .iter()
                .filter(|font| {
                    font.to_ascii_lowercase()
                        .contains(&query.to_ascii_lowercase())
                })
                .map(String::as_str)
                .take(30)
                .collect();
            for (i, name) in matches.iter().enumerate() {
                println!("{}) {name}", i + 1);
            }
            let selected = ask(tr(
                "Number or exact font family",
                "番号または正確なフォント名",
            ))?;
            let family = selected
                .parse::<usize>()
                .ok()
                .and_then(|n| matches.get(n.saturating_sub(1)).copied())
                .unwrap_or(&selected)
                .trim();
            validate_family(family)?;
            Some(family.to_string())
        }
    };

    if matches!(terminal, TerminalKind::MacTerminal | TerminalKind::ITerm) {
        let family = family
            .as_deref()
            .unwrap_or(tr("your preferred font", "使用したいフォント"));
        if terminal == TerminalKind::MacTerminal {
            println!(
                "{} {family}",
                tr(
                    "Terminal → Settings → Profiles → Text → Font → Change: select",
                    "ターミナル → 設定 → プロファイル → テキスト → フォント → 変更 で選択:"
                )
            );
        } else {
            println!(
                "{} {family}",
                tr(
                    "iTerm2 → Settings → Profiles → Text → Font: select",
                    "iTerm2 → Settings → Profiles → Text → Font で選択:"
                )
            );
            println!(
                "{}",
                tr(
                    "If Use Non-ASCII Font is enabled, set its font as well.",
                    "Use Non-ASCII Font が有効なら、そちらのフォントも設定してください。"
                )
            );
        }
        println!(
            "{}",
            tr(
                "Set Prompt → Font level in `zsh-turbo configure` to match the font's symbols.",
                "記号に合わせて `zsh-turbo configure` の Prompt → Font level を設定してください。"
            )
        );
        return Ok(());
    }

    println!(
        "{}",
        tr(
            "Ligatures: 1) Terminal default  2) Enable calt + liga  3) Disable calt + liga",
            "リガチャ: 1) 端末の既定値  2) calt + liga を有効化  3) calt + liga を無効化"
        )
    );
    let ligatures = choose_number(tr("Ligatures", "リガチャ"), 1, 3)?;
    if family.as_deref() == Some("MesloLGS NF") && ligatures == 2 {
        println!(
            "{}",
            tr(
                "MesloLGS NF has no programming ligatures; these flags will not change its appearance.",
                "MesloLGS NF にプログラミング用リガチャはありません。この設定では見た目は変わりません。"
            )
        );
    }
    let candidates = config_candidates(terminal)?;
    println!("{}", tr("Configuration files:", "設定ファイル:"));
    for (i, path) in candidates.iter().enumerate() {
        println!(
            "{}) {}{}",
            i + 1,
            path.display(),
            if path.exists() {
                tr(" (exists)", "（既存）")
            } else {
                ""
            }
        );
    }
    let default_path = candidates
        .iter()
        .rposition(|path| path.exists())
        .map_or(1, |i| i + 1);
    let chosen = choose_number(tr("Write to", "書き込み先"), default_path, candidates.len())? - 1;
    let path = &candidates[chosen];
    if terminal == TerminalKind::Ghostty && chosen < default_path.saturating_sub(1) {
        println!(
            "{}",
            tr(
                "A later configuration file may override this choice.",
                "後で読み込まれる設定ファイルがこの選択を上書きする可能性があります。"
            )
        );
    }
    let resolved = resolve_path(path)?;
    let previous = match fs::read_to_string(&resolved) {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let updated = update_config(&previous, family.as_deref(), ligatures)?;
    println!(
        "\n{} {}: {}",
        terminal.name(),
        tr("configuration", "の設定"),
        path.display()
    );
    println!("{}", managed_block(family.as_deref(), ligatures)?);
    if updated == previous {
        println!(
            "{}",
            tr(
                "Already configured; no changes made.",
                "設定済みのため変更はありません。"
            )
        );
        return Ok(());
    }
    if ask(tr(
        "Apply these settings? [y/N]",
        "この設定を適用しますか？ [y/N]",
    ))?
    .eq_ignore_ascii_case("y")
    {
        if family.as_deref() == Some("MesloLGS NF")
            && ask(tr(
                "Install MesloLGS NF if needed? [y/N]",
                "必要なら MesloLGS NF をインストールしますか？ [y/N]",
            ))?
            .eq_ignore_ascii_case("y")
        {
            crate::font_install::install_meslo_nerd_font(false, "\n")?;
        }
        write_config(&resolved, &previous, &updated)?;
        println!(
            "{}",
            tr(
                "Saved. Reload Ghostty or restart cmux to apply the font.",
                "保存しました。Ghostty は設定を再読み込み、cmux は再起動してください。"
            )
        );
    } else {
        println!("{}", tr("Cancelled.", "取り消しました。"));
    }
    Ok(())
}

fn ask(label: &str) -> io::Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut input = String::new();
    if io::stdin().read_line(&mut input)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            tr("input ended", "入力が終了しました"),
        ));
    }
    Ok(input.trim().to_string())
}

fn choose_number(label: &str, default: usize, max: usize) -> io::Result<usize> {
    loop {
        let answer = ask(&format!("{label} [{default}]"))?;
        if answer.is_empty() {
            return Ok(default);
        }
        if let Ok(n) = answer.parse::<usize>()
            && (1..=max).contains(&n)
        {
            return Ok(n);
        }
        println!(
            "{} 1–{max}.",
            tr("Choose a number from", "番号を選んでください:")
        );
    }
}

fn installed_fonts() -> Vec<String> {
    let output = Command::new("ghostty")
        .arg("+list-fonts")
        .output()
        .ok()
        .or_else(|| {
            Command::new("/Applications/Ghostty.app/Contents/MacOS/ghostty")
                .arg("+list-fonts")
                .output()
                .ok()
        });
    let mut fonts: Vec<String> = output
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|line| !line.starts_with(char::is_whitespace) && !line.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    fonts.sort();
    fonts.dedup();
    fonts
}

fn config_candidates(terminal: TerminalKind) -> io::Result<Vec<PathBuf>> {
    let home = dirs::home_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "home directory unavailable"))?;
    if terminal == TerminalKind::Cmux {
        return Ok(vec![home.join(
            "Library/Application Support/com.cmuxterm.app/config.ghostty",
        )]);
    }
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let mut choices = vec![xdg.join("ghostty/config.ghostty")];
    let old = xdg.join("ghostty/config");
    if old.exists() {
        choices.push(old);
    }
    let app_support = home.join("Library/Application Support/com.mitchellh.ghostty/config.ghostty");
    if app_support.exists() {
        choices.push(app_support);
    }
    let app_support_old = home.join("Library/Application Support/com.mitchellh.ghostty/config");
    if app_support_old.exists() {
        choices.push(app_support_old);
    }
    Ok(choices)
}

fn validate_family(family: &str) -> io::Result<()> {
    if family.is_empty()
        || family
            .chars()
            .any(|c| c.is_control() || c == '#' || c == '=' || c == '"')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid font family",
        ));
    }
    Ok(())
}

fn managed_block(family: Option<&str>, ligatures: usize) -> io::Result<String> {
    let mut text = format!("{START}\n");
    if let Some(family) = family {
        validate_family(family)?;
        text.push_str("font-family = \"\"\n");
        text.push_str(&format!("font-family = {family}\n"));
    }
    if ligatures == 2 || ligatures == 3 {
        let sign = if ligatures == 2 { '+' } else { '-' };
        text.push_str(&format!(
            "font-feature = {sign}calt\nfont-feature = {sign}liga\n"
        ));
    }
    text.push_str(END);
    Ok(text)
}

fn update_config(previous: &str, family: Option<&str>, ligatures: usize) -> io::Result<String> {
    let block = managed_block(family, ligatures)?;
    let starts: Vec<usize> = previous.match_indices(START).map(|(i, _)| i).collect();
    let ends: Vec<usize> = previous.match_indices(END).map(|(i, _)| i).collect();
    if starts.len() != ends.len()
        || starts.len() > 1
        || starts
            .first()
            .zip(ends.first())
            .is_some_and(|(s, e)| s >= e)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "font managed block is incomplete or duplicated",
        ));
    }
    if let (Some(&start), Some(&end)) = (starts.first(), ends.first()) {
        let tail = end + END.len();
        let mut result = previous.to_string();
        let line_ending = if previous.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        result.replace_range(start..tail, &block.replace('\n', line_ending));
        Ok(result)
    } else {
        if family.is_none() && ligatures == 1 {
            return Ok(previous.to_string());
        }
        let newline = if previous.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let mut result = previous.to_string();
        if !result.is_empty() && !result.ends_with('\n') {
            result.push_str(newline);
        }
        result.push_str(&block.replace('\n', newline));
        result.push_str(newline);
        Ok(result)
    }
}

fn resolve_path(path: &Path) -> io::Result<PathBuf> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => fs::canonicalize(path),
        Ok(_) => Ok(path.to_path_buf()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(path.to_path_buf()),
        Err(e) => Err(e),
    }
}

fn write_config(path: &Path, expected: &str, updated: &str) -> io::Result<()> {
    let current = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    if current != expected {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "configuration changed while editing",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid config path"))?;
    fs::create_dir_all(parent)?;
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let temp = parent.join(format!(
        ".{name}.zsh-turbo-{}-{sequence}",
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        if let Ok(metadata) = fs::metadata(path) {
            file.set_permissions(metadata.permissions())?;
        }
        file.write_all(updated.as_bytes())?;
        file.sync_all()?;
        if path.exists() {
            let backup = parent.join(format!(
                "{name}.zsh-turbo-backup-{}-{sequence}",
                std::process::id()
            ));
            let mut backup_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&backup)?;
            backup_file.write_all(expected.as_bytes())?;
            backup_file.sync_all()?;
            println!("Backup: {}", backup.display());
        }
        if fs::read_to_string(path).unwrap_or_default() != expected {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "configuration changed while editing",
            ));
        }
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_block_is_idempotent_and_keeps_other_settings() {
        let original = "theme = dark\n";
        let first = update_config(original, Some("MesloLGS NF"), 2).unwrap();
        assert!(first.starts_with(original));
        assert!(first.contains("font-feature = +calt\nfont-feature = +liga"));
        assert_eq!(
            update_config(&first, Some("MesloLGS NF"), 2).unwrap(),
            first
        );
        assert!(
            update_config(&first, Some("JetBrains Mono"), 3)
                .unwrap()
                .contains("font-feature = -liga")
        );
    }

    #[test]
    fn malformed_markers_are_rejected() {
        assert!(update_config(START, None, 1).is_err());
        assert!(update_config(&format!("{START}\n{END}\n{START}\n{END}"), None, 1).is_err());
        assert!(managed_block(Some("bad\nvalue"), 1).is_err());
    }

    #[test]
    fn default_choices_do_not_create_a_block_and_crlf_is_preserved() {
        assert_eq!(
            update_config("theme = dark\n", None, 1).unwrap(),
            "theme = dark\n"
        );
        let first = update_config("theme = dark\r\n", Some("Fira Code"), 2).unwrap();
        assert!(!first.replace("\r\n", "").contains('\n'));
        assert_eq!(update_config(&first, Some("Fira Code"), 2).unwrap(), first);
    }

    #[test]
    fn symlink_is_preserved_and_concurrent_change_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        fs::write(&target, "theme = dark\n").unwrap();
        let link = dir.path().join("config");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).unwrap();
        #[cfg(unix)]
        {
            let resolved = resolve_path(&link).unwrap();
            let updated = update_config("theme = dark\n", None, 2).unwrap();
            write_config(&resolved, "theme = dark\n", &updated).unwrap();
            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert!(write_config(&resolved, "theme = dark\n", &updated).is_err());
        }
    }
}
