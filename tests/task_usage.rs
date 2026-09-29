use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn cli(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zsh-turbo"));
    command
        .current_dir(root.join("project"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env_remove("HISTFILE");
    command
}

fn setup(makefile: &str, history: &str) -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("project")).unwrap();
    std::fs::create_dir_all(tmp.path().join("config/zsh-turbo")).unwrap();
    std::fs::write(tmp.path().join("project/Makefile"), makefile).unwrap();
    std::fs::write(tmp.path().join("history"), history).unwrap();
    tmp
}

fn ui_list(root: &Path, buffer: &str) -> String {
    let output = cli(root)
        .args(["suggest", "--ui-list", "--history-file"])
        .arg(root.join("history"))
        .args(["--", buffer])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn suggest(root: &Path, buffer: &str) -> String {
    let output = cli(root)
        .args(["suggest", "--history-file"])
        .arg(root.join("history"))
        .args(["--", buffer])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

fn record(root: &Path, line: &str) {
    let mut child = cli(root)
        .arg("record")
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(line.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
}

fn store(root: &Path) -> String {
    std::fs::read_to_string(root.join("state/zsh-turbo/task-usage.json")).unwrap_or_default()
}

/// 応答から (ghost, select, item の BUFFER の並び) を取り出す
fn parse(response: &str) -> (Option<&str>, Option<&str>, Vec<&str>) {
    let field = |tag: &str| {
        response
            .lines()
            .find_map(|line| line.strip_prefix(tag)?.strip_prefix('\t'))
    };
    let items = response
        .lines()
        .filter_map(|line| line.strip_prefix("item\t")?.split_once('\t'))
        .map(|(_, value)| value)
        .collect();
    (field("ghost"), field("select"), items)
}

#[test]
fn よく使うタスクを初期選択とghostにしディレクトリでの利用が重なると上書きする() {
    let tmp = setup(
        "build:\ncheck:\ninstall:\ntest:\n",
        "make install\nmake install\nmake test\n",
    );
    let root = tmp.path();

    // 履歴全体では install が多いので、名前順の先頭 build ではなく install を選ぶ
    let response = ui_list(root, "make");
    let (ghost, select, items) = parse(&response);
    assert_eq!(
        items,
        ["make build", "make check", "make install", "make test"]
    );
    assert_eq!(
        (ghost, select),
        (Some("make install"), Some("3")),
        "{response}"
    );
    assert_eq!(suggest(root, "make"), "make install");
    // 絞り込み中も同じ規則で選ぶ
    let response = ui_list(root, "make ");
    assert!(response.contains("ghost\tmake install\n"), "{response}");
    let response = ui_list(root, "make t");
    let (ghost, select, _) = parse(&response);
    // 候補が 1 つなら選択位置は送らない (先頭のまま)
    assert_eq!((ghost, select), (Some("make test"), None));

    // このディレクトリで test を重ねて使うと上書きする
    for _ in 0..3 {
        record(root, "make test");
    }
    let response = ui_list(root, "make");
    let (ghost, select, _) = parse(&response);
    assert_eq!((ghost, select), (Some("make test"), Some("4")));
    assert_eq!(suggest(root, "make"), "make test");

    // 先頭が空白の行・定義にないタスクは記録しない
    record(root, " make build");
    record(root, "make unknown");
    let saved = store(root);
    assert!(saved.contains(r#""task":"test""#), "{saved}");
    assert!(
        !saved.contains("build") && !saved.contains("unknown"),
        "{saved}"
    );
}

#[test]
fn 記録を無効にすると記録せずここでの利用も使わない() {
    let tmp = setup("build:\ninstall:\ntest:\n", "make install\n");
    let root = tmp.path();
    for _ in 0..5 {
        record(root, "make test");
    }
    assert_eq!(parse(&ui_list(root, "make")).0, Some("make test"));

    std::fs::write(
        root.join("config/zsh-turbo/config.toml"),
        "[suggest]\nrecord_task_usage = false\n",
    )
    .unwrap();
    let before = store(root);
    record(root, "make build");
    assert_eq!(store(root), before);
    // 履歴全体の傾向だけで選ぶ
    assert_eq!(parse(&ui_list(root, "make")).0, Some("make install"));

    // 設定が読めないときも既定 (有効) に戻さず記録しない
    std::fs::write(
        root.join("config/zsh-turbo/config.toml"),
        "[suggest]\nrecord_task_usage = false\n[prompt\n",
    )
    .unwrap();
    record(root, "make build");
    assert_eq!(store(root), before);
}

#[test]
fn 入力中の表示行数を超えるタスクもすべて送り選んだ位置を返す() {
    let makefile: String = (1..=11)
        .map(|n| format!("a{n:02}:\n"))
        .chain(["zeta:\n".to_owned()])
        .collect();
    let tmp = setup(&makefile, "make zeta\n");
    let response = ui_list(tmp.path(), "make");
    // 見出しは 12 件・入力中の表示は既定の 10 行
    assert!(
        response.starts_with("v1\ttasks\t12\tcomplete\t10\t"),
        "{response}"
    );
    let (ghost, select, items) = parse(&response);
    assert_eq!(items.len(), 12);
    assert_eq!((ghost, select), (Some("make zeta"), Some("12")));
}

#[test]
fn 利用の記録も履歴もなければ先頭のタスクをghostにし選択位置を送らない() {
    let tmp = setup("build:\ninstall:\n", "make busted\n");
    let response = ui_list(tmp.path(), "make");
    let (ghost, select, _) = parse(&response);
    assert_eq!((ghost, select), (Some("make build"), None));
}
