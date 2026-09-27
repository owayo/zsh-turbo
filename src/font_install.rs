use crate::style::{Color, plain_colored};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

/// MesloLGS NF (Nerd Font 加工済み)。
/// 配布元: <https://github.com/romkatv/powerlevel10k-media>
const MESLO_BASE_URL: &str = "https://raw.githubusercontent.com/romkatv/powerlevel10k-media/145eb9fbc2f42ee408dacd9b22d8e6e0e553f83d";
const MESLO_LICENSE: &str = include_str!("../licenses/MesloLGS-NF.txt");
const MESLO_LICENSE_FILE: &str = "MesloLGS NF License.txt";

const MESLO_VARIANTS: &[&str] = &[
    "MesloLGS NF Regular.ttf",
    "MesloLGS NF Bold.ttf",
    "MesloLGS NF Italic.ttf",
    "MesloLGS NF Bold Italic.ttf",
];

/// インストール結果。件数はライセンス文書を除くフォントファイルのみ。
pub struct InstallResult {
    pub dest_dir: PathBuf,
    pub installed: usize,
    pub skipped: usize,
}

/// MesloLGS NF を OS 既定のユーザフォントディレクトリへダウンロードする。
///
/// すでに同名ファイルが存在する場合はスキップする（`force=true` で上書き）。
/// ターミナルが raw mode の場合は `line_ending` に `"\r\n"` を渡す。
pub fn install_meslo_nerd_font(force: bool, line_ending: &str) -> io::Result<InstallResult> {
    let dest_dir = font_install_dir()?;
    let result = install_meslo_nerd_font_to(dest_dir, force, line_ending)?;
    refresh_font_cache(line_ending);
    Ok(result)
}

fn install_meslo_nerd_font_to(
    dest_dir: PathBuf,
    force: bool,
    line_ending: &str,
) -> io::Result<InstallResult> {
    std::fs::create_dir_all(&dest_dir)?;
    ensure_writable(&dest_dir)?;
    save_font_license(&dest_dir)?;

    print_line(
        &format!(
            "  {} {}",
            plain_colored("\u{2192}", Color::BrightBlack),
            plain_colored(
                &format!("Installing MesloLGS NF to {}", dest_dir.display()),
                Color::BrightCyan,
            )
        ),
        line_ending,
    );

    ensure_curl()?;

    let mut installed = 0;
    let mut skipped = 0;

    for variant in MESLO_VARIANTS {
        let dest = dest_dir.join(variant);
        if !force && dest.exists() {
            print_line(
                &format!(
                    "  {} {} {}",
                    plain_colored("\u{2713}", Color::BrightBlack),
                    variant,
                    plain_colored("(already installed)", Color::BrightBlack),
                ),
                line_ending,
            );
            skipped += 1;
            continue;
        }

        let url = format!("{MESLO_BASE_URL}/{}", percent_encode(variant));
        print_line(
            &format!(
                "  {} {} ...",
                plain_colored("\u{2193}", Color::BrightBlack),
                variant
            ),
            line_ending,
        );

        download(&url, &dest)?;
        installed += 1;
    }

    Ok(InstallResult {
        dest_dir,
        installed,
        skipped,
    })
}

// フォントがインストール済みでも、ライセンスはアトミックに補完する。
fn save_font_license(dest_dir: &Path) -> io::Result<()> {
    use std::io::Write;

    let dest = dest_dir.join(MESLO_LICENSE_FILE);
    let tmp = create_download_temp(&dest)?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new().write(true).open(&tmp)?;
        file.write_all(MESLO_LICENSE.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, &dest)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// macOS は `~/Library/Fonts`、それ以外の Unix は `~/.local/share/fonts` を使う。
#[cfg(target_os = "macos")]
fn font_install_dir() -> io::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not resolve home directory")
    })?;
    Ok(home.join("Library").join("Fonts"))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn font_install_dir() -> io::Result<PathBuf> {
    let base = dirs::data_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "could not resolve data directory")
    })?;
    Ok(base.join("fonts"))
}

#[cfg(not(any(target_os = "macos", unix)))]
fn font_install_dir() -> io::Result<PathBuf> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "font install is only supported on macOS and Linux",
    ))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn refresh_font_cache(line_ending: &str) {
    if which("fc-cache").is_some() {
        let status = Command::new("fc-cache").arg("-f").status();
        if matches!(status, Ok(s) if s.success()) {
            print_line(
                &format!(
                    "  {} fc-cache refreshed",
                    plain_colored("\u{2713}", Color::BrightGreen)
                ),
                line_ending,
            );
        }
    }
}

#[cfg(not(all(unix, not(target_os = "macos"))))]
fn refresh_font_cache(_line_ending: &str) {}

fn ensure_curl() -> io::Result<()> {
    if which("curl").is_some() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "curl is required to download MesloLGS NF. Install curl and retry.",
        ))
    }
}

fn download(url: &str, dest: &Path) -> io::Result<()> {
    let tmp = create_download_temp(dest)?;

    let status = match Command::new("curl")
        .arg("-fSL")
        .arg("--retry")
        .arg("3")
        .arg("--connect-timeout")
        .arg("15")
        .arg("-o")
        .arg(&tmp)
        .arg(url)
        .status()
    {
        Ok(status) => status,
        Err(error) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
    };
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(io::Error::other(format!(
            "curl failed to download {url} ({status})"
        )));
    }

    // 同名ファイルがあれば差し替える（macOS は rename で上書きできる）。
    std::fs::rename(&tmp, dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io::Error::other(format!(
            "failed to move {} -> {}: {e}",
            tmp.display(),
            dest.display()
        ))
    })?;
    Ok(())
}

static DOWNLOAD_TEMP_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn create_download_temp(dest: &Path) -> io::Result<PathBuf> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let file_name = dest
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();

    for _ in 0..100 {
        let sequence = DOWNLOAD_TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp = parent.join(format!(
            ".{file_name}.zsh-turbo-download-{}-{sequence}",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => {
                drop(file);
                return Ok(tmp);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "フォント取得用の一時ファイルを作成できませんでした",
    ))
}

/// インストール先ディレクトリが書き込み可能かを事前に確認する。
/// 一意なプローブを作成して、同名の既存ファイルを変更しない。
fn ensure_writable(dir: &Path) -> io::Result<()> {
    static PROBE_SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    for _ in 0..100 {
        let sequence = PROBE_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let probe = dir.join(format!(
            ".zsh-turbo-write-test-{}-{sequence}",
            std::process::id()
        ));
        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&probe)
        {
            Ok(_) => {
                return std::fs::remove_file(&probe).map_err(|e| {
                    io::Error::new(
                        e.kind(),
                        format!("failed to remove write probe {}: {e}", probe.display()),
                    )
                });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("{} is not writable: {e}", dir.display()),
                ));
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("failed to create a unique write probe in {}", dir.display()),
    ))
}

fn which(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(cmd);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// GitHub raw URL 用の最小限のパーセントエンコード（スペースのみ対象）。
fn percent_encode(name: &str) -> String {
    name.replace(' ', "%20")
}

fn print_line(text: &str, line_ending: &str) {
    print!("{text}{line_ending}");
    use std::io::Write;
    let _ = io::stdout().flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn インストール済みフォントを保持してライセンスを補完する() {
        let tmp = tempfile::tempdir().unwrap();
        for variant in MESLO_VARIANTS {
            std::fs::write(tmp.path().join(variant), b"existing font").unwrap();
        }

        let result = install_meslo_nerd_font_to(tmp.path().to_path_buf(), false, "\n").unwrap();

        assert_eq!(result.installed, 0);
        assert_eq!(result.skipped, 4);
        for variant in MESLO_VARIANTS {
            assert_eq!(
                std::fs::read(tmp.path().join(variant)).unwrap(),
                b"existing font"
            );
        }
        let license = std::fs::read_to_string(tmp.path().join(MESLO_LICENSE_FILE)).unwrap();
        assert!(license.contains("Copyright 2009, 2010, 2013 André Berg"));
        assert!(license.contains("END OF TERMS AND CONDITIONS"));
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 5);
    }

    #[test]
    fn ライセンス保存失敗時はフォントを取得せず一時ファイルを残さない() {
        let tmp = tempfile::tempdir().unwrap();
        let blocked = tmp.path().join(MESLO_LICENSE_FILE);
        std::fs::create_dir(&blocked).unwrap();
        std::fs::write(blocked.join("keep"), b"keep").unwrap();

        assert!(install_meslo_nerd_font_to(tmp.path().to_path_buf(), false, "\n").is_err());
        assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"keep");
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn percent_encode_はスペースのみをエスケープする() {
        assert_eq!(
            percent_encode("MesloLGS NF Bold.ttf"),
            "MesloLGS%20NF%20Bold.ttf"
        );
    }

    #[test]
    fn meslo_variants_は4種類でユニーク() {
        let mut set: Vec<&str> = MESLO_VARIANTS.to_vec();
        set.sort();
        set.dedup();
        assert_eq!(set.len(), MESLO_VARIANTS.len());
        assert_eq!(MESLO_VARIANTS.len(), 4);
    }

    #[test]
    fn which_は存在するコマンドのパスを返す() {
        let found = which("sh");
        assert!(found.is_some(), "sh が PATH から見つからない");
        assert!(found.unwrap().is_file());
    }

    #[test]
    fn which_は存在しないコマンドでnoneを返す() {
        assert!(which("zsh-turbo-definitely-missing-cmd-xyz").is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn font_install_dir_はライブラリフォンツを指す() {
        let dir = font_install_dir().expect("ホームディレクトリが解決できない");
        assert!(dir.ends_with("Library/Fonts"), "dir={}", dir.display());
    }

    #[test]
    fn ensure_writable_は書き込み可能ディレクトリでokを返しプローブを残さない() {
        let tmp = tempfile::tempdir().expect("tempdir 作成失敗");
        assert!(ensure_writable(tmp.path()).is_ok());
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
    }

    #[test]
    fn ensure_writable_は同名の既存ファイルを変更しない() {
        let tmp = tempfile::tempdir().expect("tempdir 作成失敗");
        let existing = tmp.path().join(".zsh-turbo-write-test");
        std::fs::write(&existing, b"keep").expect("既存ファイル作成失敗");

        assert!(ensure_writable(tmp.path()).is_ok());
        assert_eq!(std::fs::read(&existing).unwrap(), b"keep");
    }

    #[test]
    fn create_download_temp_は同じ保存先でも一意なファイルを予約する() {
        let tmp = tempfile::tempdir().expect("tempdir 作成失敗");
        let dest = tmp.path().join("font.ttf");

        let first = create_download_temp(&dest).expect("1個目の一時ファイル作成失敗");
        let second = create_download_temp(&dest).expect("2個目の一時ファイル作成失敗");

        assert_ne!(first, second);
        assert!(first.exists());
        assert!(second.exists());
        assert!(!dest.exists());

        std::fs::remove_file(first).unwrap();
        std::fs::remove_file(second).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn ensure_writable_は書き込み不可ディレクトリでpermission_deniedを返す() {
        use std::os::unix::fs::PermissionsExt;
        // root は DAC を無視して書けるためスキップする。
        if unsafe { libc_geteuid() } == 0 {
            return;
        }
        let tmp = tempfile::tempdir().expect("tempdir 作成失敗");
        let mut perms = std::fs::metadata(tmp.path()).unwrap().permissions();
        perms.set_mode(0o555);
        std::fs::set_permissions(tmp.path(), perms).unwrap();
        let err = ensure_writable(tmp.path()).expect_err("書き込み不可なのに Ok");
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        // tempdir の削除のために書き込み権限を戻す。
        let mut restore = std::fs::metadata(tmp.path()).unwrap().permissions();
        restore.set_mode(0o755);
        std::fs::set_permissions(tmp.path(), restore).unwrap();
    }

    #[cfg(unix)]
    unsafe fn libc_geteuid() -> u32 {
        // libc クレートなしで euid を取得する（unistd 直接呼び出し）。
        unsafe extern "C" {
            fn geteuid() -> u32;
        }
        unsafe { geteuid() }
    }
}
