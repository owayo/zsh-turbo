use super::*;
use crate::completion::{self, Registration, Source};
use ratatui::{layout::Rect, widgets::Wrap};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Field {
    Command,
    Input,
    Watch,
}

impl Field {
    pub(super) fn label(self, lang: Lang) -> &'static str {
        match self {
            Self::Command => lang.text("CLI command name", "CLI コマンド名"),
            Self::Input => lang.text(
                "Generator argv (JSON array) / completion file",
                "生成コマンド（JSON 配列）/ 補完ファイル",
            ),
            Self::Watch => lang.text(
                "Extra files to watch (JSON array)",
                "追加で監視するファイル（JSON 配列）",
            ),
        }
    }
    pub(super) fn value(self, entry: &Registration) -> String {
        match self {
            Self::Command => entry.command.clone(),
            Self::Input if entry.source == Source::File => entry.file.clone(),
            Self::Input => serde_json::to_string(&entry.generator).unwrap_or_default(),
            Self::Watch => serde_json::to_string(&entry.watch_files).unwrap_or_default(),
        }
    }
    pub(super) fn set(self, entry: &mut Registration, value: &str) {
        match self {
            Self::Command => entry.command = value.into(),
            Self::Input if entry.source == Source::File => entry.file = value.into(),
            Self::Input => entry.generator = serde_json::from_str(value).unwrap_or_default(),
            Self::Watch => entry.watch_files = serde_json::from_str(value).unwrap_or_default(),
        }
    }
}

pub(super) fn validate(
    app: &App,
    index: usize,
    field: Field,
    value: &str,
) -> Result<(), &'static str> {
    match field {
        Field::Command
            if !completion::valid_command(value)
                || app
                    .config
                    .completions
                    .iter()
                    .enumerate()
                    .any(|(i, e)| i != index && e.command == value) =>
        {
            Err(app.lang.text(
                "Enter a unique CLI name (letters, digits, -, _, +, .).",
                "重複しない CLI 名を指定してください（英数字・-・_・+・.）。",
            ))
        }
        Field::Watch | Field::Input
            if field == Field::Watch || app.config.completions[index].source != Source::File =>
        {
            let args = serde_json::from_str::<Vec<String>>(value).map_err(|_| {
                app.lang.text(
                    "Use a JSON string array, e.g. [\"tool\",\"completion\",\"zsh\"].",
                    "文字列の JSON 配列を入力してください。例: [\"tool\",\"completion\",\"zsh\"]",
                )
            })?;
            if field == Field::Input && args.first().is_none_or(|s| s.is_empty()) {
                return Err(app.lang.text(
                    "Specify an executable first.",
                    "先頭に実行ファイルを指定してください。",
                ));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn add(app: &mut App) {
    app.completion_draft_prior_dirty = app.dirty;
    let command = (1..)
        .map(|n| format!("cli{n}"))
        .find(|s| !app.config.completions.iter().any(|e| e.command == *s))
        .unwrap();
    app.config.completions.push(Registration {
        command,
        ..Default::default()
    });
    app.completion_index = app.config.completions.len() - 1;
    app.completion_detail = true;
    app.completion_draft = true;
    app.completion_focus = 1;
    app.dirty = true;
    app.start_edit(EditTarget::Completion(app.completion_index, Field::Command));
}

pub(super) fn remove(app: &mut App) {
    if app.config.completions.is_empty() {
        return;
    }
    app.config.completions.remove(app.completion_index);
    app.completion_index = app
        .completion_index
        .min(app.config.completions.len().saturating_sub(1));
    app.completion_focus = 0;
    app.completion_detail = false;
    app.dirty = true;
}

pub(super) fn select(app: &mut App, forward: bool) {
    if !app.completion_detail {
        return;
    }
    let len = app.config.completions.len();
    if len == 0 {
        return;
    }
    if app.completion_focus == 0 {
        app.completion_index = if forward {
            (app.completion_index + 1) % len
        } else {
            (app.completion_index + len - 1) % len
        };
    } else if matches!(app.completion_focus, 2 | 3) {
        activate(app);
    }
}

pub(super) fn activate(app: &mut App) {
    if !app.completion_detail {
        if !app.config.completions.is_empty() {
            app.completion_detail = true;
            app.completion_focus = 1;
        }
        return;
    }
    if app.completion_focus == 0 {
        app.completion_detail = false;
        return;
    }
    if matches!(app.completion_focus, 6 | 7) {
        refresh(app, app.completion_focus == 7);
        return;
    }
    let Some(entry) = app.config.completions.get_mut(app.completion_index) else {
        add(app);
        return;
    };
    let field = match app.completion_focus {
        0 => {
            select(app, true);
            return;
        }
        1 => Field::Command,
        2 => {
            entry.enabled = !entry.enabled;
            app.dirty = true;
            return;
        }
        3 => {
            entry.source = match entry.source {
                Source::Help => Source::Generator,
                Source::Generator => Source::File,
                Source::File => Source::Help,
            };
            app.dirty = true;
            return;
        }
        4 if entry.source != Source::Help => Field::Input,
        5 => Field::Watch,
        _ => return,
    };
    app.start_edit(EditTarget::Completion(app.completion_index, field));
}

pub(super) fn refresh(app: &mut App, force: bool) {
    if app.completion_job.is_some() {
        app.status_msg = Some(
            app.lang
                .text("A refresh is already running.", "再生成を実行中です。")
                .into(),
        );
        return;
    }
    if app.dirty {
        app.status_msg = Some(
            app.lang
                .text(
                    "Save settings first (s).",
                    "先に s で設定を保存してください。",
                )
                .into(),
        );
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut command = std::process::Command::new(exe);
        command.arg("completion-refresh");
        if force {
            command.arg("--force");
        }
        match command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(child) => {
                let (sender, receiver) = std::sync::mpsc::channel();
                app.completion_job = Some(receiver);
                std::thread::spawn(move || {
                    let result = match child.wait_with_output() {
                        Ok(output) => String::from_utf8_lossy(&output.stdout)
                            .trim()
                            .replace('\n', " / "),
                        Err(error) => error.to_string(),
                    };
                    let _ = sender.send(result);
                });
                app.status_msg = Some(
                    app.lang
                        .text(
                            "Refreshing saved registrations in the background…",
                            "保存済みの登録をバックグラウンドで再生成しています…",
                        )
                        .into(),
                );
            }
            Err(error) => app.status_msg = Some(error.to_string()),
        }
    }
}

pub(super) fn poll_refresh(app: &mut App) {
    let Some(receiver) = &app.completion_job else {
        return;
    };
    match receiver.try_recv() {
        Ok(result) => {
            app.status_msg = Some(format!("{}: {result}", app.lang.text("Result", "実行結果")));
            app.completion_job = None;
        }
        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
            app.completion_job = None;
        }
        Err(std::sync::mpsc::TryRecvError::Empty) => {}
    }
}

pub(super) fn render(frame: &mut Frame, app: &App, area: Rect) {
    if !app.completion_detail {
        render_list(frame, app, area);
        return;
    }
    let parts = Layout::vertical([Constraint::Min(0), Constraint::Length(4)]).split(area);
    if let Some(entry) = app.config.completions.get(app.completion_index) {
        let source = match entry.source {
            Source::Help => app.lang.text("Help (automatic)", "ヘルプ（自動生成）"),
            Source::Generator => app.lang.text("Native generator", "生成コマンド"),
            Source::File => app.lang.text("Completion file", "補完ファイル"),
        };
        let state = completion::status(entry);
        let state = match state.as_str() {
            "ready" => app.lang.text("Ready", "利用可能"),
            "pending" => app.lang.text("Pending", "未生成"),
            _ => &state,
        };
        let selection = format!(
            "{} / {}  {} — {}",
            app.completion_index + 1,
            app.config.completions.len(),
            entry.command,
            state
        );
        let input = if entry.source == Source::Help {
            "--help".into()
        } else {
            Field::Input.value(entry)
        };
        let rows = [
            (
                app.lang
                    .text("Back to registered CLIs", "登録済み CLI 一覧に戻る"),
                selection,
            ),
            (Field::Command.label(app.lang), entry.command.clone()),
            (
                app.lang.text("Enabled", "有効"),
                bool_str(entry.enabled, app.lang).into(),
            ),
            (app.lang.text("Source", "補完元"), source.into()),
            (Field::Input.label(app.lang), input),
            (Field::Watch.label(app.lang), Field::Watch.value(entry)),
            (
                app.lang
                    .text("Rebuild changed caches (r)", "変更を確認して再生成（r）"),
                app.lang
                    .text("Enter: all saved CLIs", "Enter: 保存済みの全 CLI")
                    .into(),
            ),
            (
                app.lang.text(
                    "Force rebuild caches (Shift+R)",
                    "キャッシュを強制再生成（Shift+R）",
                ),
                app.lang
                    .text("Enter: all saved CLIs", "Enter: 保存済みの全 CLI")
                    .into(),
            ),
        ];
        let items = rows
            .iter()
            .enumerate()
            .map(|(i, (name, value))| field_item(name, value, app.completion_focus == i, false))
            .collect();
        render_fields(
            frame,
            items,
            app.lang.text(" CLI completions ", " CLI 補完 "),
            app.completion_focus,
            parts[0],
        );
    } else {
        frame.render_widget(
            Paragraph::new(app.lang.text(
                "Press n to register a CLI.",
                "n で補完対象の CLI を登録します。",
            )),
            parts[0],
        );
    }
    let help = if app.completion_job.is_some() {
        app.lang.text(
            "Refreshing… You can continue editing or close this screen.",
            "再生成中… 編集を続けたり、画面を閉じたりできます。",
        )
    } else {
        app.lang.text(
        "n: Add  Delete: Remove  r: Refresh  Shift+R: Force  s: Save\nBinary changes → background rebuild (30s checks at prompts).\nRestart the shell after changing registrations. Failed updates keep the last cache.",
        "n: 追加  Delete: 削除  r: 再生成  Shift+R: 強制再生成  s: 保存\nバイナリ変更で自動再生成（プロンプト表示時、30秒間隔で確認）。\n登録変更後はシェルを再起動。更新失敗時は前の補完を維持します。"
    )
    };
    frame.render_widget(Paragraph::new(help).wrap(Wrap { trim: false }), parts[1]);
}

fn render_list(frame: &mut Frame, app: &App, area: Rect) {
    let title = app.lang.text(" Registered CLIs ", " 登録済み CLI ");
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if app.config.completions.is_empty() {
        frame.render_widget(
            Paragraph::new(app.lang.text(
                "No CLI registered. Press n to add one.",
                "登録された CLI はありません。n で追加します。",
            )),
            inner,
        );
        return;
    }
    let visible = usize::from(inner.height).max(1);
    let start = app
        .completion_index
        .saturating_add(1)
        .saturating_sub(visible);
    let items: Vec<ListItem> = app
        .config
        .completions
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, entry)| {
            let status = completion::status(entry);
            let status = match status.as_str() {
                "ready" => app.lang.text("Ready", "利用可能"),
                "pending" => app.lang.text("Pending", "未生成"),
                _ => &status,
            };
            let source = match entry.source {
                Source::Help => app.lang.text("help", "ヘルプ"),
                Source::Generator => app.lang.text("generator", "生成コマンド"),
                Source::File => app.lang.text("file", "ファイル"),
            };
            let marker = if i == app.completion_index {
                "▶ "
            } else {
                "  "
            };
            let enabled = if entry.enabled { "●" } else { "○" };
            let line = format!(
                "{marker}{enabled}  {}  ({source})  [{status}]",
                entry.command
            );
            let style = if i == app.completion_index {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(line).style(style)
        })
        .collect();
    frame.render_widget(List::new(items), inner);
}
