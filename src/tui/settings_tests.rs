use super::*;
use ratatui::{Terminal, backend::TestBackend};
use serde_json::Value;

fn press(app: &mut App, code: KeyCode) {
    assert!(!handle_event(app, KeyEvent::from(code)));
}

fn focus(app: &mut App, tab: usize, field: usize) {
    while app.tab != tab {
        press(app, KeyCode::Tab);
    }
    while app.current_focus() > field {
        press(app, KeyCode::Up);
    }
    while app.current_focus() < field {
        press(app, KeyCode::Down);
    }
}

fn replace_input(app: &mut App, text: &str) {
    press(app, KeyCode::Home);
    for _ in 0..app.edit_buffer.chars().count() {
        press(app, KeyCode::Delete);
    }
    for c in text.chars() {
        if c == '\n' {
            handle_event(app, KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT));
        } else {
            press(app, KeyCode::Char(c));
        }
    }
}

fn edit(app: &mut App, tab: usize, field: usize, text: &str) {
    focus(app, tab, field);
    press(app, KeyCode::Enter);
    assert!(app.editing.is_some());
    if app.color_picker.is_some() {
        press(app, KeyCode::Char('e'));
    }
    replace_input(app, text);
    press(app, KeyCode::Enter);
    assert!(app.editing.is_none(), "{:?}", app.status_msg);
}

fn assert_all_fields_changed(before: &Value, after: &Value, path: &str) {
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (key, value) in a {
                assert_all_fields_changed(value, &b[key], &format!("{path}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(b)) if a.first().is_some_and(Value::is_object) => {
            assert_eq!(a.len(), b.len());
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                assert_all_fields_changed(a, b, &format!("{path}[{i}]"));
            }
        }
        _ => assert_ne!(before, after, "TUI から変更できていない設定: {path}"),
    }
}

#[test]
fn 全設定をキー操作で変更してtomlへ保存できる() {
    let mut app = App::new(Config::default(), Lang::En);
    focus(&mut app, 5, 0);
    press(&mut app, KeyCode::Char('n'));
    let before = serde_json::to_value(&app.config).unwrap();
    for (tab, field) in [
        (0, 0),
        (0, 1),
        (0, 2),
        (0, 7),
        (4, 4),
        (2, 0),
        (2, 1),
        (2, 2),
        (3, 0),
        (6, 1),
    ] {
        focus(&mut app, tab, field);
        press(&mut app, KeyCode::Enter);
    }
    for (tab, field, text) in [
        (0, 3, "家"),
        (0, 4, "0"),
        (0, 8, "1"),
        (0, 5, "..."),
        (0, 6, "en7"),
        (3, 1, "fg=12,bold"),
        (3, 2, "42"),
        (4, 0, "cyan"),
        (4, 1, "#123456"),
        (4, 2, "yellow"),
        (4, 3, "240"),
        (5, 1, "日本語"),
        (5, 2, "printf 'a'\nprintf 'b'"),
        (5, 3, "★"),
        (5, 4, "black"),
        (5, 5, "24"),
        (5, 6, "file:Cargo.toml"),
        (6, 0, "/tmp/one:/tmp/two with spaces"),
    ] {
        edit(&mut app, tab, field, text);
    }
    focus(&mut app, 1, 0);
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Right);
    while available_segments(&app.config)[app.seg_avail_state.selected().unwrap()] != "日本語" {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Enter);
    assert!(app.config.prompt.right_segments.contains(&"日本語".into()));
    focus(&mut app, 7, 0);
    press(&mut app, KeyCode::Char('n'));
    replace_input(&mut app, "example");
    press(&mut app, KeyCode::Enter);
    let saved = toml::to_string_pretty(&app.config).unwrap();
    let loaded: Config = toml::from_str(&saved).unwrap();
    let after = serde_json::to_value(loaded).unwrap();
    assert_eq!(after, serde_json::to_value(&app.config).unwrap());
    assert_all_fields_changed(&before, &after, "config");
    assert_eq!(
        app.config.prompt.custom[0].command,
        "printf 'a'\nprintf 'b'"
    );
}

#[test]
fn カスタム名の変更と削除で配置を同期し取消しでは維持する() {
    let mut app = App::new(Config::default(), Lang::En);
    focus(&mut app, 5, 0);
    press(&mut app, KeyCode::Char('n'));
    let name = app.config.prompt.custom[0].name.clone();
    app.config.prompt.left_segments.push(name.clone());
    app.config.prompt.right_segments.push(name.clone());
    edit(&mut app, 5, 1, "renamed");
    assert!(!app.config.prompt.left_segments.contains(&name));
    assert!(app.config.prompt.left_segments.contains(&"renamed".into()));
    assert!(app.config.prompt.right_segments.contains(&"renamed".into()));
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.config.prompt.custom.len(), 1);
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Delete);
    assert!(app.config.prompt.custom.is_empty());
    assert!(!app.config.prompt.left_segments.contains(&"renamed".into()));
    assert!(!app.config.prompt.right_segments.contains(&"renamed".into()));
    assert_eq!(app.custom_focus, 0);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Delete);
}

#[test]
fn 候補を左右へ追加でき配置済み項目も反対側へコピーできる() {
    let mut app = App::new(Config::default(), Lang::En);
    focus(&mut app, 5, 0);
    press(&mut app, KeyCode::Char('n'));
    focus(&mut app, 1, 0);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    while available_segments(&app.config)[app.seg_avail_state.selected().unwrap()] != "custom_1" {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Char('L'));
    assert_eq!(app.config.prompt.left_segments.last().unwrap(), "custom_1");
    assert!(
        !app.config
            .prompt
            .right_segments
            .contains(&"custom_1".into())
    );
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.config.prompt.right_segments.last().unwrap(), "custom_1");
    let right = app.config.prompt.right_segments.clone();
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.config.prompt.right_segments, right);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    let selected = available_segments(&app.config)[app.seg_avail_state.selected().unwrap()].clone();
    press(&mut app, KeyCode::Char('R'));
    assert_eq!(app.config.prompt.right_segments.last(), Some(&selected));
}

#[test]
fn カスタム名の衝突と不正な条件を拒否して編集を継続する() {
    let mut app = App::new(Config::default(), Lang::En);
    focus(&mut app, 5, 0);
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Char('n'));
    for bad in ["", " ", "dir", "custom_1"] {
        focus(&mut app, 5, 1);
        press(&mut app, KeyCode::Enter);
        replace_input(&mut app, bad);
        press(&mut app, KeyCode::Enter);
        assert!(app.editing.is_some());
        assert!(app.status_msg.is_some());
        assert_eq!(app.config.prompt.custom[1].name, "custom_2");
        press(&mut app, KeyCode::Esc);
    }
    focus(&mut app, 5, 6);
    press(&mut app, KeyCode::Enter);
    replace_input(&mut app, "env:");
    press(&mut app, KeyCode::Enter);
    assert!(app.editing.is_some());
    replace_input(&mut app, "env:CI");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.prompt.custom[1].when, "env:CI");
    press(&mut app, KeyCode::Left);
    assert_eq!(app.custom_index, 0);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.custom_index, 1);
}

#[test]
fn 数値は不正値を拒否しゼロと最大値を直接入力できる() {
    let mut app = App::new(Config::default(), Lang::En);
    for (tab, field) in [(0, 4), (3, 2)] {
        for bad in ["", "-1", "1.5", "abc", "9999999999999999999999999999999"] {
            focus(&mut app, tab, field);
            press(&mut app, KeyCode::Enter);
            replace_input(&mut app, bad);
            press(&mut app, KeyCode::Enter);
            assert!(app.editing.is_some());
            assert!(app.status_msg.is_some());
            press(&mut app, KeyCode::Esc);
        }
        edit(&mut app, tab, field, "0");
        press(&mut app, KeyCode::Left);
        edit(&mut app, tab, field, &usize::MAX.to_string());
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(app.config.prompt.dir.truncation_length, usize::MAX);
    assert_eq!(app.config.suggest.max_suggestions, usize::MAX);
}

#[test]
fn 日本語をカーソル移動して編集でき取消しで既存値を保持する() {
    let mut app = App::new(Config::default(), Lang::En);
    edit(&mut app, 0, 5, "日本語");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Char('英'));
    assert_eq!(app.edit_buffer, "日英語");
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::End);
    press(&mut app, KeyCode::Char('!'));
    assert_eq!(app.edit_buffer, "英語!");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.config.prompt.dir.truncation_symbol, "日本語");
}

#[test]
fn 小さい端末でも選択行と長い編集値の末尾を表示する() {
    let mut app = App::new(Config::default(), Lang::En);
    let mut terminal = Terminal::new(TestBackend::new(80, 16)).unwrap();
    focus(&mut app, 0, 6);
    terminal.draw(|frame| ui(frame, &mut app)).unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("IP Interface"));
    focus(&mut app, 5, 0);
    press(&mut app, KeyCode::Char('n'));
    focus(&mut app, 5, 6);
    terminal.draw(|frame| ui(frame, &mut app)).unwrap();
    assert!(format!("{:?}", terminal.backend().buffer()).contains("When"));
    focus(&mut app, 5, 2);
    press(&mut app, KeyCode::Enter);
    replace_input(&mut app, &format!("{}END", "長いコマンド".repeat(40)));
    terminal.draw(|frame| ui(frame, &mut app)).unwrap();
    assert!(format!("{:?}", terminal.backend().buffer()).contains("END"));
    for (width, height) in [(40, 12), (10, 5), (1, 1)] {
        terminal
            .resize(ratatui::layout::Rect::new(0, 0, width, height))
            .unwrap();
        terminal.draw(|frame| ui(frame, &mut app)).unwrap();
    }
}

#[test]
fn プレビューはカスタムコマンドを実行しない() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("should-not-exist");
    let mut app = App::new(Config::default(), Lang::En);
    app.custom_add();
    app.config.prompt.custom[0].command = format!(
        "touch {}",
        crate::shell_single_quote(marker.to_str().unwrap())
    );
    app.config.prompt.left_segments.push("custom_1".into());
    assert!(
        render_preview(&app.config, 80, None)
            .to_string()
            .contains("[custom_1]")
    );
    assert!(!marker.exists());
}

#[test]
fn 全配色をキー操作で選択してプレビューと保存に反映できる() {
    let mut app = App::new(Config::default(), Lang::Ja);
    app.config.prompt.prompt_style = "rainbow".into();
    app.config.prompt.left_segments = vec!["os_icon".into(), "dir".into(), "git".into()];
    focus(&mut app, 4, 4);
    for name in RAINBOW_PALETTES
        .iter()
        .cycle()
        .take(RAINBOW_PALETTES.len() + 1)
    {
        assert_eq!(app.config.style.rainbow_palette, *name);
        let preview = render_preview(&app.config, 100, None);
        let backgrounds: Vec<_> = preview.lines[0]
            .spans
            .iter()
            .filter(|span| span.content.starts_with(' '))
            .filter_map(|span| span.style.bg)
            .collect();
        let expected: Vec<_> = rainbow_palette(name)
            .iter()
            .take(3)
            .map(|(_, bg)| to_ratatui_color(*bg))
            .collect();
        assert_eq!(backgrounds, expected);
        let saved = toml::to_string(&app.config).unwrap();
        let loaded: Config = toml::from_str(&saved).unwrap();
        assert_eq!(loaded.style.rainbow_palette, *name);
        press(&mut app, KeyCode::Right);
        assert!(app.dirty);
    }
    press(&mut app, KeyCode::Left);
    assert_eq!(app.config.style.rainbow_palette, "default");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.config.style.rainbow_palette, "blue");
    assert!(app.editing.is_none());
}

#[test]
fn 入力行の位置を切り替えるとプレビューの記号と右側も移動する() {
    let mut app = App::new(Config::default(), Lang::Ja);
    app.config.prompt.font_level = "ascii".into();
    app.config.prompt.left_segments = vec!["dir".into()];
    app.config.prompt.right_segments = vec!["time".into()];
    focus(&mut app, 0, 7);
    let initial = render_preview(&app.config, 80, None);
    assert_eq!(initial.lines.len(), 2);
    assert!(initial.lines[1].to_string().contains('>'));
    assert!(initial.lines[1].to_string().ends_with("12:34:56"));
    for key in [KeyCode::Enter, KeyCode::Left, KeyCode::Right] {
        let old = app.config.prompt.newline;
        press(&mut app, key);
        assert_ne!(app.config.prompt.newline, old);
        let preview = render_preview(&app.config, 80, None);
        assert_eq!(
            preview.lines.len(),
            if app.config.prompt.newline { 2 } else { 1 }
        );
        assert!(preview.lines.last().unwrap().to_string().contains('>'));
        assert!(
            preview
                .lines
                .last()
                .unwrap()
                .to_string()
                .ends_with("12:34:56")
        );
    }
    assert!(
        !render_preview(&app.config, 5, None)
            .to_string()
            .contains("12:34:56")
    );
}

#[test]
fn escは未保存確認と取消しを区別しqは終了しない() {
    let mut app = App::new(Config::default(), Lang::En);
    press(&mut app, KeyCode::Char('q'));
    assert!(handle_event(&mut app, KeyCode::Esc.into()));
    app.dirty = true;
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.quit_state, QuitState::Confirming);
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(app.quit_state, QuitState::Normal);
    app.start_edit(EditTarget::HomeSymbol);
    press(&mut app, KeyCode::Char('q'));
    assert!(app.edit_buffer.ends_with('q'));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.quit_state, QuitState::Normal);
    assert_eq!(app.config.prompt.dir.home_symbol, "~");
    app.activate_edit(EditTarget::PrimaryColor);
    press(&mut app, KeyCode::Esc);
    assert!(app.color_picker.is_none());
    assert_eq!(app.quit_state, QuitState::Normal);
    press(&mut app, KeyCode::Esc);
    assert!(handle_event(&mut app, KeyCode::Esc.into()));
}

#[test]
fn ブロックの色を選択して保存しセット変更後も維持と解除ができる() {
    let mut config = Config::default();
    config.prompt.prompt_style = "rainbow".into();
    config.prompt.left_segments = vec!["os_icon".into(), "dir".into(), "git".into()];
    let mut app = App::new(config, Lang::Ja);
    focus(&mut app, 4, 5);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.rainbow_selected(), Some("dir"));
    assert!(!app.dirty);
    focus(&mut app, 4, 6);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Esc);
    assert!(!app.dirty);
    assert!(app.config.style.rainbow_overrides.is_empty());
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Home);
    press(&mut app, KeyCode::Enter);
    edit(&mut app, 4, 7, "#f0c674");
    let pair = (AppColor::Ansi256(0), AppColor::Rgb(240, 198, 116));
    assert_eq!(rainbow_colors(&app.config.style, "dir", 1), pair);
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|f| ui(f, &mut app)).unwrap();
    let sample = app.rainbow_sample("dir", Some((false, "88")));
    assert_eq!(sample.fg, Some(Color::Indexed(0)));
    assert_eq!(sample.bg, Some(Color::Indexed(88)));
    focus(&mut app, 4, 4);
    press(&mut app, KeyCode::Right);
    assert_eq!(rainbow_colors(&app.config.style, "dir", 1), pair);
    app.config.prompt.left_segments.swap(0, 1);
    assert_eq!(rainbow_colors(&app.config.style, "dir", 0), pair);
    let saved = toml::to_string_pretty(&app.config).unwrap();
    let loaded: Config = toml::from_str(&saved).unwrap();
    assert_eq!(rainbow_colors(&loaded.style, "dir", 0), pair);
    app.rainbow_index = 0;
    edit(&mut app, 4, 6, "");
    assert!(app.config.style.rainbow_overrides["dir"].fg.is_none());
    assert_eq!(
        app.config.style.rainbow_overrides["dir"].bg.as_deref(),
        Some("#f0c674")
    );
    app.set_rainbow_color("git".into(), false, "88".into());
    focus(&mut app, 4, 8);
    press(&mut app, KeyCode::Enter);
    assert!(!app.config.style.rainbow_overrides.contains_key("dir"));
    assert!(app.config.style.rainbow_overrides.contains_key("git"));
}

#[test]
fn カスタムの改名と削除に個別配色が追従する() {
    let mut app = App::new(Config::default(), Lang::En);
    app.custom_add();
    let name = app.config.prompt.custom[0].name.clone();
    app.config.prompt.left_segments = vec![name.clone()];
    edit(&mut app, 4, 7, "88");
    edit(&mut app, 5, 1, "renamed");
    assert!(!app.config.style.rainbow_overrides.contains_key(&name));
    assert_eq!(
        app.config.style.rainbow_overrides["renamed"].bg.as_deref(),
        Some("88")
    );
    press(&mut app, KeyCode::Delete);
    press(&mut app, KeyCode::Delete);
    assert!(app.config.style.rainbow_overrides.is_empty());
}

#[test]
fn ブロックを減らした後や空の配置でも色選択が壊れない() {
    let mut app = App::new(Config::default(), Lang::En);
    app.rainbow_index = usize::MAX;
    app.config.prompt.left_segments = vec!["missing".into(), "dir".into(), "dir".into()];
    assert_eq!(app.rainbow_selected(), Some("dir"));
    app.rainbow_select(true);
    assert_eq!(app.rainbow_selected(), Some("dir"));
    app.config.prompt.left_segments.clear();
    app.tab = 4;
    assert_eq!(app.max_focus(), 5);
    app.style_focus = 5;
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Left);
    assert!(app.rainbow_selected().is_none());
    assert!(!app.dirty);
    let mut terminal = Terminal::new(TestBackend::new(40, 12)).unwrap();
    terminal.draw(|f| ui(f, &mut app)).unwrap();
}

#[test]
fn 空行数を左右キーと直接入力で変更しプレビューと保存に反映する() {
    let mut app = App::new(Config::default(), Lang::Ja);
    app.config.prompt.left_segments = vec!["dir".into()];
    app.config.prompt.right_segments.clear();
    focus(&mut app, 0, 8);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.config.prompt.blank_lines, 1);
    press(&mut app, KeyCode::Left);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.config.prompt.blank_lines, 0);
    edit(&mut app, 0, 8, "2");
    for newline in [false, true] {
        app.config.prompt.newline = newline;
        let preview = render_spacing_preview(&app.config, 80, None);
        assert_eq!(
            preview
                .lines
                .iter()
                .filter(|line| line.spans.is_empty())
                .count(),
            2
        );
        assert_eq!(preview.to_string().matches("~/project").count(), 2);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
        assert_eq!(
            format!("{:?}", terminal.backend().buffer())
                .matches("~/project")
                .count(),
            2
        );
    }
    let loaded: Config = toml::from_str(&toml::to_string(&app.config).unwrap()).unwrap();
    assert_eq!(loaded.prompt.blank_lines, 2);
    app.dirty = false;
    press(&mut app, KeyCode::Enter);
    replace_input(&mut app, "3");
    let preview = render_spacing_preview(
        &app.config,
        80,
        Some((app.editing.as_ref().unwrap(), &app.edit_buffer)),
    );
    assert_eq!(
        preview
            .lines
            .iter()
            .filter(|line| line.spans.is_empty())
            .count(),
        3
    );
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.config.prompt.blank_lines, 2);
    assert!(!app.dirty);
}

#[test]
fn 空行数の範囲外を拒否し極端な設定でも加算と描画が壊れない() {
    let mut app = App::new(Config::default(), Lang::En);
    focus(&mut app, 0, 8);
    for value in ["-1", "11", "1.5", "invalid", ""] {
        press(&mut app, KeyCode::Enter);
        replace_input(&mut app, value);
        press(&mut app, KeyCode::Enter);
        assert!(app.editing.is_some());
        assert_eq!(app.config.prompt.blank_lines, 0);
        assert!(app.status_msg.as_deref().unwrap().contains("0 to 10"));
        press(&mut app, KeyCode::Esc);
    }
    app.config.prompt.blank_lines = usize::MAX;
    press(&mut app, KeyCode::Right);
    assert_eq!(
        app.config.prompt.blank_lines,
        config::MAX_PROMPT_BLANK_LINES
    );
    press(&mut app, KeyCode::Right);
    assert_eq!(
        app.config.prompt.blank_lines,
        config::MAX_PROMPT_BLANK_LINES
    );
    for (width, height) in [(80, 24), (40, 18), (1, 1)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|f| ui(f, &mut app)).unwrap();
    }
}

#[test]
fn 補完登録を編集し切替と削除ができる() {
    let mut app = App::new(Config::default(), Lang::Ja);
    focus(&mut app, 7, 0);
    press(&mut app, KeyCode::Char('n'));
    replace_input(&mut app, "example");
    press(&mut app, KeyCode::Enter);
    focus(&mut app, 7, 3);
    press(&mut app, KeyCode::Enter);
    edit(&mut app, 7, 4, r#"["example", "completion", "zsh"]"#);
    edit(&mut app, 7, 5, r#"["~/bin/actual"]"#);
    focus(&mut app, 7, 2);
    press(&mut app, KeyCode::Enter);
    assert!(!app.config.completions[0].enabled);
    let data = toml::to_string(&app.config).unwrap();
    let saved: Config = toml::from_str(&data).unwrap();
    assert_eq!(saved.completions, app.config.completions);
    focus(&mut app, 7, 3);
    press(&mut app, KeyCode::Enter);
    edit(&mut app, 7, 4, "~/completions/_example");
    assert_eq!(app.config.completions[0].file, "~/completions/_example");
    press(&mut app, KeyCode::Delete);
    assert!(app.config.completions.is_empty());
    assert_eq!(app.current_focus(), 0);
}

#[test]
fn 補完キャッシュの通常再生成と強制再生成を選択できる() {
    let mut app = App::new(Config::default(), Lang::Ja);
    focus(&mut app, 7, 0);
    press(&mut app, KeyCode::Char('n'));
    replace_input(&mut app, "example");
    press(&mut app, KeyCode::Enter);
    for row in [6, 7] {
        focus(&mut app, 7, row);
        press(&mut app, KeyCode::Enter);
        assert!(app.status_msg.as_deref().unwrap().contains("先に s"));
    }
    for code in [KeyCode::Char('r'), KeyCode::Char('R')] {
        press(&mut app, code);
        assert!(app.status_msg.as_deref().unwrap().contains("先に s"));
    }
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal.draw(|frame| ui(frame, &mut app)).unwrap();
    let rendered = format!("{:?}", terminal.backend().buffer());
    assert!(rendered.contains("変更を確認して再生成"));
    assert!(rendered.contains("キャッシュを強制再生成"));
    let (sender, receiver) = std::sync::mpsc::channel();
    app.completion_job = Some(receiver);
    sender.send("example: updated".into()).unwrap();
    completions::poll_refresh(&mut app);
    assert!(app.completion_job.is_none());
    assert!(
        app.status_msg
            .as_deref()
            .unwrap()
            .contains("example: updated")
    );
}
