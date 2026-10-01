emulate -L zsh
zmodload zsh/zpty || exit 1
zmodload zsh/zselect || exit 1
# 各待ちの上限 (0.05 秒単位)。条件が揃えばすぐ抜けるので、高負荷の端末で初回 compinit が
# 数秒かかっても落ちないよう余裕を持たせる。
typeset -gi fixture_wait=400
local_output=''
function drain_fixture() {
    while zpty -r fixture local_output 2>/dev/null; do
        print -rn -- "$local_output" >> "$TEST_ROOT/terminal"
    done
}
function snapshot_fixture() {
    : > "$TEST_ROOT/ready"
    zpty -w -n fixture $'\e[24~'
    repeat $fixture_wait; do
        drain_fixture
        [[ -s "$TEST_ROOT/ready" ]] && return 0
        zselect -t 5
    done
    print -u2 -- 'snapshot timeout'
    cat "$TEST_ROOT/terminal" >&2
    return 1
}
function await_fixture() {
    local field="$1" expected="$2" mode="${3:-exact}" actual
    repeat $fixture_wait; do
        snapshot_fixture || return 1
        actual="$(<"$TEST_ROOT/$field")"
        [[ "$actual" == "$expected" ]] && return 0
        [[ "$mode" == contains && "$actual" == *"$expected"* ]] && return 0
        zselect -t 5
    done
    print -u2 -- "unexpected $field: $(<"$TEST_ROOT/$field") (expected $expected)"
    return 1
}
# メニューの描画 (入力欄と一覧) が期待どおりになるまで待つ
function await_menu() {
    local expected="$1" actual=""
    repeat $fixture_wait; do
        drain_fixture
        [[ -f "$TEST_ROOT/menu" ]] && actual="$(<"$TEST_ROOT/menu")"
        [[ "$actual" == "$expected" ]] && return 0
        zselect -t 5
    done
    print -u2 -- "unexpected menu: $actual (expected $expected)"
    return 1
}
# メニューで項目が選ばれて描かれるまで待つ。入力中の一覧で既にその色が付いていると、
# ZLE は差分だけを描くため、端末出力に `> 項目` が一続きで現れない
function await_selected() {
    local expected="$1" actual=""
    repeat $fixture_wait; do
        drain_fixture
        [[ -f "$TEST_ROOT/menu" ]] && actual="$(<"$TEST_ROOT/menu")"
        # サブコマンドの行は説明が続くため、名前の後ろは空白か行末で区切る
        [[ $'\n'"$actual"$'\n' == *$'\n> '"$expected"[$' \n']* ]] && return 0
        zselect -t 5
    done
    print -u2 -- "unexpected menu: $actual (expected > $expected)"
    return 1
}
function await_terminal() {
    emulate -L zsh
    setopt extendedglob
    local expected="$1" actual
    repeat $fixture_wait; do
        drain_fixture
        actual="$(<"$TEST_ROOT/terminal")"
        actual="${actual//$'\e'\[[0-9\;]##m/}"
        [[ "$actual" == *"$expected"* ]] && return 0
        zselect -t 5
    done
    print -u2 -- "terminal missing $expected"
    tail -c 1200 "$TEST_ROOT/terminal" >&2
    return 1
}
function history_key_fixture() {
    local key="$1" expected="$2"
    zpty -w -n fixture "$key"
    repeat $fixture_wait; do
        drain_fixture
        if [[ -f "$TEST_ROOT/history-buffer" ]] && [[ "$(<"$TEST_ROOT/history-buffer")" == "$expected" ]]; then
            zselect -t 20
            drain_fixture
            return 0
        fi
        zselect -t 5
    done
    print -u2 -- "history buffer: $(<"$TEST_ROOT/history-buffer") (expected $expected)"
    return 1
}
zpty -b fixture zsh -di || exit 1
{
    repeat $fixture_wait; do
        drain_fixture
        [[ -s "$TEST_ROOT/boot" ]] && break
        zselect -t 5
    done
    [[ -s "$TEST_ROOT/boot" ]] || { cat "$TEST_ROOT/terminal" >&2; exit 2; }
    # 観測用ウィジェットを挟まず、非同期描画の完了後に続けて履歴をたどる。
    for expected_history in 'pnpm deploy' 'make busted' 'ls -l /path/to/hoge/fuga' 'echo 日本語'; do
        history_key_fixture $'\e[A' "$expected_history" || exit 80
    done
    for expected_history in 'ls -l /path/to/hoge/fuga' 'make busted' 'pnpm deploy' ''; do
        history_key_fixture $'\e[B' "$expected_history" || exit 81
    done
    history_key_fixture 'echo sam' 'echo sam' || exit 82
    history_key_fixture $'\e[A' 'echo sample-beta' || exit 82
    history_key_fixture $'\e[A' 'echo sample-alpha' || exit 82
    history_key_fixture $'\e[B' 'echo sample-beta' || exit 83
    history_key_fixture $'\eOA' 'echo sample-alpha' || exit 83
    history_key_fixture $'\eOB' 'echo sample-beta' || exit 83
    history_key_fixture $'\e[B' 'echo sam' || exit 83
    history_key_fixture $'\x15ls' 'ls' || exit 84
    history_key_fixture $'\e[A' 'ls -l /path/to/hoge/fuga' || exit 84
    history_key_fixture $'\x15' '' || exit 84
    zpty -w -n fixture 'make'
    await_fixture project $'make build\nmake check\nmake clean\nmake deploy' || exit 77
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'build' || exit 78
    zpty -w -n fixture $'\e[B\r'
    await_fixture buffer 'make check' || exit 79
    zpty -w -n fixture $'\x15'
    zpty -w -n fixture 'echo sam'
    await_fixture ghost 'ple-alpha' || exit 3
    await_fixture highlight '0 4 ' contains || exit 4
    zpty -w -n fixture $'\e[D'
    await_fixture ghost '' || exit 5
    zpty -w -n fixture $'\x05'
    await_fixture ghost 'ple-alpha' || exit 6
    zpty -w -n fixture $'\e[C'
    await_fixture buffer 'echo sample-alpha' || exit 7
    await_fixture ghost '' || exit 8
    zpty -w -n fixture $'\x15echo sam\x12\e[B\r'
    await_fixture buffer 'echo sample-beta' || exit 9
    zpty -w -n fixture $'\x15echo sam\x12\e[B\e'
    await_fixture buffer 'echo sam' || exit 10
    await_fixture ghost 'ple-alpha' || exit 11
    zpty -w -n fixture $'\x15echo newer-fixture\r'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 2 ]] && break
        zselect -t 5
    done
    [[ "$(<"$TEST_ROOT/boot")" == 2 ]] || exit 12
    await_fixture buffer '' || exit 12
    zpty -w -n fixture 'echo new'
    await_fixture ghost 'er-fixture' || exit 13
    zpty -w -n fixture $'\x15echo 日'
    await_fixture ghost '本語' || exit 14
    zpty -w -n fixture $'\x15cd cand\t'
    await_fixture buffer 'cd candidate-dir/' || exit 15
    zpty -w -n fixture $'\x15zsh-turbo conf\t'
    await_fixture buffer 'zsh-turbo configure ' || exit 16
    zpty -w -n fixture $'\x15zsh-turbo install-font --fo\t'
    await_fixture buffer 'zsh-turbo install-font --force ' || exit 17
    drain_fixture
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15echo sam\x12\e[B'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/terminal")" == *'[2/2]'* ]] && break
        zselect -t 5
    done
    [[ "$(<"$TEST_ROOT/terminal")" == *'[2/2]'* ]] || exit 18
    zpty -w -n fixture $'\x03'
    await_fixture buffer 'echo sam' || exit 18
    await_fixture ghost 'ple-alpha' || exit 19
    zpty -w -n fixture $'\x01\x12\e[B\e'
    await_fixture buffer 'echo sam' || exit 20
    await_fixture cursor '0' || exit 21
    await_fixture ghost '' || exit 22
    zpty -w -n fixture $'\x05\x15'
    await_fixture fds '0 0' || exit 23
    zpty -w -n fixture $'print -u2 -- stderr-visible-$((20+5))\r'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 3 ]] && break
        zselect -t 5
    done
    await_fixture buffer '' || exit 24
    [[ "$(<"$TEST_ROOT/terminal")" == *'stderr-visible-25'* ]] || exit 25
    zpty -w -n fixture $'\e[23~make'
    await_fixture project 'make build' contains || exit 66
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'build' || exit 67
    zpty -w -n fixture $'\e[Z'
    await_selected 'deploy' || exit 68
    zpty -w -n fixture $'\r'
    await_fixture buffer 'make deploy' || exit 69
    zpty -w -n fixture $'\x15echo sam'
    await_fixture ghost 'ple-alpha' || exit 26
    zpty -w -n fixture $'\e[C'
    await_fixture buffer 'echo sample-alpha' || exit 27
    zpty -w -n fixture $'\x15echo sam\x12\t\r'
    await_fixture buffer 'echo sample-beta' || exit 28
    zpty -w -n fixture $'\x15ls -l'
    await_fixture ghost ' /path/to/hoge/fuga' || exit 29
    zpty -w -n fixture $'\e[C'
    await_fixture buffer 'ls -l /path/' || exit 30
    await_fixture ghost 'to/hoge/fuga' || exit 31
    zpty -w -n fixture $'\x15ls -l /path'
    await_fixture ghost '/to/hoge/fuga' || exit 32
    zpty -w -n fixture $'\e[C'
    await_fixture buffer 'ls -l /path/to/' || exit 32
    await_fixture ghost 'hoge/fuga' || exit 33
    zpty -w -n fixture $'\t'
    await_fixture buffer 'ls -l /path/to/hoge/fuga' || exit 34
    zpty -w -n fixture $'\x15make bu'
    await_fixture ghost 'ild' || exit 35
    zpty -w -n fixture $'\x15make'
    await_fixture project $'make build\nmake check\nmake clean\nmake deploy' || exit 51
    await_fixture display $' build\nTasks (4):\n  build\n  check\n  clean\n  deploy' || exit 52
    drain_fixture
    [[ "$(<"$TEST_ROOT/terminal")" == *'Tasks (4):'* ]] || exit 52
    zpty -w -n fixture $'\e[D'
    await_fixture display '' || exit 52
    zpty -w -n fixture $'\e[C'
    await_fixture display $' build\nTasks (4):\n  build\n  check\n  clean\n  deploy' || exit 52
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'build' || exit 56
    [[ "$(<"$TEST_ROOT/terminal")" == *$'\e[36m'* ]] || exit 56
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'check' || exit 57
    [[ "$(<"$TEST_ROOT/terminal")" == *$'\e[36m'* ]] || exit 57
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[A'
    await_selected 'build' || exit 58
    zpty -w -n fixture $'\e'
    await_fixture buffer 'make' || exit 59
    await_fixture project 'make build' contains || exit 60
    zpty -w -n fixture $'\e[B\e[B\r'
    await_fixture buffer 'make check' || exit 61
    await_fixture project '' || exit 62
    zpty -w -n fixture $'\x15make'
    await_fixture project 'make build' contains || exit 63
    zpty -w -n fixture $'\t'
    await_fixture buffer 'make build' || exit 52
    await_fixture project '' || exit 52
    zpty -w -n fixture $'\x15pnpm run'
    await_fixture project $'pnpm run dev\npnpm run test' || exit 53
    await_fixture display $' dev\nTasks (2):\n  dev\n  test' || exit 54
    zpty -w -n fixture $'\e[B\e[B\r'
    await_fixture buffer 'pnpm run test' || exit 64
    await_fixture project '' || exit 65
    # pnpm のサブコマンドを選んで Tab で確定しても、次候補へ移動せず実行もしない
    zpty -w -n fixture $'\x15pnpm'
    await_fixture project $'pnpm run \npnpm self \npnpm sync ' || exit 64
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'run' || exit 64
    zpty -w -n fixture $'\e[B'
    await_selected 'self' || exit 64
    zpty -w -n fixture $'\t'
    await_menu closed || exit 64
    await_fixture buffer 'pnpm self ' || exit 64
    [[ "$(<"$TEST_ROOT/boot")" == 3 ]] || exit 64
    zpty -w -n fixture $'\x15echo sam'
    await_fixture project '' || exit 55
    await_fixture display 'ple-alpha' || exit 55
    zpty -w -n fixture $'\t'
    await_fixture buffer 'echo sample-alpha' || exit 36
    zpty -w -n fixture $'\x15pnpm run de'
    await_fixture ghost 'v' || exit 37
    zpty -w -n fixture $'\x15bun run de'
    await_fixture ghost 'v' || exit 38
    zpty -w -n fixture $'\x15uv run he'
    await_fixture ghost 'llo' || exit 39
    zpty -w -n fixture $'\x15npm run de'
    await_fixture ghost 'v' || exit 39
    zpty -w -n fixture $'\x15yarn run de'
    await_fixture ghost 'v' || exit 39
    zpty -w -n fixture $'\x15deno task ch'
    await_fixture ghost 'eck' || exit 39
    zpty -w -n fixture $'\x15mise run bu'
    await_fixture ghost 'ild' || exit 39
    zpty -w -n fixture $'\x15just ch'
    await_fixture ghost 'eck' || exit 39
    zpty -w -n fixture $'\x15task bu'
    await_fixture ghost 'ild' || exit 39
    zpty -w -n fixture $'\x15make\r'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 4 ]] && break
        zselect -t 5
    done
    await_fixture display '' || exit 39
    zpty -w -n fixture $'make build\r'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 5 ]] && break
        zselect -t 5
    done
    [[ "$(<"$TEST_ROOT/boot")" == 5 ]] || exit 70
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'make\e[B'
    await_selected 'build' || exit 75
    zpty -w -n fixture $'\e'
    await_fixture buffer 'make' || exit 76
    zpty -w -n fixture $'\x15'
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[A\e[A'
    await_terminal 'Tasks (4):' || exit 71
    zpty -w -n fixture $'\e[B'
    await_selected 'build' || exit 73
    zpty -w -n fixture $'\e'
    await_fixture buffer 'make' || exit 74
    await_fixture project $'make build\nmake check\nmake clean\nmake deploy' || exit 72
    zpty -w -n fixture $'\x15ZSH_TURBO_KEY_TAB=default\r'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 6 ]] && break
        zselect -t 5
    done
    [[ "$(<"$TEST_ROOT/boot")" == 6 ]] || exit 40
    zpty -w -n fixture $'make bu\t'
    await_fixture buffer 'make build ' || exit 41
    zpty -w -n fixture $'\x15pnpm run de\t'
    await_fixture buffer 'pnpm run dev ' || exit 42
    zpty -w -n fixture $'\x15bun run de\t'
    await_fixture buffer 'bun run dev ' || exit 43
    zpty -w -n fixture $'\x15uv run he\t'
    await_fixture buffer 'uv run hello ' || exit 44
    zpty -w -n fixture $'\x15npm run de\t'
    await_fixture buffer 'npm run dev ' || exit 45
    zpty -w -n fixture $'\x15yarn run de\t'
    await_fixture buffer 'yarn run dev ' || exit 46
    zpty -w -n fixture $'\x15deno task ch\t'
    await_fixture buffer 'deno task check ' || exit 47
    zpty -w -n fixture $'\x15mise run bu\t'
    await_fixture buffer 'mise run build ' || exit 48
    zpty -w -n fixture $'\x15just ch\t'
    await_fixture buffer 'just check ' || exit 49
    zpty -w -n fixture $'\x15task bu\t'
    await_fixture buffer 'task build ' || exit 50
    # ファイル一覧: 入力中のパスのディレクトリ内を下に出し、↓ で選んで Enter で入れる
    zpty -w -n fixture $'\x15ls -l books/'
    await_fixture labels $'sub/\nAlpha Beta.txt\nalpha.md\nパン.txt' || exit 85
    await_fixture project $'ls -l books/sub/\nls -l books/Alpha\\ Beta.txt \nls -l books/alpha.md \nls -l books/パン.txt ' || exit 85
    await_fixture display $'sub/\nFiles (4):\n  sub/\n  Alpha Beta.txt\n  alpha.md\n  パン.txt' || exit 86
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'sub/' || exit 87
    zpty -w -n fixture $'\e[B\t'
    await_menu closed || exit 88
    await_fixture buffer 'ls -l books/Alpha\ Beta.txt ' || exit 88
    await_fixture fds '0 0' || exit 88
    await_fixture project '' || exit 88
    # 前方一致で絞り、大文字小文字まで一致するものを先に出す
    zpty -w -n fixture $'\x15ls -l books/al'
    await_fixture labels $'alpha.md\nAlpha Beta.txt' || exit 89
    await_fixture ghost 'pha.md ' || exit 89
    # IME の NFC 入力で NFD のファイル名を選び、ディスク上の名前のまま入れる
    zpty -w -n fixture $'\x15ls -l books/パ'
    await_fixture labels $'パン.txt' || exit 90
    zpty -w -n fixture $'\e[B\r'
    await_fixture buffer $'ls -l books/パン.txt ' || exit 90
    # ディレクトリを選ぶとその中の一覧へ切り替わる
    zpty -w -n fixture $'\x15ls -l books/s'
    await_fixture labels 'sub/' || exit 91
    zpty -w -n fixture $'\e[B\r'
    await_fixture buffer 'ls -l books/sub/' || exit 91
    await_fixture labels 'inner.txt' || exit 92
    zpty -w -n fixture $'\x15ls -l books/.'
    await_fixture labels '.hidden' || exit 93
    # Esc で元の入力へ戻し、一覧も戻る。入力直後の ↓ でも結果を待って開く
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15ls -l books/\e[B'
    await_selected 'sub/' || exit 94
    zpty -w -n fixture $'\e'
    await_fixture buffer 'ls -l books/' || exit 94
    await_fixture labels 'sub/' contains || exit 94
    # 履歴で呼び出した行には一覧を出さず、↓ は履歴を戻る
    zpty -w -n fixture $'\x15: books/\r'
    repeat $fixture_wait; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 7 ]] && break
        zselect -t 5
    done
    [[ "$(<"$TEST_ROOT/boot")" == 7 ]] || exit 95
    zpty -w -n fixture $'\e[A'
    await_fixture buffer ': books/' || exit 95
    await_fixture fds '0 0' || exit 95
    await_fixture project '' || exit 95
    zpty -w -n fixture $'\e[B'
    await_fixture buffer '' || exit 96
    zpty -w -n fixture ': books/'
    await_fixture labels 'sub/' contains || exit 97
    # `run` 等を要する CLI は名前だけならサブコマンドを、`run` まで入れるとスクリプトを出す
    zpty -w -n fixture $'\x15uv'
    await_fixture labels $'run   Run a command or script\nself  Manage the uv executable\nsync  Update the project\'s environment' || exit 98
    await_fixture project $'uv run \nuv self \nuv sync ' || exit 98
    await_fixture display $' run \nCommands (3):\n  run   Run a command or script\n  self  Manage the uv executable\n  sync  Update the project\'s environment' || exit 98
    zpty -w -n fixture $'\x15uv s'
    await_fixture project $'uv self \nuv sync ' || exit 99
    zpty -w -n fixture $'\e[B\e[B\r'
    await_fixture buffer 'uv sync ' || exit 99
    zpty -w -n fixture $'\x15uv --'
    await_fixture project 'uv --quiet ' || exit 101
    zpty -w -n fixture $'\x15uv run'
    await_fixture project 'uv run hello' || exit 102
    # メニューの Ctrl+C は元の入力に戻し、入力を変えるまで一覧を閉じる (Esc は一覧を残す)
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15uv\e[B'
    await_selected 'run' || exit 104
    zpty -w -n fixture $'\x03'
    await_fixture buffer 'uv' || exit 104
    await_fixture fds '0 0' || exit 104
    await_fixture project '' || exit 104
    zpty -w -n fixture ' s'
    await_fixture project $'uv self \nuv sync ' || exit 105
    # 実行したタスクをディレクトリごとに記録し、よく使うタスクを ghost と ↓ の初期選択にする
    for boot in 8 9; do
        zpty -w -n fixture $'\x15make deploy\r'
        repeat $fixture_wait; do
            drain_fixture
            [[ "$(<"$TEST_ROOT/boot")" == $boot ]] && break
            zselect -t 5
        done
        [[ "$(<"$TEST_ROOT/boot")" == $boot ]] || exit 106
    done
    # 記録はバックグラウンドで書かれる
    usage_file="$TEST_ROOT/state/zsh-turbo/task-usage.json"
    repeat $fixture_wait; do
        [[ -s "$usage_file" && "$(<"$usage_file")" == *'"task":"deploy"'* ]] && break
        zselect -t 5
    done
    [[ -s "$usage_file" && "$(<"$usage_file")" == *'"task":"deploy"'* ]] || exit 107
    zpty -w -n fixture 'make'
    await_fixture ghost ' deploy' || exit 108
    await_fixture display $' deploy\nTasks (4):\n  build\n  check\n  clean\n  deploy' || exit 108
    await_fixture highlight 'fg=cyan,bold' contains || exit 108
    : > "$TEST_ROOT/terminal"
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\e[B'
    await_selected 'deploy' || exit 109
    zpty -w -n fixture $'\e[A'
    await_selected 'clean' || exit 109
    zpty -w -n fixture $'\r'
    await_fixture buffer 'make clean' || exit 110
    # メニューの中で文字を打つと、区切りの空白を補って入力欄を編集し、一覧を絞り込む
    menu_deploy=$'make\n\nTasks (4/4):\n  build\n  check\n  clean\n> deploy'
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15make\e[B'
    await_menu "$menu_deploy" || exit 111
    zpty -w -n fixture 'c'
    await_menu $'make c\n\nTasks (1/2):\n> check\n  clean' || exit 112
    zpty -w -n fixture 'l'
    await_menu $'make cl\n\nTasks (1/1):\n> clean' || exit 113
    zpty -w -n fixture $'\r'
    await_fixture buffer 'make clean' || exit 114
    # Backspace で戻すと一覧も戻り、選んでいた項目を選び続ける。Esc は打った文字を残して閉じる
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15make\e[B'
    await_menu "$menu_deploy" || exit 115
    zpty -w -n fixture 'cl'
    await_menu $'make cl\n\nTasks (1/1):\n> clean' || exit 115
    zpty -w -n fixture $'\x7f\x7f'
    await_menu $'make \n\nTasks (3/4):\n  build\n  check\n> clean\n  deploy' || exit 116
    zpty -w -n fixture $'\e'
    await_fixture buffer 'make ' || exit 116
    # 応答を待たずに続けて打っても文字を失わず、Tab は新しい一覧が届いてから選ぶ
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15make\e[B'
    await_menu "$menu_deploy" || exit 117
    zpty -w -n fixture $'cl\t'
    await_menu closed || exit 117
    await_fixture buffer 'make clean' || exit 117
    # 貼り付けも同じく絞り込む
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15make\e[B'
    await_menu "$menu_deploy" || exit 118
    zpty -w -n fixture $'\e[200~ch\e[201~'
    await_menu $'make ch\n\nTasks (1/1):\n> check' || exit 118
    zpty -w -n fixture $'\r'
    await_fixture buffer 'make check' || exit 118
    # 候補が無くなればメニューを閉じ、打った文字 (日本語を含む) は残す
    : > "$TEST_ROOT/menu"
    zpty -w -n fixture $'\x15make\e[B'
    await_menu "$menu_deploy" || exit 119
    zpty -w -n fixture 'zあ'
    await_menu closed || exit 119
    await_fixture buffer 'make zあ' || exit 119
    await_fixture display '' || exit 119
    zpty -w -n fixture $'\x15exit\r'
    print 'ZLE OK'
} always {
    zpty -d fixture 2>/dev/null
}
