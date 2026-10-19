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
            if (field == Field::Watch
                && args.iter().any(|path| !completion::valid_source_path(path)))
                || (field == Field::Input && !completion::valid_generator(&args[0]))
            {
                return Err(app.lang.text(
                    "Use absolute paths or ~/; a generator can also use a PATH command.",
                    "絶対パスか ~/ を使ってください。生成コマンド名は PATH からも検索できます。",
                ));
            }
            Ok(())
        }
        Field::Input if !completion::valid_source_path(value) => Err(app.lang.text(
            "Use an absolute completion file path or ~/.",
            "補完ファイルは絶対パスか ~/ で指定してください。",
        )),
        _ => Ok(()),
    }
}

/// 詳細画面の行。表示順は `ALL` の並びで決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DetailItem {
    /// 一覧へ戻る（←/→ で登録を切り替える）
    Back,
    Command,
    Enabled,
    Source,
    Input,
    Watch,
    Refresh,
    ForceRefresh,
}

impl DetailItem {
    pub(super) const ALL: [Self; 8] = [
        Self::Back,
        Self::Command,
        Self::Enabled,
        Self::Source,
        Self::Input,
        Self::Watch,
        Self::Refresh,
        Self::ForceRefresh,
    ];

    fn handle(self, app: &mut App, action: ItemAction) {
        let index = app.completion_index;
        match self {
            Self::Back => match action {
                ItemAction::Adjust(direction) => select_registration(app, direction),
                ItemAction::Activate => app.completion_detail = false,
            },
            Self::Command => {
                app.edit_on_activate(action, EditTarget::Completion(index, Field::Command))
            }
            // 有効と補完元は左右でも決定と同じく切り替え、補完元は向きに関わらず次へ進める
            Self::Enabled => {
                if let Some(entry) = app.config.completions.get_mut(index) {
                    entry.enabled = !entry.enabled;
                    app.dirty = true;
                }
            }
            Self::Source => {
                if let Some(entry) = app.config.completions.get_mut(index) {
                    entry.source = next_source(&entry.source);
                    app.dirty = true;
                }
            }
            // 入力欄は生成コマンドか補完ファイルを使うときだけ編集する
            Self::Input => {
                if app
                    .config
                    .completions
                    .get(index)
                    .is_some_and(|entry| matches!(entry.source, Source::Generator | Source::File))
                {
                    app.edit_on_activate(action, EditTarget::Completion(index, Field::Input));
                }
            }
            Self::Watch => {
                app.edit_on_activate(action, EditTarget::Completion(index, Field::Watch))
            }
            Self::Refresh => match action {
                ItemAction::Activate => refresh(app, false),
                ItemAction::Adjust(_) => {}
            },
            Self::ForceRefresh => match action {
                ItemAction::Activate => refresh(app, true),
                ItemAction::Adjust(_) => {}
            },
        }
    }

    fn row(self, app: &App, entry: &Registration, focused: bool) -> ListItem<'static> {
        let lang = app.lang;
        let (label, value) = match self {
            Self::Back => (
                lang.text("Back to registered CLIs", "登録済み CLI 一覧に戻る"),
                format!(
                    "{} / {}  {} — {}",
                    app.completion_index + 1,
                    app.config.completions.len(),
                    entry.command,
                    status_label(entry, lang)
                ),
            ),
            Self::Command => (Field::Command.label(lang), entry.command.clone()),
            Self::Enabled => (
                lang.text("Enabled", "有効"),
                bool_str(entry.enabled, lang).into(),
            ),
            Self::Source => (
                lang.text("Source", "補完元"),
                match entry.source {
                    Source::Help => lang.text("Help (automatic)", "ヘルプ（自動生成）"),
                    Source::Generator => lang.text("Native generator", "生成コマンド"),
                    Source::File => lang.text("Completion file", "補完ファイル"),
                    Source::Native => lang.text("Existing zsh completion", "既存の zsh 補完"),
                }
                .into(),
            ),
            Self::Input => (
                Field::Input.label(lang),
                match entry.source {
                    Source::Help => "--help".into(),
                    Source::Native => lang
                        .text("Uses existing fpath", "既存の fpath を使用")
                        .into(),
                    Source::Generator | Source::File => Field::Input.value(entry),
                },
            ),
            Self::Watch => (Field::Watch.label(lang), Field::Watch.value(entry)),
            Self::Refresh => (
                lang.text("Rebuild changed caches (r)", "変更を確認して再生成（r）"),
                lang.text("Enter: all saved CLIs", "Enter: 保存済みの全 CLI")
                    .into(),
            ),
            Self::ForceRefresh => (
                lang.text(
                    "Force rebuild caches (Shift+R)",
                    "キャッシュを強制再生成（Shift+R）",
                ),
                lang.text("Enter: all saved CLIs", "Enter: 保存済みの全 CLI")
                    .into(),
            ),
        };
        field_item(label, &value, focused, false)
    }
}

/// 詳細画面で選べる行。登録が無いときは戻る行だけにする。
pub(super) fn visible_items(app: &App) -> &'static [DetailItem] {
    if app.completion_detail && !app.config.completions.is_empty() {
        &DetailItem::ALL
    } else {
        &[DetailItem::Back]
    }
}

/// 補完キャッシュの状態の表示名
fn status_label(entry: &Registration, lang: Lang) -> String {
    let status = completion::status(entry);
    match status.as_str() {
        "ready" => lang.text("Ready", "利用可能").into(),
        "pending" => lang.text("Pending", "未生成").into(),
        "native" => lang.text("Provided by zsh", "zsh が提供").into(),
        _ => status,
    }
}

fn next_source(source: &Source) -> Source {
    match source {
        Source::Help => Source::Generator,
        Source::Generator => Source::File,
        Source::File => Source::Native,
        Source::Native => Source::Help,
    }
}

/// 詳細画面で表示する登録を前後に切り替える。端では反対側へ回る。
fn select_registration(app: &mut App, direction: Direction) {
    let len = app.config.completions.len();
    if len > 0 {
        app.completion_index = cycle_index(app.completion_index, len, direction);
    }
}

/// 一覧では登録を選び、詳細画面では行を選ぶ
pub(super) fn move_focus(app: &mut App, direction: Direction) {
    if app.completion_detail {
        move_item(visible_items(app), &mut app.completion_focus, direction);
        return;
    }
    app.completion_index = match direction {
        Direction::Previous => app.completion_index.saturating_sub(1),
        Direction::Next => app
            .completion_index
            .saturating_add(1)
            .min(app.config.completions.len().saturating_sub(1)),
    };
}

/// 左右キーと決定。一覧では決定で詳細画面を開くだけにする。
pub(super) fn handle(app: &mut App, action: ItemAction) {
    if app.completion_detail {
        focused_item(visible_items(app), app.completion_focus).handle(app, action);
    } else if action == ItemAction::Activate && !app.config.completions.is_empty() {
        app.completion_detail = true;
        app.completion_focus = DetailItem::Command;
    }
}

/// 詳細画面を閉じて一覧へ戻る
pub(super) fn close_detail(app: &mut App) {
    app.completion_detail = false;
    app.completion_focus = DetailItem::Back;
}

/// 補完タブ専用のキー
pub(super) fn handle_key(app: &mut App, code: KeyCode) {
    match code {
        KeyCode::Char('n') => add(app),
        KeyCode::Delete | KeyCode::Backspace | KeyCode::Char('d')
            if !app.config.completions.is_empty() =>
        {
            app.completion_delete_pending = true;
            app.status_msg = Some(format!(
                "{}: {}",
                app.config.completions[app.completion_index].command,
                app.lang.text(
                    "Press Backspace (⌫) or d again to remove this CLI.",
                    "もう一度 ⌫ または d でこの CLI を削除します。"
                )
            ));
        }
        KeyCode::Char('r') => refresh(app, false),
        KeyCode::Char('R') => refresh(app, true),
        _ => {}
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
    app.completion_focus = DetailItem::Command;
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
    app.completion_focus = DetailItem::Back;
    app.completion_detail = false;
    app.dirty = true;
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
        render_items(
            frame,
            &DetailItem::ALL,
            app.completion_focus,
            app.lang.text(" CLI completions ", " CLI 補完 "),
            parts[0],
            |item, focused| item.row(app, entry, focused),
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
        "n: Add  Backspace (⌫)/d: Remove  r: Refresh  Shift+R: Force  s: Save\nBinary changes → background rebuild (30s checks at prompts).\nRestart the shell after changing registrations. Failed updates keep the last cache.",
        "n: 追加  Backspace (⌫)/d: 削除  r: 再生成  Shift+R: 強制再生成  s: 保存\nバイナリ変更で自動再生成（プロンプト表示時、30秒間隔で確認）。\n登録変更後はシェルを再起動。更新失敗時は前の補完を維持します。"
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
            let status = status_label(entry, app.lang);
            let source = match entry.source {
                Source::Help => app.lang.text("help", "ヘルプ"),
                Source::Generator => app.lang.text("generator", "生成コマンド"),
                Source::File => app.lang.text("file", "ファイル"),
                Source::Native => app.lang.text("zsh", "zsh"),
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
