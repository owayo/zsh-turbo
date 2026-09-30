use clap::{CommandFactory, Parser, Subcommand, ValueHint};

mod completion;
mod config;
mod directory_history;
mod doctor;
mod font_install;
mod font_wizard;
mod highlight;
mod icons;
mod path_candidates;
mod project_tasks;
mod prompt;
mod state;
mod style;
mod suggest;
mod task_usage;
mod tui;
mod ui_list;

#[derive(Parser)]
#[command(
    name = "zsh-turbo",
    version,
    about = "High-performance zsh enhancement tool"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// zsh 初期化スクリプトを出力する
    Init,
    /// 登録した CLI の補完キャッシュを更新する
    CompletionRefresh {
        #[arg(long)]
        force: bool,
    },
    /// プロンプト文字列を描画する
    Prompt {
        /// 直前コマンドの終了ステータス
        #[arg(long, default_value = "0")]
        last_status: i32,
        /// 直前コマンドの実行時間（ミリ秒）
        #[arg(long, default_value = "0")]
        duration_ms: u64,
        /// プロンプトの表示側（both は左右を NUL 区切りで出力）
        #[arg(long, default_value = "left")]
        side: String,
        /// バックグラウンドジョブ数
        #[arg(long, default_value = "0")]
        jobs: usize,
    },
    /// 現在の入力に対する自動候補を取得する
    Suggest {
        /// 現在の入力プレフィックス（zsh から渡る `$BUFFER` は `-` 始まりも許容する）
        #[arg(allow_hyphen_values = true)]
        prefix: String,
        /// zsh 履歴ファイルのパス
        #[arg(long, allow_hyphen_values = true, value_hint = ValueHint::FilePath)]
        history_file: Option<String>,
        /// ディレクトリごとの履歴を使う (シェル連携用)
        #[arg(long, hide = true)]
        directory_history: bool,
        /// 検索戦略（prefix, substring, fuzzy）
        #[arg(long, allow_hyphen_values = true)]
        strategy: Option<String>,
        /// プロジェクトのタスク候補を一覧用プロトコルで返す (旧シェル連携との互換用)
        #[arg(long, hide = true)]
        project_list: bool,
        /// ZLE の候補一覧 (タスク・ファイル・履歴) を行プロトコルで返す
        #[arg(long, hide = true, conflicts_with = "project_list")]
        ui_list: bool,
        /// 一覧の表示名を収める端末の桁数（0 は切り詰めない）
        #[arg(long, hide = true, default_value_t = 0, requires = "ui_list")]
        columns: usize,
        /// 直前に実行された ZLE widget 名
        #[arg(
            long,
            hide = true,
            default_value = "",
            allow_hyphen_values = true,
            requires = "ui_list"
        )]
        last_widget: String,
        /// ↓ で開いたメニューの中で絞り込んでいる (打ち切った名前の項目も含めて選ぶ)
        #[arg(long, hide = true, requires = "ui_list")]
        menu: bool,
        /// メニューで直前に選んでいた項目の BUFFER
        #[arg(long, hide = true, allow_hyphen_values = true, requires = "ui_list")]
        selected: Option<String>,
    },
    /// 履歴ベースの補完候補を一覧表示する
    Complete {
        /// 現在の入力プレフィックス（zsh から渡る `$BUFFER` は `-` 始まりも許容する）
        #[arg(allow_hyphen_values = true)]
        prefix: String,
        /// zsh 履歴ファイルのパス
        #[arg(long, allow_hyphen_values = true, value_hint = ValueHint::FilePath)]
        history_file: Option<String>,
        /// ディレクトリごとの履歴を使う (シェル連携用)
        #[arg(long, hide = true)]
        directory_history: bool,
        /// 検索戦略（prefix, substring, fuzzy）
        #[arg(long, allow_hyphen_values = true)]
        strategy: Option<String>,
        /// 返す補完候補の最大数
        #[arg(long)]
        max: Option<usize>,
        /// カレントディレクトリのタスク候補だけを返す
        #[arg(long)]
        project_only: bool,
    },
    /// 実行した行を標準入力から受け取り、ディレクトリごとの履歴とタスクの利用を記録する
    #[command(hide = true)]
    Record,
    /// 対話型設定ウィザードを起動する
    Configure,
    /// 端末のフォントとリガチャ設定を対話形式で選ぶ
    Font,
    /// MesloLGS NF (Nerd Font) をユーザフォントディレクトリへインストールする
    InstallFont {
        /// 既存ファイルがあっても上書きする
        #[arg(long)]
        force: bool,
    },
    /// 端末・フォント・ツール・設定を診断する
    Doctor,
    /// コマンド入力をシンタックスハイライトする（"start end style" 行を返す）
    Highlight {
        /// ハイライト対象のコマンド入力（zsh の `$BUFFER` は `-` 始まりも許容する）
        #[arg(allow_hyphen_values = true)]
        buffer: String,
        /// zsh から渡す改行区切りのエイリアス一覧
        #[arg(long, default_value = "", allow_hyphen_values = true)]
        aliases: String,
        /// zsh から渡す改行区切りの関数一覧
        #[arg(long, default_value = "", allow_hyphen_values = true)]
        functions: String,
    },
}

/// 任意の文字列を POSIX シェル向けに single-quote で安全に囲む。
/// 内部のシングルクォートは `'\''` で閉じ→エスケープ→再開の3段で表現する。
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn suggest_highlight_style(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "fg=8".into();
    }
    if trimmed.contains('=') || trimmed.contains(',') {
        return trimmed.into();
    }

    let color = match trimmed.to_ascii_lowercase().as_str() {
        "bright_black" | "gray" | "grey" => "8",
        "bright_red" => "9",
        "bright_green" => "10",
        "bright_yellow" => "11",
        "bright_blue" => "12",
        "bright_magenta" => "13",
        "bright_cyan" => "14",
        "bright_white" => "15",
        "black" => "black",
        "red" => "red",
        "green" => "green",
        "yellow" => "yellow",
        "blue" => "blue",
        "magenta" => "magenta",
        "cyan" => "cyan",
        "white" => "white",
        _ => trimmed,
    };
    format!("fg={color}")
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::CompletionRefresh { force } => {
            for result in completion::refresh_all(&config::load_config().completions, force) {
                println!("{result}");
            }
        }
        Commands::Init => {
            let cfg = config::load_config();
            print!("{}", render_init(&cfg));
        }
        Commands::Prompt {
            last_status,
            duration_ms,
            side,
            jobs,
        } => {
            let config = config::load_config();
            if side == "both" {
                let (left, right) = std::thread::scope(|scope| {
                    let right = scope.spawn(|| {
                        prompt::render_prompt(&config, last_status, duration_ms, "right", jobs)
                    });
                    let left =
                        prompt::render_prompt(&config, last_status, duration_ms, "left", jobs);
                    (left, right.join().unwrap_or_default())
                });
                print!("{left}\0{right}\0");
            } else {
                let output = prompt::render_prompt(&config, last_status, duration_ms, &side, jobs);
                print!("{output}");
            }
        }
        Commands::Suggest {
            prefix,
            history_file,
            directory_history,
            strategy,
            project_list,
            ui_list,
            columns,
            last_widget,
            menu,
            selected,
        } => {
            let config = config::load_config();
            let history_file =
                if directory_history || history_file.as_deref().is_none_or(str::is_empty) {
                    let Some(path) = directory_history::path() else {
                        if ui_list {
                            print!("v1\tnone\t0\tcomplete\t0\t\nend\n");
                        }
                        return;
                    };
                    Some(path.to_string_lossy().into_owned())
                } else {
                    history_file
                };
            let strat = suggest::Strategy::from_str(
                strategy.as_deref().unwrap_or(&config.suggest.strategy),
            );
            let task_usage = config
                .suggest
                .record_task_usage
                .then(task_usage::store_path)
                .flatten();
            if ui_list {
                print!(
                    "{}",
                    ui_list::response(&ui_list::Request {
                        buffer: &prefix,
                        history_file: history_file.as_deref(),
                        strategy: &strat,
                        max: config.suggest.max_suggestions,
                        columns,
                        last_widget: &last_widget,
                        language: config.ui.language,
                        task_usage: task_usage.as_deref(),
                        menu,
                        selected: selected.as_deref(),
                    })
                );
                return;
            }
            if project_list {
                let max = config.suggest.max_suggestions;
                let project = project_tasks::candidates(&prefix, max);
                if !project.is_empty() {
                    println!("project");
                    for candidate in project {
                        println!("{candidate}");
                    }
                    return;
                }
                println!("history");
                if let Some(suggestion) =
                    suggest::get_history_suggestion(&prefix, history_file.as_deref(), &strat)
                {
                    print!("{suggestion}");
                }
                return;
            }
            if let Some(suggestion) = suggest::get_suggestion(
                &prefix,
                history_file.as_deref(),
                &strat,
                task_usage.as_deref(),
            ) {
                print!("{suggestion}");
            }
        }
        Commands::Record => {
            use std::io::Read as _;
            // 設定で無効にした後も、再起動前のシェルから呼ばれ得るためここでも確かめる。
            // 設定が読めないときは既定 (有効) に戻さず、記録しない
            let Ok(config) = config::load_config_strict() else {
                return;
            };
            let mut line = String::new();
            let limit = task_usage::MAX_LINE_BYTES;
            if std::io::stdin()
                .take(limit + 1)
                .read_to_string(&mut line)
                .is_err()
                || line.len() as u64 > limit
            {
                return;
            }
            if let (Ok(cwd), Some(path)) = (std::env::current_dir(), task_usage::store_path()) {
                if config.suggest.record_directory_history {
                    let _ = directory_history::record(&line, &cwd, &path);
                }
                if config.suggest.record_task_usage {
                    let _ = task_usage::record(&line, &cwd, task_usage::now(), &path);
                }
            }
        }
        Commands::Complete {
            prefix,
            history_file,
            directory_history,
            strategy,
            max,
            project_only,
        } => {
            let history_file =
                if directory_history || history_file.as_deref().is_none_or(str::is_empty) {
                    let Some(path) = directory_history::path() else {
                        return;
                    };
                    Some(path.to_string_lossy().into_owned())
                } else {
                    history_file
                };
            let max = max.unwrap_or_else(|| config::load_config().suggest.max_suggestions);
            let completions = if project_only {
                project_tasks::candidates(&prefix, max)
            } else if let Some(strategy) = strategy {
                suggest::get_completions_with_strategy(
                    &prefix,
                    history_file.as_deref(),
                    &suggest::Strategy::from_str(&strategy),
                    max,
                )
            } else {
                suggest::get_completions(&prefix, history_file.as_deref(), max)
            };
            for c in completions {
                println!("{c}");
            }
        }
        Commands::Configure => {
            if let Err(e) = tui::run_tui() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::Font => {
            if let Err(e) = font_wizard::run() {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        }
        Commands::InstallFont { force } => match font_install::install_meslo_nerd_font(force, "\n")
        {
            Ok(result) => {
                println!();
                println!(
                    "Installed {} files (skipped {}). Location: {}",
                    result.installed,
                    result.skipped,
                    result.dest_dir.display()
                );
                println!(
                    "The font files are ready. See the README's Terminal Font Setup before changing your terminal font."
                );
                #[cfg(target_os = "macos")]
                println!(
                    "Terminal.app and iTerm2: select MesloLGS NF in the active profile; Ghostty 1.2+ normally needs no font change."
                );
                println!(
                    "For Nerd Font icons, set Prompt > Font level to Nerd in `zsh-turbo configure`."
                );
            }
            Err(e) => {
                eprintln!("Error: {e}");
                std::process::exit(1);
            }
        },
        Commands::Doctor => {
            doctor::run_doctor();
        }
        Commands::Highlight {
            buffer,
            aliases,
            functions,
        } => {
            let spans = highlight::highlight_buffer(&buffer, &aliases, &functions);
            print!("{}", highlight::format_spans(&spans));
        }
    }
}

fn render_init(cfg: &config::Config) -> String {
    let mut script = String::new();
    // compinit より前に渡し、既存のシェル変数による上書きも維持する。
    for (name, value) in [
        (
            "ZSH_TURBO_TRANSIENT",
            if cfg.prompt.transient { "1" } else { "0" },
        ),
        ("ZSH_TURBO_SUGGEST_STRATEGY", cfg.suggest.strategy.as_str()),
        ("ZSH_TURBO_HISTORY_MENU_HELP", tui::history_menu_help()),
        ("ZSH_TURBO_HISTORY_MENU_EMPTY", tui::history_menu_empty()),
        (
            "ZSH_TURBO_SUGGEST_HIGHLIGHT",
            &suggest_highlight_style(&cfg.suggest.highlight_color),
        ),
        ("ZSH_TURBO_KEY_TAB", cfg.suggest.keys.tab.as_str()),
        ("ZSH_TURBO_KEY_RIGHT", cfg.suggest.keys.right.as_str()),
        ("ZSH_TURBO_KEY_ALT_F", cfg.suggest.keys.alt_f.as_str()),
        (
            "ZSH_TURBO_KEY_CTRL_RIGHT",
            cfg.suggest.keys.ctrl_right.as_str(),
        ),
        (
            "ZSH_TURBO_COMPLETION_DIRS",
            cfg.shell.completion_dirs.as_str(),
        ),
        (
            "ZSH_TURBO_TERM_SHELL_INTEGRATION",
            cfg.shell.term_shell_integration.as_str(),
        ),
        (
            "ZSH_TURBO_RECORD_TASK_USAGE",
            if cfg.suggest.record_task_usage {
                "1"
            } else {
                "0"
            },
        ),
        (
            "ZSH_TURBO_RECORD_DIRECTORY_HISTORY",
            if cfg.suggest.record_directory_history {
                "1"
            } else {
                "0"
            },
        ),
    ] {
        script.push_str(&format!(
            "if [[ -z ${{{name}+x}} ]]; then\n  typeset -g {name}={}\nfi\n",
            shell_single_quote(value)
        ));
    }
    let commands: Vec<String> = project_tasks::COMMANDS
        .iter()
        .map(|command| shell_single_quote(command))
        .collect();
    script.push_str(&format!(
        "typeset -ga _ZSH_TURBO_TASK_COMMANDS=({})\n",
        commands.join(" ")
    ));
    script.push_str(include_str!("../shell/init.zsh"));
    let mut completion = Vec::new();
    clap_complete::generate(
        clap_complete::aot::Zsh,
        &mut Cli::command(),
        "zsh-turbo",
        &mut completion,
    );
    script.push_str("\n() {\n emulate -L zsh\n");
    script.push_str(&String::from_utf8(completion).expect("completion script is UTF-8"));
    script.push_str("\n}\n");
    script.push_str(include_str!("../shell/project_completion.zsh"));
    script.push_str(&completion::shell_init(&cfg.completions));
    script
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn count_bytes(haystack: &[u8], needle: &[u8]) -> usize {
        haystack
            .windows(needle.len())
            .filter(|window| *window == needle)
            .count()
    }

    #[test]
    fn shell_single_quote_は通常文字列をクオートする() {
        assert_eq!(shell_single_quote("fg=8"), "'fg=8'");
    }

    #[test]
    fn shell_single_quote_は空文字列を扱える() {
        assert_eq!(shell_single_quote(""), "''");
    }

    #[test]
    fn shell_single_quote_はsingle_quoteをエスケープする() {
        // ' を含む文字列は閉じる→ \' →再開する 3 段の構造になる
        assert_eq!(shell_single_quote("a'b"), "'a'\\''b'");
    }

    #[test]
    fn shell_single_quote_はメタ文字を素通りさせる() {
        // クオート内では `;` `$` `&` などは展開されない
        let injected = "fg=8; print PWNED >&2";
        let quoted = shell_single_quote(injected);
        // 元の文字列がクオート内に取り込まれる
        assert!(quoted.starts_with('\''));
        assert!(quoted.ends_with('\''));
        assert!(quoted.contains("fg=8; print PWNED >&2"));
    }

    #[test]
    fn shell_single_quote_は連続するシングルクォートを扱える() {
        assert_eq!(shell_single_quote("a''b"), "'a'\\'''\\''b'");
    }

    #[test]
    fn suggest_highlight_style_は既存の_region_highlight_形式を維持する() {
        assert_eq!(suggest_highlight_style("fg=8"), "fg=8");
        assert_eq!(suggest_highlight_style("fg=8,bold"), "fg=8,bold");
    }

    #[test]
    fn suggest_highlight_style_は色名を前景色指定へ変換する() {
        assert_eq!(suggest_highlight_style("bright_black"), "fg=8");
        assert_eq!(suggest_highlight_style("red"), "fg=red");
    }

    #[test]
    fn suggest_highlight_style_は空値で安全な既定値を返す() {
        assert_eq!(suggest_highlight_style("  "), "fg=8");
    }

    #[test]
    fn init_script_transient_prompt_は_zsh対応のunicode_escapeを使う() {
        let script = include_str!("../shell/init.zsh");
        // zsh の $'...' では \u{276f} は期待どおりに解釈されない。
        assert!(!script.contains("\\u{276f}"));
        assert!(script.contains("\\u276f"));
        assert!(script.contains("RPROMPT=\"\"\n    _zsh_turbo_apply_semantic_prompt_markers"));
    }

    #[test]
    fn init_script_は_visual_keymapを_vi_modeへ反映する() {
        let script = include_str!("../shell/init.zsh");
        assert!(script.contains("visual|vivis|vivli)"));
        assert!(script.contains("ZSH_TURBO_VI_MODE=\"visual\""));
    }

    #[test]
    fn init_script_は_iterm2_37以降で_semantic_prompt_markerを使う() {
        let script = include_str!("../shell/init.zsh");
        assert!(script.contains("ZSH_TURBO_TERM_SHELL_INTEGRATION"));
        assert!(script.contains("TERM_PROGRAM\" == \"iTerm.app\""));
        assert!(script.contains("case \"$TERM_PROGRAM_VERSION\" in"));
        assert!(script.contains("3.<7->*|<4->.*) return 0"));
        assert!(script.contains(r#""$PROMPT" != *$'\e]133;A\a'*"#));
        assert!(script.contains(r#""$RPROMPT" != *$'\e]133;P;k=r\a'*"#));
        assert!(script.contains(r"\e]133;P;k=r\a"));
        assert!(script.contains(r"\e]133;A;k=s\a"));
        assert!(script.contains("ITERM2_SQUELCH_PS2_MARK"));
    }

    #[cfg(unix)]
    #[test]
    fn init_script_semantic_prompt_marker_は_zsh実行時に重複しない() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = tempfile::tempdir().expect("一時ディレクトリを作成できるべき");
        let fake_bin = tmp.path().join("zsh-turbo");
        std::fs::write(
            &fake_bin,
            r#"#!/bin/sh
if [ "$1" = "prompt" ]; then
  side=left
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--side" ]; then
      shift
      side="$1"
    fi
    shift
  done
  if [ "$side" = "both" ]; then
    printf 'LEFT\000RIGHT\000'
  elif [ "$side" = "right" ]; then
    printf 'RIGHT'
  else
    printf 'LEFT'
  fi
fi
"#,
        )
        .expect("fake zsh-turbo を書き込めるべき");
        let mut perms = std::fs::metadata(&fake_bin)
            .expect("fake zsh-turbo の metadata を読めるべき")
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fake_bin, perms).expect("fake zsh-turbo を実行可能にできるべき");

        let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let zsh_code = format!(
            "setopt SH_GLOB NO_UNSET\nsource {} || exit $?\n_zsh_turbo_apply_semantic_prompt_markers\n_zsh_turbo_apply_semantic_prompt_markers\nprint -r -- \"$PROMPT\"\nprint -r -- \"$RPROMPT\"\nprint -r -- \"$PS2\"\n",
            shell_single_quote(&script.display().to_string())
        );
        let path = format!(
            "{}:{}",
            tmp.path().display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfi", "-c", &zsh_code])
            .env("PATH", path)
            .env("ZDOTDIR", tmp.path())
            .env("TERM_PROGRAM", "iTerm.app")
            .env("TERM_PROGRAM_VERSION", "3.7.0")
            .output()
            .expect("zsh を実行できるべき");

        assert!(
            output.status.success(),
            "zsh 実行が失敗した: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let lines: Vec<&[u8]> = output.stdout.split(|byte| *byte == b'\n').collect();
        assert!(lines.len() >= 3, "PROMPT/RPROMPT/PS2 が出力されるべき");
        assert_eq!(count_bytes(lines[0], b"\x1b]133;A\x07"), 1);
        assert_eq!(count_bytes(lines[0], b"\x1b]133;B\x07"), 1);
        assert_eq!(count_bytes(lines[1], b"\x1b]133;P;k=r\x07"), 1);
        assert_eq!(count_bytes(lines[1], b"\x1b]133;B\x07"), 1);
        assert_eq!(count_bytes(lines[2], b"\x1b]133;A;k=s\x07"), 1);
        assert_eq!(count_bytes(lines[2], b"\x1b]133;B\x07"), 1);
    }

    #[test]
    fn init_script_buffer_prefix_剥がしは_quoted_form_を使う() {
        // zsh の `${VAR#PATTERN}` は右辺をパターンとして解釈するため、
        // `$BUFFER` をそのまま渡すと `[`/`*`/`?` が含まれたときに想定外マッチや
        // パターンエラーを起こす。回帰防止のため quote 済みの形が含まれることを確認する。
        let script = include_str!("../shell/init.zsh");
        assert!(
            script.contains("${_ZSH_TURBO_SUGGESTION#\"$BUFFER\"}"),
            "BUFFER は ${{VAR#PATTERN}} の右辺で必ず quote すること"
        );
        // 旧バグの形 (`#$BUFFER` のように quote されない形) が残っていないことを念のため確認する。
        assert!(
            !script.contains("${_ZSH_TURBO_SUGGESTION#$BUFFER}"),
            "BUFFER を quote しない旧バグの形が残っている"
        );
    }

    #[test]
    fn init_script_clear_suggestion_は_region_highlightも更新する() {
        let script = include_str!("../shell/init.zsh");
        assert!(
            script.contains("function _zsh_turbo_clear_suggestion() {")
                && script.contains(
                    "    _ZSH_TURBO_GHOST_SUFFIX=\"\"\n    _zsh_turbo_autosuggest_display\n}"
                ),
            "サジェスト消去時は POSTDISPLAY だけでなく region_highlight も更新する必要がある"
        );
        assert!(script.contains("_zsh_turbo_clear_suggestion\n    BUFFER=\"${BUFFER}${chunk}\""));
    }

    #[test]
    fn init_script_は行末以外でサジェストを表示も受理もしない() {
        let script = include_str!("../shell/init.zsh");
        assert!(
            script.contains("(( CURSOR == ${#BUFFER} )) || return")
                && script.contains("(( CURSOR != ${#BUFFER} )); then"),
            "表示と受理は CURSOR が BUFFER の末尾にある場合だけ許可すること"
        );
        assert!(
            script.contains("typeset -gi _ZSH_TURBO_LAST_CURSOR=-1")
                && script.contains("_ZSH_TURBO_LAST_CURSOR=$CURSOR")
                && script.contains("_zsh_turbo_autosuggest_display"),
            "BUFFER が同じでも CURSOR 変更時に ghost 表示を更新すること"
        );
    }

    #[test]
    fn init_script_パス候補を階層ごとに受け入れる() {
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let script = format!(
            r#"source {}
function zle() {{ ZLE_CALLED="$1"; }}
BUFFER='ls -l'
CURSOR=${{#BUFFER}}
full='ls -l /path/to/hoge/fuga'
for expected in 'ls -l /path/' 'ls -l /path/to/' 'ls -l /path/to/hoge/' 'ls -l /path/to/hoge/fuga'; do
    POSTDISPLAY="${{full#"$BUFFER"}}"
    _ZSH_TURBO_GHOST_SUFFIX="$POSTDISPLAY"
    region_highlight=("${{#BUFFER}} 100 fg=8")
    _zsh_turbo_accept_right
    [[ "$BUFFER" == "$expected" && $CURSOR == ${{#BUFFER}} && ${{#region_highlight}} == 0 ]] || {{ print -ru2 -- "expected=$expected actual=$BUFFER cursor=$CURSOR highlights=${{(j:,:)region_highlight}}"; exit 1; }}
done
BUFFER='ls -l'
CURSOR=${{#BUFFER}}
POSTDISPLAY=' /path/to/hoge/fuga'
_ZSH_TURBO_GHOST_SUFFIX="$POSTDISPLAY"
_zsh_turbo_accept_tab
[[ "$BUFFER" == "$full" ]] || exit 2
POSTDISPLAY=''
_ZSH_TURBO_GHOST_SUFFIX=''
_zsh_turbo_accept_tab
[[ "$ZLE_CALLED" == expand-or-complete ]] || exit 3
BUFFER='ls -l'
CURSOR=${{#BUFFER}}
POSTDISPLAY=' /path/to'
_ZSH_TURBO_GHOST_SUFFIX="$POSTDISPLAY"
ZSH_TURBO_KEY_TAB=default
ZLE_CALLED=''
_zsh_turbo_accept_tab
[[ "$BUFFER" == 'ls -l' && "$ZLE_CALLED" == expand-or-complete ]] || exit 4
ZSH_TURBO_KEY_RIGHT=full
_zsh_turbo_accept_right
[[ "$BUFFER" == 'ls -l /path/to' ]] || exit 5
_zsh_turbo_suggestion_step '/to/'
[[ "$REPLY" == '/to/' ]] || exit 6
_zsh_turbo_suggestion_step ' relative/child'
[[ "$REPLY" == ' relative/' ]] || exit 7
_zsh_turbo_suggestion_step ' "space name/child"'
[[ "$REPLY" == ' "space name/' ]] || exit 8
"#,
            shell_single_quote(&init_path.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "段階採用に失敗: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn init_script_一覧の応答を解釈して見出しと表示行数を反映する() {
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let script = format!(
            r#"source {}
function zle() {{ :; }}
BUFFER='ls books/'
CURSOR=${{#BUFFER}}
LINES=24
BUFFERLINES=1
tab=$'\t'
_zsh_turbo_apply_response "v1${{tab}}files${{tab}}3${{tab}}complete${{tab}}2${{tab}}Files" \
    "ghost${{tab}}ls books/a\ b.txt " \
    "item${{tab}}a b.txt${{tab}}ls books/a\ b.txt " \
    "item${{tab}}c.txt${{tab}}ls books/c.txt " \
    "item${{tab}}sub/${{tab}}ls books/sub/" end
[[ "$_ZSH_TURBO_LIST_KIND" == files && $_ZSH_TURBO_LIST_TOTAL == 3 && $_ZSH_TURBO_LIST_ROWS == 2 ]] || exit 1
[[ "${{_ZSH_TURBO_LIST_VALUES[1]}}" == 'ls books/a\ b.txt ' && "${{_ZSH_TURBO_LIST_LABELS[3]}}" == sub/ ]] || exit 2
_zsh_turbo_autosuggest_display
# 入力中は既定の行数 (2) まで表示し、続きを … で示す
[[ "$POSTDISPLAY" == $'a\\ b.txt \nFiles (3):\n  a b.txt\n  c.txt\n  …' ]] || {{ print -r -- "$POSTDISPLAY" >&2; exit 3; }}
_zsh_turbo_list_header Files 19 0 3
[[ "$REPLY" == 'Files (3/19):' ]] || exit 4
_zsh_turbo_list_header Files 256 1
[[ "$REPLY" == 'Files (256+):' ]] || exit 5
# 旧形式・壊れた応答では一覧を出さない
_zsh_turbo_apply_response "project" "make build"
(( ${{#_ZSH_TURBO_LIST_VALUES}} == 0 )) && [[ -z "$_ZSH_TURBO_SUGGESTION" ]] || exit 6
_zsh_turbo_apply_response "v1${{tab}}none${{tab}}0${{tab}}complete${{tab}}10${{tab}}" "ghost${{tab}}ls books/x" end
(( ${{#_ZSH_TURBO_LIST_VALUES}} == 0 )) && [[ "$_ZSH_TURBO_SUGGESTION" == 'ls books/x' ]] || exit 7
"#,
            shell_single_quote(&init_path.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "応答の解釈に失敗: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn init_script_選択位置を反映し無効な値や次の応答では先頭に戻す() {
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let script = format!(
            r#"source {}
function zle() {{ :; }}
BUFFER='make'
CURSOR=4
tab=$'\t'
local -a items=("item${{tab}}build${{tab}}make build" "item${{tab}}install${{tab}}make install" "item${{tab}}test${{tab}}make test")
_zsh_turbo_apply_response "v1${{tab}}tasks${{tab}}3${{tab}}complete${{tab}}10${{tab}}Tasks" "ghost${{tab}}make install" "input${{tab}}make " "select${{tab}}2" "${{items[@]}}" end
(( _ZSH_TURBO_LIST_SELECTED == 2 )) && [[ "$_ZSH_TURBO_SUGGESTION" == 'make install' ]] || exit 1
# メニューで打った文字を足す位置 (区切りを補った入力) を受け取り、次の応答では消す
[[ "$_ZSH_TURBO_LIST_INPUT" == 'make ' ]] || exit 4
_zsh_turbo_apply_response "v1${{tab}}none${{tab}}0${{tab}}complete${{tab}}10${{tab}}" end
[[ -z "$_ZSH_TURBO_LIST_INPUT" ]] || exit 5
for bad in 0 4 x '' -1; do
    _zsh_turbo_apply_response "v1${{tab}}tasks${{tab}}3${{tab}}complete${{tab}}10${{tab}}Tasks" "select${{tab}}2" "${{items[@]}}" end
    _zsh_turbo_apply_response "v1${{tab}}tasks${{tab}}3${{tab}}complete${{tab}}10${{tab}}Tasks" "select${{tab}}$bad" "${{items[@]}}" end
    (( _ZSH_TURBO_LIST_SELECTED == 1 )) || {{ print -u2 -- "bad=$bad"; exit 2; }}
done
# select の無い応答 (旧バイナリ) では先頭に戻す
_zsh_turbo_apply_response "v1${{tab}}tasks${{tab}}3${{tab}}complete${{tab}}10${{tab}}Tasks" "select${{tab}}3" "${{items[@]}}" end
_zsh_turbo_apply_response "v1${{tab}}tasks${{tab}}3${{tab}}complete${{tab}}10${{tab}}Tasks" "${{items[@]}}" end
(( _ZSH_TURBO_LIST_SELECTED == 1 )) || exit 3
"#,
            shell_single_quote(&init_path.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "選択位置の解釈に失敗: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    #[test]
    fn init_script_タスクを実行する行だけを標準入力で記録に渡す() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let log = tmp.path().join("log");
        let stub = tmp.path().join("stub");
        std::fs::write(
            &stub,
            format!(
                "#!/bin/sh\n[ \"$1\" = record ] || exit 1\n{{ cat; echo; }} >> {}\n",
                shell_single_quote(&log.display().to_string())
            ),
        )
        .unwrap();
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).unwrap();
        let script = format!(
            r#"source {}
ZSH_TURBO_CMD={}
log={}
_ZSH_TURBO_TASK_COMMANDS=(make npm)
ZSH_TURBO_RECORD_DIRECTORY_HISTORY=0
_zsh_turbo_record_task_usage ' make build'
_zsh_turbo_record_task_usage 'echo make build'
_zsh_turbo_record_task_usage 'makefoo build'
_zsh_turbo_record_task_usage ''
ZSH_TURBO_RECORD_TASK_USAGE=0
_zsh_turbo_record_task_usage 'make check'
ZSH_TURBO_RECORD_TASK_USAGE=1
_zsh_turbo_record_task_usage 'make install PREFIX=~/x'
repeat 200; do [[ -s "$log" ]] && break; sleep 0.05; done
sleep 0.3
[[ "$(<$log)" == 'make install PREFIX=~/x' ]] || {{ cat "$log" >&2; exit 1; }}
"#,
            shell_single_quote(&init_path.display().to_string()),
            shell_single_quote(&stub.display().to_string()),
            shell_single_quote(&log.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "記録に渡す行の選別に失敗: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn init_script_はタスクを実行するコマンドの一覧と記録の設定を渡す() {
        let mut config = config::Config::default();
        let script = render_init(&config);
        assert!(script.contains(
            "typeset -ga _ZSH_TURBO_TASK_COMMANDS=('make' 'just' 'task' 'npm' 'pnpm' 'bun' 'yarn' 'uv' 'deno' 'mise')"
        ));
        assert!(script.contains("typeset -g ZSH_TURBO_RECORD_TASK_USAGE='1'"));
        assert!(script.contains("typeset -g ZSH_TURBO_RECORD_DIRECTORY_HISTORY='1'"));
        config.suggest.record_task_usage = false;
        assert!(render_init(&config).contains("typeset -g ZSH_TURBO_RECORD_TASK_USAGE='0'"));
        config.suggest.record_directory_history = false;
        assert!(render_init(&config).contains("typeset -g ZSH_TURBO_RECORD_DIRECTORY_HISTORY='0'"));
    }

    #[test]
    fn init_script_入力中の一覧はghostの項目をメニューの選択と同じ色で示し窓の外なら見える位置までずらす()
     {
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let script = format!(
            r#"source {}
function zle() {{ :; }}
BUFFER='make'
CURSOR=4
BUFFERLINES=1
tab=$'\t'
local -a lines=("v1${{tab}}tasks${{tab}}14${{tab}}complete${{tab}}10${{tab}}Tasks" "ghost${{tab}}make install" "select${{tab}}8")
for name in build check ci clean fmt fmt-check help install lint release run setup test uninstall; do lines+=("item${{tab}}$name${{tab}}make $name"); done
lines+=(end)
# メニューの選択と同じ色のスパンが当たっている文字列を返す
function selected_text() {{
    local entry text="$BUFFER$POSTDISPLAY"
    local -a span
    REPLY=""
    for entry in "${{region_highlight[@]}}"; do
        span=(${{=entry}})
        [[ "${{span[3]:-}}" == "$_ZSH_TURBO_LIST_SELECTED_STYLE" ]] && REPLY+="${{text[span[1]+1,span[2]]}}"
    done
}}
# 端末に余裕があれば先頭から表示し、ghost の項目をメニューの選択と同じ色 (シアン太字) で示す
[[ "$_ZSH_TURBO_LIST_SELECTED_STYLE" == 'fg=cyan,bold' ]] || exit 7
LINES=30
_zsh_turbo_apply_response "${{lines[@]}}"
_zsh_turbo_autosuggest_display
[[ "$POSTDISPLAY" == $' install\nTasks (14):\n  build\n  check\n  ci\n  clean\n  fmt\n  fmt-check\n  help\n  install\n  lint\n  release\n  …' ]] || {{ print -r -- "$POSTDISPLAY" >&2; exit 1; }}
selected_text
[[ "$REPLY" == install ]] || {{ print -r -- "selected=$REPLY" >&2; exit 2; }}
(( _ZSH_TURBO_LIST_FIRST == 1 )) || exit 2
# 端末が低くて入らないときは、ghost の項目が最下行に来るまでずらし、上下に … を出す
LINES=13
_zsh_turbo_autosuggest_display
[[ "$POSTDISPLAY" == $' install\nTasks (14):\n  …\n  ci\n  clean\n  fmt\n  fmt-check\n  help\n  install\n  …' ]] || {{ print -r -- "$POSTDISPLAY" >&2; exit 3; }}
selected_text
[[ "$REPLY" == install ]] || {{ print -r -- "selected=$REPLY" >&2; exit 4; }}
(( _ZSH_TURBO_LIST_FIRST == 3 )) || exit 4
# select が無い応答 (旧版の CLI や ghost が項目を経由しないとき) では強調せず先頭から出す
_zsh_turbo_apply_response "${{(@)lines:#select*}}"
_zsh_turbo_autosuggest_display
[[ "$POSTDISPLAY" == $' install\nTasks (14):\n  build\n  check\n  ci\n  clean\n  fmt\n  fmt-check\n  help\n  …' ]] || {{ print -r -- "$POSTDISPLAY" >&2; exit 5; }}
selected_text
[[ -z "$REPLY" ]] || exit 6
"#,
            shell_single_quote(&init_path.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "入力中の一覧の窓が ghost の項目を含まない: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn init_script_直前に描いた一覧の行を差し引いて表示行数を決める() {
        // BUFFERLINES は直前に描いた一覧 (POSTDISPLAY) の行も数える。差し引かないと
        // 描くたびに表示行数が増減し、一覧の末尾が出たり消えたりする。
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let script = format!(
            r#"source {}
function zle() {{ :; }}
BUFFER='uv'
CURSOR=2
LINES=20
BUFFERLINES=13
_ZSH_TURBO_DRAWN_LINES=12
_zsh_turbo_edit_lines
(( REPLY == 1 )) || exit 1
tab=$'\t'
local -a lines=("v1${{tab}}commands${{tab}}10${{tab}}complete${{tab}}10${{tab}}Commands")
for name in a b c d e f g h i j; do lines+=("item${{tab}}$name${{tab}}uv $name "); done
lines+=(end)
_zsh_turbo_apply_response "${{lines[@]}}"
_zsh_turbo_autosuggest_display
# 見出し + 10 行をすべて表示する (差し引かないと 20-13-4=3 行に削られる)
[[ ${{#${{POSTDISPLAY//[^$'\n']/}}}} == 11 ]] || {{ print -r -- "$POSTDISPLAY" >&2; exit 2; }}
# Ctrl+C で閉じた入力のままなら一覧を出さない
_ZSH_TURBO_LIST_DISMISSED='uv'
_zsh_turbo_apply_response "${{lines[@]}}"
(( ${{#_ZSH_TURBO_LIST_VALUES}} == 0 )) || exit 3
"#,
            shell_single_quote(&init_path.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "表示行数の計算が直前の一覧に引きずられている: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn init_script_同じ文字列の履歴へ移っても一覧を取り直す() {
        // HIST_IGNORE_DUPS を外した環境などでは、同じ文字列の履歴へ移ると BUFFER は
        // 変わらず HISTNO だけが変わる。その場合も取り直し、入力中の一覧を残さない。
        let tmp = tempfile::tempdir().unwrap();
        let init_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("shell/init.zsh");
        let script = format!(
            r#"source {}
function zle() {{ :; }}
typeset -gi calls=0
function _zsh_turbo_after_modify() {{ (( calls += 1 )); }}
BUFFER='ls books/'
CURSOR=${{#BUFFER}}
HISTNO=14
_zsh_turbo_line_pre_redraw
(( calls == 1 )) || exit 1
_zsh_turbo_line_pre_redraw
(( calls == 1 )) || exit 2
HISTNO=13
_zsh_turbo_line_pre_redraw
(( calls == 2 )) || exit 3
"#,
            shell_single_quote(&init_path.display().to_string())
        );
        let output = std::process::Command::new("zsh")
            .args(["-dfc", &script])
            .env("ZDOTDIR", tmp.path())
            .env("ZSH_COMPDUMP", tmp.path().join("zcompdump"))
            .env("HISTFILE", tmp.path().join("history"))
            .output()
            .expect("zsh を実行できるべき");
        assert!(
            output.status.success(),
            "HISTNO の変化で取り直していない: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn init_script_completion_dirs_は_compinit前に_fpathへ追加する() {
        let script = include_str!("../shell/init.zsh");
        let setup_pos = script
            .find("_zsh_turbo_setup_completion_dirs")
            .expect("補完ディレクトリ追加処理が必要");
        let compinit_pos = script
            .find("autoload -Uz compinit")
            .expect("compinit 初期化が必要");

        assert!(
            setup_pos < compinit_pos,
            "補完関数ディレクトリは compinit より前に fpath へ追加する必要がある"
        );
        assert!(script.contains("typeset -g ZSH_TURBO_COMPLETION_DIRS"));
        assert!(script.contains("${(@s.:.)ZSH_TURBO_COMPLETION_DIRS}"));
        assert!(script.contains("[[ -n \"$completion_dir\" && -d \"$completion_dir\" ]]"));
        assert!(script.contains("(( ${fpath[(Ie)$completion_dir]} == 0 ))"));
        assert!(
            script.contains("() {\n    emulate -L zsh\n    if [[ -f \"$ZSH_COMPDUMP\" ]]"),
            "compinit もユーザのシェルオプションから隔離して実行する必要がある"
        );
    }

    #[test]
    fn init_script_全zsh_turbo関数は_emulate_lで隔離する() {
        // ZLE ウィジェット/フックはユーザのシェルオプション(SH_GLOB/KSH_ARRAYS 等)を
        // 継承するため、各関数は先頭で `emulate -L zsh` を実行してクリーンな zsh
        // オプションへ隔離する必要がある。回帰防止のため全関数を検査する。
        let script = include_str!("../shell/init.zsh");
        let lines: Vec<&str> = script.lines().collect();
        let mut checked = 0;
        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("function _zsh_turbo_") && trimmed.ends_with("() {") {
                let next = lines.get(i + 1).copied().unwrap_or("").trim();
                // 例外: `$?` は emulate 自身が 0 に潰すため、終了ステータスの
                // 捕捉 (glob/配列展開を含まない安全な代入) だけは emulate より
                // 先に置くことを許容する。それ以外の前置処理は不可。
                let ok = next.starts_with("emulate -L zsh")
                    || (next.starts_with("local last_status=$?")
                        && lines
                            .get(i + 2)
                            .copied()
                            .unwrap_or("")
                            .trim()
                            .starts_with("emulate -L zsh"));
                assert!(
                    ok,
                    "{trimmed} の直後は `emulate -L zsh` でオプション隔離する必要がある \
                     (例外は `local last_status=$?` の 1 行のみ)"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "検査対象の _zsh_turbo 関数が見つからない");
    }

    #[test]
    fn cli_はpositionalのハイフン始まり値を受け付ける() {
        // init.zsh は `--` 区切りで positional を渡すが、`--` なしの直接呼び出しでも
        // `-` 始まりの BUFFER で clap が exit 2 にならないこと (回帰防止)
        let cli = Cli::try_parse_from(["zsh-turbo", "highlight", "-la"]).expect("parse 可能なはず");
        match cli.command {
            Commands::Highlight { buffer, .. } => assert_eq!(buffer, "-la"),
            _ => panic!("highlight コマンドになるべき"),
        }
        let cli = Cli::try_parse_from(["zsh-turbo", "suggest", "-x"]).expect("parse 可能なはず");
        match cli.command {
            Commands::Suggest { prefix, .. } => assert_eq!(prefix, "-x"),
            _ => panic!("suggest コマンドになるべき"),
        }
    }

    #[test]
    fn cli_はダブルダッシュ後の登録済みフラグ形式をpositionalとして受け付ける() {
        // BUFFER がちょうど `-h`/`--help` のとき、`--` なしでは help と解釈されて
        // stdout に help が漏れる。init.zsh は `--` を渡すため、この形式が
        // positional として通ることを保証する。
        let cli = Cli::try_parse_from(["zsh-turbo", "suggest", "--strategy", "prefix", "--", "-h"])
            .expect("`--` 後の -h は positional のはず");
        match cli.command {
            Commands::Suggest { prefix, .. } => assert_eq!(prefix, "-h"),
            _ => panic!("suggest コマンドになるべき"),
        }
        let cli = Cli::try_parse_from([
            "zsh-turbo",
            "highlight",
            "--aliases",
            "",
            "--functions",
            "",
            "--",
            "--help",
        ])
        .expect("`--` 後の --help は positional のはず");
        match cli.command {
            Commands::Highlight { buffer, .. } => assert_eq!(buffer, "--help"),
            _ => panic!("highlight コマンドになるべき"),
        }
    }

    #[test]
    fn cli_はハイフン始まりのオプション値を受け付ける() {
        // zsh から渡る $BUFFER 由来のエイリアス/関数名・戦略・履歴パスは `-` 始まりも
        // 有りうる。allow_hyphen_values が無いと clap が exit 2 で弾く（回帰防止）。
        let cli = Cli::try_parse_from([
            "zsh-turbo",
            "highlight",
            "ls",
            "--aliases",
            "-foo",
            "--functions",
            "-bar",
        ])
        .expect("highlight が `-` 始まりの --aliases/--functions を受け付けるべき");
        match cli.command {
            Commands::Highlight {
                aliases, functions, ..
            } => {
                assert_eq!(aliases, "-foo");
                assert_eq!(functions, "-bar");
            }
            _ => panic!("Highlight サブコマンドのはず"),
        }

        let cli = Cli::try_parse_from(["zsh-turbo", "suggest", "gi", "--strategy", "-bad"])
            .expect("suggest が `-` 始まりの --strategy を受け付けるべき");
        match cli.command {
            Commands::Suggest { strategy, .. } => assert_eq!(strategy.as_deref(), Some("-bad")),
            _ => panic!("Suggest サブコマンドのはず"),
        }

        let cli = Cli::try_parse_from(["zsh-turbo", "suggest", "gi", "--history-file", "-x"])
            .expect("suggest が `-` 始まりの --history-file を受け付けるべき");
        match cli.command {
            Commands::Suggest { history_file, .. } => {
                assert_eq!(history_file.as_deref(), Some("-x"))
            }
            _ => panic!("Suggest サブコマンドのはず"),
        }

        // メニューで選んでいた項目の BUFFER も `-` 始まりが有りうる
        let cli = Cli::try_parse_from([
            "zsh-turbo",
            "suggest",
            "--ui-list",
            "--menu",
            "--selected",
            "-x build",
            "--",
            "-x",
        ])
        .expect("suggest が `-` 始まりの --selected を受け付けるべき");
        match cli.command {
            Commands::Suggest { menu, selected, .. } => {
                assert!(menu);
                assert_eq!(selected.as_deref(), Some("-x build"));
            }
            _ => panic!("Suggest サブコマンドのはず"),
        }

        let cli = Cli::try_parse_from(["zsh-turbo", "complete", "gi", "--history-file", "-x"])
            .expect("complete が `-` 始まりの --history-file を受け付けるべき");
        match cli.command {
            Commands::Complete { history_file, .. } => {
                assert_eq!(history_file.as_deref(), Some("-x"))
            }
            _ => panic!("Complete サブコマンドのはず"),
        }
    }
}
