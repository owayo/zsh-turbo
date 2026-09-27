use clap::{CommandFactory, Parser, Subcommand, ValueHint};

mod completion;
mod config;
mod doctor;
mod font_install;
mod highlight;
mod icons;
mod prompt;
mod style;
mod suggest;
mod tui;

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
        /// 検索戦略（prefix, substring, fuzzy）
        #[arg(long, allow_hyphen_values = true)]
        strategy: Option<String>,
    },
    /// 履歴ベースの補完候補を一覧表示する
    Complete {
        /// 現在の入力プレフィックス（zsh から渡る `$BUFFER` は `-` 始まりも許容する）
        #[arg(allow_hyphen_values = true)]
        prefix: String,
        /// zsh 履歴ファイルのパス
        #[arg(long, allow_hyphen_values = true, value_hint = ValueHint::FilePath)]
        history_file: Option<String>,
        /// 検索戦略（prefix, substring, fuzzy）
        #[arg(long, allow_hyphen_values = true)]
        strategy: Option<String>,
        /// 返す補完候補の最大数
        #[arg(long)]
        max: Option<usize>,
    },
    /// 対話型設定ウィザードを起動する
    Configure,
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
            strategy,
        } => {
            let strategy = strategy.unwrap_or_else(|| config::load_config().suggest.strategy);
            let strat = suggest::Strategy::from_str(&strategy);
            if let Some(suggestion) =
                suggest::get_suggestion(&prefix, history_file.as_deref(), &strat)
            {
                print!("{suggestion}");
            }
        }
        Commands::Complete {
            prefix,
            history_file,
            strategy,
            max,
        } => {
            let max = max.unwrap_or_else(|| config::load_config().suggest.max_suggestions);
            let completions = if let Some(strategy) = strategy {
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
                    "Next: set your terminal font to 'MesloLGS NF', then restart the terminal."
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
    ] {
        script.push_str(&format!(
            "if [[ -z ${{{name}+x}} ]]; then\n  typeset -g {name}={}\nfi\n",
            shell_single_quote(value)
        ));
    }
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
            script.contains(
                "function _zsh_turbo_clear_suggestion() {\n    emulate -L zsh\n    POSTDISPLAY=\"\"\n    _ZSH_TURBO_SUGGESTION=\"\"\n    _zsh_turbo_autosuggest_display\n}"
            ),
            "サジェスト消去時は POSTDISPLAY だけでなく region_highlight も更新する必要がある"
        );
        assert!(script.contains("_zsh_turbo_clear_suggestion\n    BUFFER=\"${BUFFER}${chunk}\""));
    }

    #[test]
    fn init_script_は行末以外でサジェストを表示も受理もしない() {
        let script = include_str!("../shell/init.zsh");
        assert!(
            script.contains("if (( CURSOR == ${#BUFFER} )) && \\")
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
for expected in 'ls -l /path' 'ls -l /path/to' 'ls -l /path/to/hoge' 'ls -l /path/to/hoge/fuga'; do
    POSTDISPLAY="${{full#"$BUFFER"}}"
    region_highlight=("${{#BUFFER}} 100 fg=8")
    _zsh_turbo_accept_right
    [[ "$BUFFER" == "$expected" && $CURSOR == ${{#BUFFER}} && ${{#region_highlight}} == 0 ]] || {{ print -ru2 -- "expected=$expected actual=$BUFFER cursor=$CURSOR highlights=${{(j:,:)region_highlight}}"; exit 1; }}
done
BUFFER='ls -l'
CURSOR=${{#BUFFER}}
POSTDISPLAY=' /path/to/hoge/fuga'
_zsh_turbo_accept_tab
[[ "$BUFFER" == "$full" ]] || exit 2
POSTDISPLAY=''
_zsh_turbo_accept_tab
[[ "$ZLE_CALLED" == expand-or-complete ]] || exit 3
BUFFER='ls -l'
CURSOR=${{#BUFFER}}
POSTDISPLAY=' /path/to'
ZSH_TURBO_KEY_TAB=default
ZLE_CALLED=''
_zsh_turbo_accept_tab
[[ "$BUFFER" == 'ls -l' && "$ZLE_CALLED" == expand-or-complete ]] || exit 4
ZSH_TURBO_KEY_RIGHT=full
_zsh_turbo_accept_right
[[ "$BUFFER" == 'ls -l /path/to' ]] || exit 5
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
