use super::*;
use ratatui::{Terminal, backend::TestBackend};

fn screen(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui(frame, app)).unwrap();
    format!("{:?}", terminal.backend().buffer())
}

#[test]
fn 全タブの見出しと項目が選択言語で表示される() {
    for (lang, labels) in [
        (
            Lang::Ja,
            [
                "スタイル",
                "追加候補",
                "変更状態を表示",
                "候補の検索方法",
                "基本色",
                "カスタムセグメントはありません",
                "端末との連携",
                "補完対象",
            ],
        ),
        (
            Lang::En,
            [
                "Prompt Style",
                "Available",
                "Show Status",
                "Strategy",
                "Primary Color",
                "No custom segments",
                "Terminal Integration",
                "register a CLI",
            ],
        ),
    ] {
        let mut app = App::new(Config::default(), lang);
        let before = toml::to_string(&app.config).unwrap();
        for (tab, label) in labels.into_iter().enumerate() {
            app.tab = tab;
            let rendered = screen(&mut app, 100, 24);
            assert!(rendered.contains(label), "{lang:?}/{tab}: {rendered}");
            if lang == Lang::En {
                assert!(
                    !rendered
                        .chars()
                        .any(|c| ('\u{3040}'..='\u{30ff}').contains(&c))
                );
            }
            for (width, height) in [(80, 24), (40, 18), (1, 1)] {
                screen(&mut app, width, height);
            }
        }
        assert_eq!(toml::to_string(&app.config).unwrap(), before);
    }
}

#[test]
fn 日本語の検証メッセージと編集画面と確認が表示される() {
    let mut app = App::new(Config::default(), Lang::Ja);
    app.start_edit(EditTarget::TruncationLength);
    app.edit_buffer = "-1".into();
    app.edit_cursor = 2;
    app.confirm_edit();
    let rendered = screen(&mut app, 100, 24);
    assert!(rendered.contains("編集: 表示する階層数"));
    assert!(rendered.contains("0 以上の整数"));
    app.cancel_edit();
    app.custom_add();
    assert!(app.status_msg.as_deref().unwrap().contains("作成しました"));
    app.tab = 5;
    handle_event(&mut app, KeyCode::Delete.into());
    let rendered = screen(&mut app, 100, 24);
    assert!(rendered.contains("定義と配置を削除しますか？"));
    handle_event(&mut app, KeyCode::Esc.into());
    handle_event(&mut app, KeyCode::Esc.into());
    assert!(screen(&mut app, 100, 24).contains("未保存の変更があります"));
}

#[test]
fn 色選択とカスタム項目も言語が統一される() {
    for lang in [Lang::En, Lang::Ja] {
        let mut app = App::new(Config::default(), lang);
        app.custom_add();
        app.tab = 5;
        let rendered = screen(&mut app, 100, 24);
        assert!(rendered.contains(lang.text("Foreground", "文字色")));
        assert!(rendered.contains(lang.text("When", "表示条件")));
        app.activate_edit(EditTarget::PrimaryColor);
        colors::handle_picker(&mut app, KeyCode::End.into());
        for (width, height) in [(80, 24), (40, 18), (20, 4)] {
            let rendered = screen(&mut app, width, height);
            assert!(rendered.contains(">255<"));
            if width >= 40 {
                assert!(rendered.contains(lang.text("256 colors", "256 色")));
                assert!(rendered.contains(lang.text("Sample text", "サンプル")));
                assert!(rendered.contains(lang.text("Esc: cancel", "Esc:取消")));
            }
        }
        assert_eq!(app.config.style.primary_color, "blue");
    }
}

#[test]
fn 日本語入力を文字境界で編集できる() {
    let mut app = App::new(Config::default(), Lang::Ja);
    app.start_edit(EditTarget::HomeSymbol);
    app.edit_buffer = "日本語".into();
    app.edit_cursor = app.edit_buffer.len();
    handle_event(&mut app, KeyCode::Left.into());
    handle_event(&mut app, KeyCode::Backspace.into());
    assert_eq!(app.edit_buffer, "日語");
    assert!(screen(&mut app, 40, 18).contains("日▏語"));
    handle_event(&mut app, KeyCode::Enter.into());
    assert_eq!(app.config.prompt.dir.home_symbol, "日語");
}
