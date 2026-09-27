use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MAX_PROMPT_BLANK_LINES: usize = 10;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub prompt: PromptConfig,
    #[serde(default)]
    pub suggest: SuggestConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub shell: ShellConfig,
    #[serde(default)]
    pub completions: Vec<crate::completion::Registration>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
    pub language: UiLanguage,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UiLanguage {
    #[default]
    Auto,
    En,
    Ja,
}

impl<'de> Deserialize<'de> for UiLanguage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "en" => Self::En,
            "ja" => Self::Ja,
            _ => Self::Auto,
        })
    }
}

impl UiLanguage {
    pub fn next(self) -> Self {
        match self {
            Self::Auto => Self::En,
            Self::En => Self::Ja,
            Self::Ja => Self::Auto,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Auto => Self::Ja,
            Self::En => Self::Auto,
            Self::Ja => Self::En,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PromptConfig {
    #[serde(default = "default_prompt_style")]
    pub prompt_style: String,
    #[serde(default = "default_true")]
    pub newline: bool,
    #[serde(default)]
    pub blank_lines: usize,
    #[serde(default = "default_font_level")]
    pub font_level: String,
    #[serde(default = "default_left_segments")]
    pub left_segments: Vec<String>,
    #[serde(default = "default_right_segments")]
    pub right_segments: Vec<String>,
    #[serde(default)]
    pub dir: DirConfig,
    #[serde(default)]
    pub git: GitConfig,
    #[serde(default)]
    pub transient: bool,
    #[serde(default)]
    pub custom: Vec<CustomSegment>,
    #[serde(default)]
    pub ip_interface: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellConfig {
    pub completion_dirs: String,
    pub term_shell_integration: String,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            completion_dirs: String::new(),
            term_shell_integration: "auto".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSegment {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default = "default_custom_fg")]
    pub fg: String,
    #[serde(default = "default_custom_bg")]
    pub bg: String,
    #[serde(default = "default_when_always")]
    pub when: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DirConfig {
    #[serde(default = "default_truncation_length")]
    pub truncation_length: usize,
    #[serde(default = "default_truncation_symbol")]
    pub truncation_symbol: String,
    #[serde(default = "default_home_symbol")]
    pub home_symbol: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GitConfig {
    #[serde(default = "default_true")]
    pub show_ahead_behind: bool,
    #[serde(default = "default_true")]
    pub show_stash: bool,
    #[serde(default = "default_true")]
    pub show_status: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SuggestConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_highlight_color")]
    pub highlight_color: String,
    #[serde(default = "default_max_suggestions")]
    pub max_suggestions: usize,
    #[serde(default)]
    pub keys: SuggestKeys,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestKeyAction {
    Default,
    Full,
    Step,
    Word,
}

impl SuggestKeyAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Full => "full",
            Self::Step => "step",
            Self::Word => "word",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SuggestKeys {
    pub tab: SuggestKeyAction,
    pub right: SuggestKeyAction,
    pub alt_f: SuggestKeyAction,
    pub ctrl_right: SuggestKeyAction,
}

impl Default for SuggestKeys {
    fn default() -> Self {
        Self {
            tab: SuggestKeyAction::Full,
            right: SuggestKeyAction::Step,
            alt_f: SuggestKeyAction::Word,
            ctrl_right: SuggestKeyAction::Word,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StyleConfig {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub rainbow_overrides: BTreeMap<String, SegmentColors>,
    #[serde(default = "default_rainbow_palette")]
    pub rainbow_palette: String,
    #[serde(default = "default_primary_color")]
    pub primary_color: String,
    #[serde(default = "default_success_color")]
    pub success_color: String,
    #[serde(default = "default_error_color")]
    pub error_color: String,
    #[serde(default = "default_muted_color")]
    pub muted_color: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SegmentColors {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fg: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bg: Option<String>,
}

fn default_rainbow_palette() -> String {
    "default".into()
}

fn default_prompt_style() -> String {
    "lean".into()
}
fn default_font_level() -> String {
    "unicode".into()
}
fn default_left_segments() -> Vec<String> {
    vec!["dir".into(), "git".into()]
}
fn default_right_segments() -> Vec<String> {
    vec!["duration".into(), "status".into(), "time".into()]
}
fn default_truncation_length() -> usize {
    3
}
fn default_truncation_symbol() -> String {
    "\u{2026}".into()
}
fn default_home_symbol() -> String {
    "~".into()
}
fn default_true() -> bool {
    true
}
fn default_custom_fg() -> String {
    "white".into()
}
fn default_custom_bg() -> String {
    "238".into()
}
fn default_when_always() -> String {
    "always".into()
}
fn default_strategy() -> String {
    "prefix".into()
}
fn default_highlight_color() -> String {
    "fg=8".into()
}
fn default_max_suggestions() -> usize {
    10
}
fn default_primary_color() -> String {
    "blue".into()
}
fn default_success_color() -> String {
    "green".into()
}
fn default_error_color() -> String {
    "red".into()
}
fn default_muted_color() -> String {
    "bright_black".into()
}

impl Default for PromptConfig {
    fn default() -> Self {
        Self {
            prompt_style: default_prompt_style(),
            newline: true,
            blank_lines: 0,
            font_level: default_font_level(),
            left_segments: default_left_segments(),
            right_segments: default_right_segments(),
            dir: DirConfig::default(),
            git: GitConfig::default(),
            transient: false,
            custom: Vec::new(),
            ip_interface: String::new(),
        }
    }
}

impl Default for DirConfig {
    fn default() -> Self {
        Self {
            truncation_length: default_truncation_length(),
            truncation_symbol: default_truncation_symbol(),
            home_symbol: default_home_symbol(),
        }
    }
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            show_ahead_behind: true,
            show_stash: true,
            show_status: true,
        }
    }
}

impl Default for SuggestConfig {
    fn default() -> Self {
        Self {
            strategy: default_strategy(),
            highlight_color: default_highlight_color(),
            max_suggestions: default_max_suggestions(),
            keys: SuggestKeys::default(),
        }
    }
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            rainbow_overrides: BTreeMap::new(),
            rainbow_palette: default_rainbow_palette(),
            primary_color: default_primary_color(),
            success_color: default_success_color(),
            error_color: default_error_color(),
            muted_color: default_muted_color(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    config_base_from_xdg(std::env::var_os("XDG_CONFIG_HOME")).join("zsh-turbo")
}

fn config_base_from_xdg(xdg_config_home: Option<OsString>) -> PathBuf {
    // XDG_CONFIG_HOME は未設定または空文字列なら無効として扱う。
    if let Some(value) = xdg_config_home
        && !value.is_empty()
    {
        return PathBuf::from(value);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn load_config() -> Config {
    load_config_from(&config_path())
}

pub fn load_config_strict() -> std::io::Result<Config> {
    load_config_strict_from(&config_path())
}

fn load_config_strict_from(path: &Path) -> std::io::Result<Config> {
    match std::fs::read_to_string(path) {
        Ok(content) => toml::from_str(&content).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}: {e}", path.display()),
            )
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod strict_tests {
    use super::*;

    #[test]
    fn invalid_config_is_not_replaced_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[ui\nlanguage = 'ja'").unwrap();
        assert!(load_config_strict_from(&path).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[ui\nlanguage = 'ja'"
        );
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            load_config_strict_from(&path).unwrap().ui.language,
            UiLanguage::Auto
        );
        let unknown: Config = toml::from_str("[ui]\nlanguage = 'fr'").unwrap();
        assert_eq!(unknown.ui.language, UiLanguage::Auto);
    }
}

/// 指定パスから Config を読み込む。テスト容易性のため読み込み元を引数化した内部実装
/// (実環境の config に依存しない決定的なテストを可能にする)。
fn load_config_from(path: &std::path::Path) -> Config {
    if path.exists() {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("zsh-turbo: warning: failed to read {}: {e}", path.display());
                return Config::default();
            }
        };
        match toml::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "zsh-turbo: warning: failed to parse {}: {e}",
                    path.display()
                );
                Config::default()
            }
        }
    } else {
        Config::default()
    }
}

pub fn save_config(config: &Config) -> std::io::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    save_config_to(config, &config_path())
}

/// `dest` へ Config を書き込む。アトミック書き込みのテスト容易性のため
/// 保存先を引数化した内部実装。
fn save_config_to(config: &Config, dest: &Path) -> std::io::Result<()> {
    // シリアライズ失敗時に空文字列で既存設定を上書きしないよう、エラーは呼び出し元へ返す。
    let content = toml::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let (tmp, mut file) = create_config_temp(dest)?;
    if let Err(error) = file
        .write_all(content.as_bytes())
        .and_then(|()| file.sync_all())
    {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    drop(file);

    if let Err(error) = std::fs::rename(&tmp, dest) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

static CONFIG_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn create_config_temp(dest: &Path) -> std::io::Result<(PathBuf, File)> {
    let parent = dest.parent().unwrap_or_else(|| Path::new("."));
    let file_name = dest
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();

    for _ in 0..100 {
        let sequence = CONFIG_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(
            ".{file_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&tmp) {
            Ok(file) => return Ok((tmp, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "設定ファイル用の一時ファイルを作成できませんでした",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_no_config_temp_files(dir: &Path) {
        let remaining: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".config.toml.tmp-")
            })
            .map(|entry| entry.path())
            .collect();
        assert!(
            remaining.is_empty(),
            "設定保存後に一時ファイルが残っている: {remaining:?}"
        );
    }

    #[test]
    fn 旧設定の配色と改行を維持し新設定は保存後も保持する() {
        let legacy: Config = toml::from_str("[prompt]\nprompt_style = 'rainbow'\n").unwrap();
        assert!(legacy.prompt.newline);
        assert_eq!(legacy.style.rainbow_palette, "default");
        let mut changed = legacy;
        changed.prompt.newline = false;
        changed.style.rainbow_palette = "green".into();
        let saved = toml::to_string(&changed).unwrap();
        let loaded: Config = toml::from_str(&saved).unwrap();
        assert!(!loaded.prompt.newline);
        assert_eq!(loaded.style.rainbow_palette, "green");
    }

    // --- config_dir / config_path のパス検証 ---

    #[test]
    fn config_dir_は_zsh_turboで終わる() {
        let dir = config_dir();
        assert!(
            dir.ends_with("zsh-turbo"),
            "config_dir が zsh-turbo で終わっていない: {dir:?}"
        );
    }

    #[test]
    fn config_path_は_config_tomlで終わる() {
        let path = config_path();
        assert!(
            path.ends_with("config.toml"),
            "config_path が config.toml で終わっていない: {path:?}"
        );
    }

    #[test]
    fn config_path_は_config_dir配下にある() {
        let dir = config_dir();
        let path = config_path();
        assert!(
            path.starts_with(&dir),
            "config_path が config_dir 配下にない: dir={dir:?}, path={path:?}"
        );
    }

    #[test]
    fn config_base_from_xdg_指定値を優先する() {
        let base = config_base_from_xdg(Some(OsString::from("/tmp/zsh-turbo-config")));
        assert_eq!(base, PathBuf::from("/tmp/zsh-turbo-config"));
    }

    #[test]
    fn config_base_from_xdg_空文字列は_defaultへフォールバックする() {
        let base = config_base_from_xdg(Some(OsString::new()));
        assert!(
            base.ends_with(".config"),
            "空の XDG_CONFIG_HOME は ~/.config 相当へフォールバックする必要がある: {base:?}"
        );
        assert!(
            !base.as_os_str().is_empty(),
            "空の XDG_CONFIG_HOME から空パスを採用してはならない"
        );
    }

    // --- 存在しないパスで load_config がデフォルトを返す ---

    #[test]
    fn load_config_は存在しないパスでデフォルトを返す() {
        // 実環境の config に依存しないよう、一時ディレクトリ内の存在しないパスで検証する
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("no-such-config.toml");
        let config = load_config_from(&missing);
        let default_config = Config::default();
        // デフォルト値と一致することを確認
        assert_eq!(
            config.prompt.prompt_style,
            default_config.prompt.prompt_style
        );
        assert_eq!(config.suggest.strategy, default_config.suggest.strategy);
        assert_eq!(
            config.style.primary_color,
            default_config.style.primary_color
        );
    }

    #[test]
    fn load_config_from_は既存ファイルの値を読み込む() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[prompt]\nprompt_style = \"classic\"\n").unwrap();
        let config = load_config_from(&path);
        assert_eq!(config.prompt.prompt_style, "classic");
    }

    #[test]
    fn load_config_from_は壊れたtomlでデフォルトを返す() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "prompt_style = [broken").unwrap();
        let config = load_config_from(&path);
        assert_eq!(config.prompt.prompt_style, "lean");
    }

    // --- Config のデフォルト値検証 ---

    #[test]
    fn config_デフォルトはすべてのフィールドがデフォルト() {
        let config = Config::default();
        // 各サブ構造体がデフォルトであることを確認
        assert_eq!(config.prompt.prompt_style, "lean");
        assert_eq!(config.suggest.strategy, "prefix");
        assert_eq!(config.style.primary_color, "blue");
    }

    // --- PromptConfig のデフォルト値 ---

    #[test]
    fn prompt_config_デフォルト値() {
        let p = PromptConfig::default();
        assert_eq!(p.prompt_style, "lean");
        assert_eq!(p.font_level, "unicode");
        assert_eq!(p.left_segments, vec!["dir", "git"]);
        assert_eq!(p.right_segments, vec!["duration", "status", "time"]);
        assert!(!p.transient, "transient はデフォルトで false");
        assert!(p.custom.is_empty(), "custom はデフォルトで空");
    }

    // --- DirConfig のデフォルト値 ---

    #[test]
    fn dir_config_デフォルト値() {
        let d = DirConfig::default();
        assert_eq!(d.truncation_length, 3);
        assert_eq!(d.truncation_symbol, "\u{2026}"); // …
        assert_eq!(d.home_symbol, "~");
    }

    // --- GitConfig のデフォルト値 ---

    #[test]
    fn git_config_デフォルト値() {
        let g = GitConfig::default();
        assert!(g.show_ahead_behind);
        assert!(g.show_stash);
        assert!(g.show_status);
    }

    // --- SuggestConfig のデフォルト値 ---

    #[test]
    fn suggest_config_デフォルト値() {
        let s = SuggestConfig::default();
        assert_eq!(s.strategy, "prefix");
        assert_eq!(s.highlight_color, "fg=8");
        assert_eq!(s.max_suggestions, 10);
        assert_eq!(s.keys.tab, SuggestKeyAction::Full);
        assert_eq!(s.keys.right, SuggestKeyAction::Step);
        assert_eq!(s.keys.alt_f, SuggestKeyAction::Word);
        assert_eq!(s.keys.ctrl_right, SuggestKeyAction::Word);
    }

    #[test]
    fn 旧設定でキー操作を補う() {
        let config: Config = toml::from_str("[suggest]\nstrategy = 'fuzzy'\n").unwrap();
        assert_eq!(config.suggest.keys.tab, SuggestKeyAction::Full);
        assert_eq!(config.suggest.keys.right, SuggestKeyAction::Step);
    }

    #[test]
    fn suggest_highlight_color_既定値は_region_highlight_形式() {
        let s = SuggestConfig::default();
        assert!(
            s.highlight_color.starts_with("fg="),
            "region_highlight に渡す既定値は zsh のスタイル形式である必要がある: {}",
            s.highlight_color
        );
    }

    // --- StyleConfig のデフォルト値 ---

    #[test]
    fn style_config_デフォルト値() {
        let s = StyleConfig::default();
        assert_eq!(s.primary_color, "blue");
        assert_eq!(s.success_color, "green");
        assert_eq!(s.error_color, "red");
        assert_eq!(s.muted_color, "bright_black");
    }

    // --- TOML デシリアライズで空文字列からデフォルトが設定される ---

    #[test]
    fn 空のtomlからデフォルト値が設定される() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.prompt.prompt_style, "lean");
        assert_eq!(config.suggest.strategy, "prefix");
        assert_eq!(config.style.primary_color, "blue");
    }

    // --- TOML デシリアライズで部分指定が動作する ---

    #[test]
    fn 部分指定のtomlで未指定フィールドはデフォルト() {
        let toml_str = r#"
[prompt]
prompt_style = "classic"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        // 指定した値が反映される
        assert_eq!(config.prompt.prompt_style, "classic");
        // 未指定のフィールドはデフォルト
        assert_eq!(config.prompt.font_level, "unicode");
        assert_eq!(config.suggest.strategy, "prefix");
        assert_eq!(config.style.primary_color, "blue");
    }

    // --- save_config の書き込みテスト ---

    #[test]
    fn save_configで保存した設定をload可能() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("zsh-turbo");
        let config_file = config_dir.join("config.toml");

        // テスト用の設定
        let mut config = Config::default();
        config.prompt.prompt_style = "classic".into();
        config.style.primary_color = "cyan".into();

        // 直接ファイルに書き込む（save_config は固定パスを使うため）
        std::fs::create_dir_all(&config_dir).unwrap();
        let content = toml::to_string_pretty(&config).unwrap();
        std::fs::write(&config_file, &content).unwrap();

        // 書き戻した内容をパースして一致確認
        let loaded: Config =
            toml::from_str(&std::fs::read_to_string(&config_file).unwrap()).unwrap();
        assert_eq!(loaded.prompt.prompt_style, "classic");
        assert_eq!(loaded.style.primary_color, "cyan");
        // 未変更フィールドはデフォルト
        assert_eq!(loaded.prompt.font_level, "unicode");
    }

    #[test]
    fn config_tomlシリアライズのラウンドトリップ() {
        let config = Config::default();
        let serialized = toml::to_string_pretty(&config).unwrap();
        let deserialized: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(deserialized.prompt.prompt_style, config.prompt.prompt_style);
        assert_eq!(deserialized.prompt.font_level, config.prompt.font_level);
        assert_eq!(deserialized.suggest.strategy, config.suggest.strategy);
        assert_eq!(deserialized.style.primary_color, config.style.primary_color);
    }

    #[test]
    fn config_round_trip_transientとカスタムセグメントを保持する() {
        // transient(bool) は dir/git サブテーブルの後に定義され、custom は
        // array-of-tables。TOML はスカラを table より前へ整列するため、
        // シリアライズ順序で値が失われないことを回帰確認する。
        let mut config = Config::default();
        config.prompt.transient = true;
        config.prompt.custom = vec![CustomSegment {
            name: "weather".into(),
            command: "curl -s wttr.in".into(),
            icon: "icon".into(),
            fg: "white".into(),
            bg: "blue".into(),
            when: "always".into(),
        }];

        let serialized = toml::to_string_pretty(&config).unwrap();
        let loaded: Config = toml::from_str(&serialized).unwrap();

        assert!(
            loaded.prompt.transient,
            "transient が round-trip で失われた"
        );
        assert_eq!(loaded.prompt.custom.len(), 1);
        assert_eq!(loaded.prompt.custom[0].name, "weather");
        assert_eq!(loaded.prompt.custom[0].command, "curl -s wttr.in");
        assert_eq!(loaded.prompt.custom[0].fg, "white");
        assert_eq!(loaded.prompt.custom[0].when, "always");
    }

    #[test]
    fn save_config_to_は新規ファイルを書き込み一時ファイルを残さない() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("config.toml");
        let cfg = Config::default();

        save_config_to(&cfg, &dest).expect("新規保存は成功する");
        assert!(dest.exists(), "保存先ファイルが存在する");

        assert_no_config_temp_files(tmp.path());

        // 書かれた内容がパース可能であること
        let content = std::fs::read_to_string(&dest).unwrap();
        let _: Config = toml::from_str(&content).expect("書き込み内容がパース可能");
    }

    #[test]
    fn save_config_to_は既存ファイルをアトミックに置換する() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("config.toml");

        // 既存設定として有効な TOML を置く
        let mut existing = Config::default();
        existing.prompt.transient = false;
        save_config_to(&existing, &dest).unwrap();

        // 上書き保存
        let mut updated = Config::default();
        updated.prompt.transient = true;
        save_config_to(&updated, &dest).expect("上書き保存は成功する");

        // 上書き後の内容が更新されていること
        let content = std::fs::read_to_string(&dest).unwrap();
        let loaded: Config = toml::from_str(&content).unwrap();
        assert!(loaded.prompt.transient, "上書きが反映されていない");

        assert_no_config_temp_files(tmp.path());
    }

    #[test]
    fn save_config_to_は同じ保存先への並行書き込みを分離する() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("config.toml");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
        let mut handles = Vec::new();

        for writer in 0..16 {
            let dest = dest.clone();
            let barrier = std::sync::Arc::clone(&barrier);
            handles.push(std::thread::spawn(move || {
                let mut config = Config::default();
                config.prompt.dir.home_symbol = format!("writer-{writer}");
                barrier.wait();
                save_config_to(&config, &dest)
            }));
        }

        for handle in handles {
            handle.join().unwrap().expect("並行保存はすべて成功する");
        }

        let content = std::fs::read_to_string(&dest).unwrap();
        let loaded: Config = toml::from_str(&content).unwrap();
        assert!(
            loaded.prompt.dir.home_symbol.starts_with("writer-"),
            "いずれかの完全な設定が保存される"
        );
        assert_no_config_temp_files(tmp.path());
    }

    #[test]
    fn save_config_to_は不正な保存先でエラーを返し既存ファイルを壊さない() {
        let tmp = tempfile::tempdir().unwrap();
        // 親ディレクトリを存在させないことで rename を失敗させる
        let dest = tmp.path().join("missing").join("config.toml");
        let cfg = Config::default();
        let result = save_config_to(&cfg, &dest);
        assert!(
            result.is_err(),
            "存在しないディレクトリへの書き込みはエラー"
        );
        assert!(!dest.exists(), "保存先ファイルが作られていない");
        assert_no_config_temp_files(tmp.path());
    }
}
