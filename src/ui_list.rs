//! ZLE の候補一覧 (`suggest --ui-list`) の応答を組み立てる。
//!
//! 1 行目 `v1<TAB>種類<TAB>件数<TAB>complete|partial<TAB>入力中の表示行数<TAB>見出し`、
//! 続いて `ghost<TAB>BUFFER`、`select<TAB>N`、`item<TAB>表示名<TAB>BUFFER` を並べ、最後に `end` を置く。
//! `select` は ↓ でメニューを開いたときに選ぶ項目の番号 (送った item の 1 始まりの順番)。
//! zsh 側は表示名を並べ、選ばれた候補の BUFFER へ置き換えるだけにする。
//! 行の種類は後から足せるよう、zsh 側は知らない種類の行を無視する。

use crate::config::UiLanguage;
use crate::{completion, path_candidates, project_tasks, suggest, tui};
use std::fmt::Write as _;
use std::path::Path;
use unicode_width::UnicodeWidthChar;

/// パス候補の取得上限。入力中は `max_suggestions` 行まで表示し、残りは ↓ のメニューで選ぶ。
const PATH_LIST_LIMIT: usize = 256;
/// zsh の整数変数で扱える範囲に表示行数を収める
const MAX_PREVIEW_ROWS: usize = 1000;
/// これより狭い端末では表示名を切り詰めない
const MIN_FIT_COLUMNS: usize = 8;

pub struct Request<'a> {
    pub buffer: &'a str,
    pub history_file: Option<&'a str>,
    pub strategy: &'a suggest::Strategy,
    /// `suggest.max_suggestions`。入力中の表示行数の上限、0 で一覧なし
    pub max: usize,
    /// 端末の桁数 (0 は切り詰めない)
    pub columns: usize,
    /// 直前に実行された ZLE widget 名
    pub last_widget: &'a str,
    pub language: UiLanguage,
    /// ディレクトリごとのタスクの利用記録 (使わないなら None)
    pub task_usage: Option<&'a Path>,
}

pub fn response(request: &Request) -> String {
    let mut out = String::new();
    let rows = request.max.min(MAX_PREVIEW_ROWS);
    let tasks: Vec<_> = if request.buffer.is_empty() || request.max == 0 {
        Vec::new()
    } else {
        project_tasks::labeled_candidates(request.buffer, project_tasks::LIST_LIMIT)
            .into_iter()
            .filter(|(name, command)| sendable(name) && sendable(command))
            .collect()
    };
    if !tasks.is_empty() {
        push_header(
            &mut out,
            "tasks",
            tasks.len(),
            true,
            rows,
            tui::task_list_title(request.language),
        );
        // よく使うタスクを ↓ の初期選択と ghost にそろえ、Tab と ↓→Enter の結果を一致させる
        let preferred = suggest::preferred_task(
            request.buffer,
            &tasks,
            request.history_file,
            request.task_usage,
        );
        push_line(&mut out, "ghost", None, &tasks[preferred.unwrap_or(0)].1);
        let items: Vec<_> = tasks
            .into_iter()
            .map(|(name, command)| (fit_width(&name, request.columns), command))
            .collect();
        push_items(&mut out, &items, preferred);
        out.push_str("end\n");
        return out;
    }

    let history =
        suggest::get_history_suggestion(request.buffer, request.history_file, request.strategy);
    let listing_allowed = request.max > 0 && !is_history_motion(request.last_widget);

    // `uv` や `npm i` のように `run` 等を要する CLI の名前・2 語目を入力中なら、そのサブコマンド
    if listing_allowed
        && let Some((command, head, word)) = project_tasks::subcommand_query(request.buffer)
        && let Some(help) = completion::top_level(command)
    {
        let items = sendable_items(
            command_items(&help, &head, word)
                .into_iter()
                .map(|(label, value)| (fit_width(&label, request.columns), value)),
        );
        if !items.is_empty() {
            let title = if word.starts_with('-') {
                tui::option_list_title(request.language)
            } else {
                tui::command_list_title(request.language)
            };
            push_header(&mut out, "commands", items.len(), true, rows, title);
            push_ghost_and_items(&mut out, history.as_deref(), request.buffer, &items);
            out.push_str("end\n");
            return out;
        }
    }

    let listing = listing_allowed
        .then(|| path_candidates::list(request.buffer, PATH_LIST_LIMIT))
        .flatten()
        .map(|listing| {
            let items = sendable_items(listing.candidates.into_iter().map(|candidate| {
                (
                    fit_width(&candidate.label, request.columns),
                    candidate.buffer,
                )
            }));
            (items, listing.total, listing.complete)
        })
        .filter(|(items, ..)| !items.is_empty());
    match listing {
        Some((items, total, complete)) => {
            push_header(
                &mut out,
                "files",
                total,
                complete,
                rows,
                tui::file_list_title(request.language),
            );
            push_ghost_and_items(&mut out, history.as_deref(), request.buffer, &items);
        }
        None => {
            push_header(&mut out, "none", 0, true, rows, "");
            if let Some(history) = &history {
                push_line(&mut out, "ghost", None, history);
            }
        }
    }
    out.push_str("end\n");
    out
}

fn push_header(
    out: &mut String,
    kind: &str,
    total: usize,
    complete: bool,
    rows: usize,
    title: &str,
) {
    let state = if complete { "complete" } else { "partial" };
    let _ = writeln!(out, "v1\t{kind}\t{total}\t{state}\t{rows}\t{title}");
}

/// TAB・改行は行プロトコルを壊すため、制御文字を含む値と空の値は送らない。
fn sendable(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

/// 送れる (表示名, BUFFER) だけを残す。`select` の番号は送った項目で数えるため、先にふるう
fn sendable_items(items: impl Iterator<Item = (String, String)>) -> Vec<(String, String)> {
    items
        .filter(|(label, value)| sendable(label) && sendable(value))
        .collect()
}

fn push_line(out: &mut String, tag: &str, label: Option<&str>, value: &str) {
    if !sendable(value) || label.is_some_and(|label| !sendable(label)) {
        return;
    }
    out.push_str(tag);
    if let Some(label) = label {
        out.push('\t');
        out.push_str(label);
    }
    out.push('\t');
    out.push_str(value);
    out.push('\n');
}

/// `select` と `item` の行を出す。`selected` は `items` の添字
fn push_items(out: &mut String, items: &[(String, String)], selected: Option<usize>) {
    if let Some(index) = selected {
        let _ = writeln!(out, "select\t{}", index + 1);
    }
    for (label, value) in items {
        push_line(out, "item", Some(label), value);
    }
}

/// 履歴の続き (なければ先頭の項目) を ghost にし、ghost が通る項目を ↓ の初期選択にする
fn push_ghost_and_items<'a>(
    out: &mut String,
    history: Option<&'a str>,
    buffer: &str,
    items: &'a [(String, String)],
) {
    let values = items.iter().map(|(_, value)| value.as_str());
    let ghost = pick_ghost(history, buffer, values);
    if let Some(ghost) = ghost {
        push_line(out, "ghost", None, ghost);
    }
    let selected = ghost.and_then(|ghost| {
        items
            .iter()
            .position(|(_, value)| passes_through(ghost, value))
    });
    push_items(out, items, selected);
}

/// ghost が一覧の項目を経由するか。項目の BUFFER で始まり、その直後で語か階層が切れること
/// (`make install` は `make installer` を経由しない。`ls dir/` は `ls dir/a` を経由する)
fn passes_through(ghost: &str, value: &str) -> bool {
    let stem = value.trim_end_matches([' ', '\t']);
    ghost
        .strip_prefix(stem)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']) || stem.ends_with('/'))
}

/// 履歴の続きを優先し、なければ BUFFER を延長する先頭の候補を薄く出す。
fn pick_ghost<'a>(
    history: Option<&'a str>,
    buffer: &str,
    mut values: impl Iterator<Item = &'a str>,
) -> Option<&'a str> {
    let extends = |value: &str| value.len() > buffer.len() && value.starts_with(buffer);
    history
        .filter(|value| extends(value))
        .or_else(|| values.find(|value| extends(value)))
        .or(history)
}

/// 入力中の語に前方一致するサブコマンド (`-` 始まりならオプション) を、
/// (説明付きの表示名, 採用後の BUFFER) の組で名前順に返す。
fn command_items(help: &completion::TopLevel, head: &str, word: &str) -> Vec<(String, String)> {
    let entries = if word.starts_with('-') {
        &help.options
    } else {
        &help.commands
    };
    let mut matched: Vec<_> = entries
        .iter()
        .filter(|(name, _)| name.starts_with(word) && name != word)
        .collect();
    matched.sort_by(|a, b| a.0.cmp(&b.0));
    matched.dedup_by(|a, b| a.0 == b.0);
    matched.truncate(PATH_LIST_LIMIT);
    // 名前の列をそろえ、説明を続ける (長すぎる名前で説明が押し出されないよう上限を設ける)
    let width = matched
        .iter()
        .map(|(name, _)| name.chars().count())
        .max()
        .unwrap_or(0)
        .min(24);
    matched
        .into_iter()
        .map(|(name, description)| {
            let description = description.split_whitespace().collect::<Vec<_>>().join(" ");
            let label = if description.is_empty() {
                name.clone()
            } else {
                format!("{name:<width$}  {description}")
            };
            (label, format!("{head}{name} "))
        })
        .collect()
}

/// 直前の widget が履歴の移動なら真。呼び出した行でファイル一覧を出すと、
/// 次の ↓ が一覧に取られて履歴を戻れなくなる。
/// zsh 側は ↓ が履歴検索へ回したとき、実際に動いた widget 名を渡す。
fn is_history_motion(widget: &str) -> bool {
    widget.contains("history")
        || widget.contains("beginning-search")
        || widget.starts_with("up-line-or-")
        || widget.starts_with("down-line-or-")
}

/// 行頭の印 2 セルと右端の 1 セルを除いた幅に収まるよう、末尾を … で切り詰める。
/// 折り返すと一覧の行数計算が崩れ、画面外へはみ出す。
fn fit_width(text: &str, columns: usize) -> String {
    if columns < MIN_FIT_COLUMNS {
        return text.to_owned();
    }
    let max = columns - 3;
    let width = |c: char| c.width().unwrap_or(0);
    if text.chars().map(width).sum::<usize>() <= max {
        return text.to_owned();
    }
    // … を曖昧幅 (2 セル) で描く端末もあるため 2 セル残す
    let budget = max - 2;
    let mut used = 0;
    let mut fitted: String = text
        .chars()
        .take_while(|&c| {
            used += width(c);
            used <= budget
        })
        .collect();
    fitted.push('…');
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request<'a>(buffer: &'a str, history_file: &'a str) -> Request<'a> {
        Request {
            buffer,
            history_file: Some(history_file),
            strategy: &suggest::Strategy::Prefix,
            max: 10,
            columns: 0,
            last_widget: "self-insert",
            language: UiLanguage::En,
            task_usage: None,
        }
    }

    #[test]
    fn 履歴だけの応答は見出しとghostと終端を返す() {
        let dir = tempfile::tempdir().unwrap();
        let history = dir.path().join("history");
        std::fs::write(&history, "echo zsh-turbo-ui-list-test\n").unwrap();
        let history = history.to_str().unwrap();
        assert_eq!(
            response(&request("echo zsh-turbo-ui", history)),
            "v1\tnone\t0\tcomplete\t10\t\nghost\techo zsh-turbo-ui-list-test\nend\n"
        );
        assert_eq!(
            response(&request("echo no-match-xyz", history)),
            "v1\tnone\t0\tcomplete\t10\t\nend\n"
        );
    }

    #[test]
    fn ファイル一覧は表示名と採用後のbufferを並べ履歴のghostを優先する() {
        let dir = tempfile::tempdir().unwrap();
        let books = dir.path().join("books");
        std::fs::create_dir(&books).unwrap();
        std::fs::create_dir(books.join("sub")).unwrap();
        std::fs::write(books.join("alpha.txt"), "").unwrap();
        let history = dir.path().join("history");
        let base = format!("ls {}/", books.display());
        std::fs::write(&history, format!("{base}alpha.txt | wc\n")).unwrap();
        let history = history.to_str().unwrap();

        // ghost が経由する alpha.txt (2 番目) を ↓ の初期選択にする
        let out = response(&request(&base, history));
        assert_eq!(
            out,
            format!(
                "v1\tfiles\t2\tcomplete\t10\tFiles\nghost\t{base}alpha.txt | wc\nselect\t2\nitem\tsub/\t{base}sub/\nitem\talpha.txt\t{base}alpha.txt \nend\n"
            )
        );

        // 履歴に続きが無ければ先頭のパス候補を ghost にする
        let empty = dir.path().join("empty-history");
        std::fs::write(&empty, "").unwrap();
        let out = response(&request(&base, empty.to_str().unwrap()));
        assert!(
            out.contains(&format!("\nghost\t{base}sub/\nselect\t1\n")),
            "{out}"
        );
    }

    #[test]
    fn ghostが項目の語や階層の区切りまで一致するときだけ経由とみなす() {
        assert!(passes_through("make install", "make install"));
        assert!(passes_through("make install PREFIX=/x", "make install"));
        assert!(!passes_through("make installer", "make install"));
        assert!(passes_through("uv run pytest -x", "uv run "));
        assert!(passes_through("uv run", "uv run "));
        assert!(!passes_through("uv runner", "uv run "));
        assert!(passes_through("ls dir/sub/a.txt", "ls dir/sub/"));
        assert!(passes_through("ls dir/a.txt | wc", "ls dir/a.txt "));
        assert!(!passes_through("ls dir/a.txtx", "ls dir/a.txt "));
        assert!(!passes_through("ls other", "ls dir/"));
    }

    #[test]
    fn 送れない項目を除いてからselectの番号を数える() {
        let items = sendable_items(
            [
                ("a".to_owned(), "cmd a ".to_owned()),
                ("b\tx".to_owned(), "cmd b ".to_owned()),
                ("c".to_owned(), "cmd c ".to_owned()),
            ]
            .into_iter(),
        );
        let mut out = String::new();
        push_ghost_and_items(&mut out, Some("cmd c --flag"), "cmd ", &items);
        assert_eq!(
            out,
            "ghost\tcmd c --flag\nselect\t2\nitem\ta\tcmd a \nitem\tc\tcmd c \n"
        );
        // どの項目も経由しない ghost では select を出さない
        let mut out = String::new();
        push_ghost_and_items(&mut out, Some("cmdx"), "cmd", &items);
        assert_eq!(out, "ghost\tcmdx\nitem\ta\tcmd a \nitem\tc\tcmd c \n");
    }

    #[test]
    fn 履歴移動の直後と候補数0ではファイル一覧を返さない() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("alpha.txt"), "").unwrap();
        let history = dir.path().join("history");
        std::fs::write(&history, "").unwrap();
        let history = history.to_str().unwrap();
        let base = format!("cat {}/", dir.path().display());

        let mut after_history = request(&base, history);
        after_history.last_widget = "up-line-or-beginning-search";
        assert!(response(&after_history).starts_with("v1\tnone\t"));

        let mut disabled = request(&base, history);
        disabled.max = 0;
        assert!(response(&disabled).starts_with("v1\tnone\t0\tcomplete\t0\t"));

        // 一覧対象のディレクトリには alpha.txt と履歴ファイルの 2 件がある
        assert!(response(&request(&base, history)).starts_with("v1\tfiles\t2\t"));
    }

    #[test]
    fn 履歴移動のwidgetを判定する() {
        for widget in [
            "up-line-or-beginning-search",
            "down-line-or-beginning-search",
            "up-line-or-history",
            "down-line-or-search",
            "up-history",
            "history-substring-search-up",
            "history-beginning-search-backward",
            "vi-fetch-history",
            "_zsh_turbo_history_menu",
        ] {
            assert!(is_history_motion(widget), "{widget}");
        }
        for widget in [
            "",
            "self-insert",
            "backward-delete-char",
            "_zsh_turbo_accept_tab",
            "insert-last-word",
            "yank",
        ] {
            assert!(!is_history_motion(widget), "{widget}");
        }
    }

    #[test]
    fn サブコマンドとオプションを前方一致で名前順に並べ説明を添える() {
        let help = completion::TopLevel {
            commands: vec![
                ("sync".into(), "Update the\tenvironment".into()),
                ("run".into(), "Run a command".into()),
                ("self".into(), String::new()),
            ],
            options: vec![
                ("-q".into(), "Quiet".into()),
                ("--quiet".into(), "Quiet".into()),
            ],
        };
        let pair = |label: &str, value: &str| (label.to_owned(), value.to_owned());
        assert_eq!(
            command_items(&help, "uv ", ""),
            [
                pair("run   Run a command", "uv run "),
                pair("self", "uv self "),
                pair("sync  Update the environment", "uv sync "),
            ]
        );
        assert_eq!(
            command_items(&help, "uv ", "s"),
            [
                pair("self", "uv self "),
                pair("sync  Update the environment", "uv sync ")
            ]
        );
        assert!(command_items(&help, "uv ", "sync").is_empty());
        assert_eq!(
            command_items(&help, "uv ", "--"),
            [pair("--quiet  Quiet", "uv --quiet ")]
        );
    }

    #[test]
    fn 表示名を端末幅に収まるよう切り詰める() {
        assert_eq!(fit_width("short.txt", 80), "short.txt");
        assert_eq!(fit_width("abcdefghij", 0), "abcdefghij");
        assert_eq!(fit_width("abcdefghij", 7), "abcdefghij");
        // 12 桁: 印 2 + 右端 1 を除く 9 セルへ、… の 2 セルを残して 7 セル
        assert_eq!(fit_width("abcdefghijklmn", 12), "abcdefg…");
        // 全角は 2 セルで数える
        assert_eq!(fit_width("日本語のファイル名.txt", 12), "日本語…");
        // 結合文字は幅 0
        assert_eq!(fit_width("e\u{301}", 12), "e\u{301}");
    }

    #[test]
    fn 制御文字を含む値は行に出さない() {
        let mut out = String::new();
        push_line(&mut out, "item", Some("a\tb"), "ls a");
        push_line(&mut out, "item", Some("a"), "ls\na");
        push_line(&mut out, "ghost", None, "");
        assert!(out.is_empty());
        push_line(&mut out, "item", Some("a b"), "ls a\\ b");
        assert_eq!(out, "item\ta b\tls a\\ b\n");
    }
}
