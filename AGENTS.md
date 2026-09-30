# zsh-turbo 作業ガイド

## 概要

- Rust 製の zsh 拡張 CLI。主な実装は `src/` 配下にある。
- `CLAUDE.md` は `AGENTS.md` へのシンボリックリンクとして管理する。

## 変更時の基本方針

- コメントを追加・更新する場合は、必要最小限かつ日本語で記述する。
- コード解析が必要なときは `astro-sight` を優先して使う。
- プロンプト描画中の外部コマンドは短時間で返る前提にし、カスタムセグメントは stdout のみを表示する。タイムアウト時はシェル本体だけでなく同じプロセスグループも終了し、パイプラインや子プロセスを残さないこと。
- public repository として運用し、GitHub Actions で CI と Homebrew 向けリリースを行う。
- セグメント名一覧は `prompt::segments::ALL_SEGMENT_NAMES` を単一ソースとし、`make_segment` の match アームと同期させる。TUI など他所に別の一覧を複製しないこと。
- `zsh-turbo configure` の設定画面は `tui.rs`（ratatui）。旧 crossterm 直書きウィザード（wizard.rs）は削除済みで、first-run ウィザードを作る場合は現行の `Config`/TUI/`font_install` の責務に合わせて新規設計する。

## 安全に扱うべき入力境界

- プロンプト出力 (`style::colored*`) は `text` 中の制御文字を U+FFFD に置換し、`%` を `%%` にエスケープして zsh の prompt expansion と端末制御シーケンス注入を防ぐ。`plain_colored` も制御文字を置換するが `%` は維持する。新しい色付き出力ヘルパーにも同じ境界処理を適用する。
- `zsh-turbo init` が出力する変数代入は値を single-quote で囲むこと。`shell_single_quote` ヘルパーを使い、設定ファイル経由のコマンド注入を防ぐ。
- `XDG_CONFIG_HOME` は未設定または空文字列なら無効として扱い、`~/.config/zsh-turbo` にフォールバックすること。空文字列を相対パスとして採用しない。
- オートサジェストのハイライト設定は zsh の `region_highlight` 形式（例: `fg=8`）で扱う。Rust 側の色名（例: `bright_black`）をそのまま渡さないこと。
- zsh から渡る `$BUFFER` やエイリアス/関数名は `-` 始まりも有り得るため、それらを受ける CLI 引数には `#[arg(allow_hyphen_values = true)]` を付ける。positional の `buffer`/`prefix` だけでなく、`highlight --aliases`/`--functions`、`suggest --strategy`、`suggest`/`complete --history-file` などのオプションも対象（`-` 始まりの値で clap が exit 2 になり、ハイライト等が無言で停止する）。
- `git status -b` のヘッダーは `## No commits yet on <branch>` (unborn) や `## HEAD (no branch)` (detached) のような特殊形式を返す。`parse_git_branch` で網羅する。
- zsh 拡張履歴 `: <ts>:<dur>;<cmd>` の判定は timestamp/duration が数字であることを必ず検証する。`: echo a;cmd` のような通常履歴を誤って分割しないこと。
- zsh histfile は非 ASCII バイトを metafy（`0x83` + `byte^0x20`）して保存するため、読み込み時は必ず `unmetafy` してから UTF-8 変換する。復元しないと日本語履歴の候補が文字化けし、生 UTF-8 のクエリと一致しない。テストで履歴ファイルを作る際も metafied 形式で書くこと（Meta ペアの 2 バイト目は 0x80 以上のため改行と衝突せず、行分割の前後どちらで unmetafy しても安全）。
- zsh histfile の複数行コマンドは内部改行が `\`+LF で書かれる。`\` 終端（ただし `\\` 終端は除く、zsh readhistline と同規則）の行とその継続行は 1 行プロトコルで運べないため候補から除外する（`history_candidates_rev`）。実際に `\` で終わる単一行コマンドは `\ `+LF で書かれるため誤検出しない。
- 履歴の末尾チャンクを読むときは境界直前のバックスラッシュも確認し、境界をまたぐ複数行コマンドの残りを候補にしない。境界で候補を取り逃した場合は全ファイル検索で補う。
- サジェストはファイル全体で前方一致 → 部分一致 → 曖昧一致の順に選ぶ。末尾チャンクの部分一致を、古い前方一致より先に返さない。
- 履歴読み込みでは改行終端でない最終行（zsh が追記中の torn read）を捨てる（`drop_partial_tail`）。完全なファイルでは no-op。
- `--history-file` の空文字列指定・空の `HISTFILE` 環境変数は未設定として扱い、`~/.zsh_history` へフォールバックする（`XDG_CONFIG_HOME` と同じ規約）。
- gcloud セグメントは、空でない `CLOUDSDK_CONFIG` があればその配下、未指定または空文字列なら `~/.config/gcloud` 配下の `active_config` と `configurations/config_<name>` を読む。ファイルベースのため `gcloud` バイナリ呼び出しなしでプロジェクト ID を解決できる。`parse_gcloud_project` は INI ライク形式の `[core]` セクション内 `project = <id>` のみを採用する。
- Terraform セグメントは Terraform/OpenTofu の両方を扱う。プロジェクト判定では `*.tf`, `*.tf.json`, `*.tofu`, `*.tofu.json`, `.terraform` を対象にし、ワークスペース取得は `terraform` がなければ `tofu` を試す。
- シンタックスハイライトの `$PATH` キャッシュは実行可能ファイルだけをコマンド候補にする。判定は symlink を辿る `fs::metadata` で行うこと（`DirEntry::metadata()` は lstat 相当のため、Homebrew 等の symlink されたコマンドがほぼ全て未知コマンド扱いになる）。壊れた symlink は除外のまま。コマンド位置の `FOO=bar` 形式は代入語として扱い、その後の単語を引き続きコマンド位置として解析する。
- `$PATH` の空要素は zsh と同じくカレントディレクトリとして走査する。空文字列をそのまま `read_dir` に渡すと、そのディレクトリ内の実行可能ファイルが未知コマンド扱いになる。
- シンタックスハイライトの単語走査はクォートを shell の語結合と同様に単語の一部として消費する（`FOO='a b' cmd` の cmd がコマンド位置を維持、`ec'ho'` は 1 語として分類）。分類はクォートを剥いだ論理語で行い、クォートセグメントには文字列色スパンを単語スパンの後に重ねる。引数位置のオプション/パス/glob 判定はクォート外の文字のみで行う。
- シンタックスハイライトのスパンは zsh の `region_highlight`・`CURSOR` と同じ**文字（コードポイント）単位**。パイプライン全体（Rust の `Vec<char>` ベース走査 → init.zsh の `${#BUFFER}` 比較）が文字単位で整合しており、バイト単位への「修正」はマルチバイト入力で全スパンをズラす本物のバグになる。
- シンタックスハイライトの CLOBBER リダイレクト `>|`/`>>|`/`N>|` の `|` は演算子の一部として 1 トークンで消費する。パイプ扱いすると `after_pipe_or_semi` が立ち、リダイレクト先ファイル名がコマンド分類される。
- プレフィックスコマンド（sudo/env 等）直後の `-` 始まりの語はオプション（シアン）として扱い、コマンド位置を消費する。値付きオプションを区別できないため後続をコマンド分類しない（誤検出ゼロ優先）。この救済はプレフィックス直後限定で、`;` 直後の `-foo` は引き続き未知コマンド（赤）。
- `if`/`then`/`else`/`elif`/`do`/`while`/`until`/`time`/`coproc`/`!`/`{` は後続がコマンドの予約語のため `keeps_command_position` でコマンド位置を維持する。`for`/`case`/`select`/`in` 等は後続が非コマンドなので含めない。
- BUILTINS には `.`/`:`/`[` を、RESERVED_WORDS には `!`/`{`/`}`/`[[` を含める。`\cmd`（エイリアスバイパス）は `\` を剥いだ語で builtin/関数/PATH を検索するが、予約語・エイリアスには一致させない（zsh の意味論どおり）。
- シンタックスハイライトのバックスラッシュは、語中では次の文字を論理語へ取り込みつつ `\` 自体を除き、エスケープ対象を option/path/glob 判定から除外する。語頭の `\cmd` だけはエイリアスバイパス判定のため論理語に `\` を残す。
- `classify_command` のパス実在判定は `path_exists`（チルダ展開込み）を使う。`Path::new(word).exists()` では `~/bin/x` が赤になる。
- シンタックスハイライトでは改行をコマンド区切りとして扱い、次の単語をコマンド位置として解析する。複数行バッファの 2 行目以降を通常引数扱いしないこと。
- シンタックスハイライトでは `$?`, `$$`, `$!`, `$#`, `$-`, `$*`, `$@` を zsh 特殊パラメータとして 1 つの変数スパンにする。`$?` の `?` を glob として分離しないこと。
- シンタックスハイライトで `;`/`|`/改行の直後に全体がクォートの語（`'ls'` 等）や変数展開が来た場合、それ自体がコマンド名として消費される。`is_command_position` と併せて `after_pipe_or_semi` も落とし、後続の単語（引数）をコマンド位置として誤分類しないこと（リダイレクトは例外で、後続単語が引き続きコマンド）。
- シンタックスハイライトのリダイレクト演算子は `>`/`>>`/`<`/`2>`/`&>` に加え、`2>&1`/`0<&3` のような fd 複製（`N>&`/`N<&`）も 1 つの演算子トークンとして扱う。数字始まりの fd 分岐と bare な `>&`/`<&` 分岐の挙動を揃え、複製先 fd を未知コマンド（赤）に誤分類しないこと。
- シンタックスハイライトでリダイレクト演算子（`>`/`>>`/`<`/`2>`/`&>`/`N>&` 等）に入った時は `is_command_position=false` だけでなく `after_pipe_or_semi=false` も落とす。`; > file rest` のように `;`/`|` 直後にリダイレクトが続くケースで `after_pipe_or_semi` が残ると、リダイレクト先ファイル名や後続単語が `classify_command` に渡されコマンドとして誤分類される。
- `save_config` は TOML シリアライズ失敗時にエラーを呼び出し元へ返す。`unwrap_or_default()` で空文字列を書き込み、既存の `config.toml` を破壊しないこと。設定ファイル更新は同じディレクトリに一意な一時ファイルを `create_new(true)` で排他的に作成し、書き込みと `sync_all` の後に `rename(2)` でアトミックに置換する。固定名の一時ファイルは並行保存同士で競合するため使わない。アトミック動作のテスト容易性のため保存先を引数化した `save_config_to` を内部関数として用意し、`save_config` はその薄いラッパとする。
- フォントインストール先の書き込み確認とダウンロード途中の保存には、一意な名前を `create_new(true)` で排他的に作成する。固定名を開いたり削除したりして、既存ファイルや別プロセスのダウンロード途中のファイルを破壊しないこと。作成したプローブと失敗したダウンロードの一時ファイルは必ず削除する。
- zsh の非同期 `zle -F` コールバックへ渡す process substitution は `read` と合わせて改行終端にする。NUL 終端にすると候補やハイライト行に NUL が混ざる、または最終行が処理されない。
- zsh の `$'...'` 内で Unicode を書く場合は `\u276f` のような zsh 対応形式を使う。Rust 形式の `\u{276f}` は使わないこと。
- OSC 133 の semantic prompt marker は `_zsh_turbo_apply_semantic_prompt_markers` だけで付与する。PROMPT/RPROMPT は重複付与を防ぎ、transient prompt で PROMPT を差し替えた後も同 helper を再適用すること。`PS2` の `k=s` marker は iTerm2 3.7 以上に限定し、`ITERM2_SQUELCH_PS2_MARK` を尊重する。
- `region_highlight` を更新する処理は、構文ハイライト範囲とサジェスト範囲を分けて扱う。サジェスト更新時に構文ハイライト全体を上書きしないこと。
- サジェストを受け入れる、またはクリアする処理では、`BUFFER` を伸ばす前に古いサジェスト範囲を `region_highlight` から除去すること。先に `BUFFER` を更新すると、旧サジェスト範囲の開始位置が通常の構文ハイライトとして残る。
- ghost 表示とサジェスト受理は `CURSOR == ${#BUFFER}` のときだけ許可する。`zle-line-pre-redraw` は BUFFER と CURSOR の両方を追跡し、BUFFER が同じでもカーソル移動時は `_zsh_turbo_autosuggest_display` を呼んで ghost 表示を消去・復元する。
- `parse_pmset_battery` は `%` を含まない異常出力では `id=...` 内の数字を percent と誤認しないよう、必ず `%` 存在チェックを行う。状態判定は `charged`/`finishing charge` → `discharging` → `not charging`（最適化充電による AC 接続・充電停止 = Full 扱い）→ else Charging の順。`discharging`/`not charging` は `charging` を部分文字列に含むため判定順を崩さないこと。
- Linux のバッテリー取得は `/sys/class/power_supply/BAT*` を順に確認し、`capacity` または `status` が欠けたデバイスはスキップして次の有効なバッテリーを探す。最初の不完全なデバイスだけで全体を `None` にしないこと。
- `parse_df_used_pct` は固定列位置ではなく `%` 終端のフィールドを探す。macOS の autofs は Filesystem 名が `map auto_home`（空白入り）のため、`split_whitespace` の固定 index では列がずれる。
- テストでプロセス共通のカレントディレクトリを変更する場合は `CWD_LOCK`（`segments.rs` の tests 内 static Mutex）を必ず取得する。`cargo test` はスレッド並列のため、無同期の `env::set_current_dir` はテスト間で競合しフレークする。
- `load_config` は読み込み元を引数化した `load_config_from` の薄いラッパとする。テストは実環境の `~/.config/zsh-turbo/config.toml` に依存せず一時ディレクトリのパスで検証する（実 config が存在する開発マシンで `cargo test` が壊れるため）。
- テストで対話 zsh にコマンドを実行させる場合（`tests/zle.rs` など）は `XDG_STATE_HOME` を一時ディレクトリへ向ける。向けないと実環境の `~/.local/state/zsh-turbo/task-usage.json` に記録が書かれる。
- TUI の編集モードは `KeyModifiers::CONTROL`/`ALT` 付きの `KeyCode::Char` を挿入しない。crossterm は raw mode で Ctrl+C を `Char('c')+CONTROL` として届けるため、ガードがないと制御キーが設定値に混入して保存される。SHIFT は大文字入力のため許可する。
- TUI の数値設定やパス省略数のように `usize` を加算する処理は、設定ファイルから最大値が渡ってもデバッグビルドで panic しないよう飽和演算を使う。
- doctor の kubectl チェックに `--short` を付けない（kubectl 1.28 で削除済み）。`ZSH_VERSION` は zsh が export しないシェル変数のため `check_env` では恒久的に検出不能 — `zsh --version` のツールチェックを使う。
- icons の `unicode` 階層は「Nerd Font 不要」が契約のため私用領域（PUA, U+E000..U+F8FF）のグリフを使わない。`os_icon` は全階層で OS 分岐（macOS/Linux）させる。
- `parse_vm_stat_used_pct` で `pages purgeable` は `pages inactive` のサブセットになりうるため、空き容量に加算しない（重複加算で使用率が過小評価される）。
- `vi_mode` セグメントは zsh 側 `zle-keymap-select` フックで `ZSH_TURBO_VI_MODE` を `insert`/`normal`/`visual` に設定する想定。Rust 側は環境変数として読むため **`typeset -gx` で export 必須**（`typeset -g` では子プロセスに渡らずセグメントが一切表示されない）。PROMPT は precmd 時点の静的文字列のため、keymap-select ではモードが変わった時に `_zsh_turbo_render_prompt`（precmd がキャッシュした last_status/duration/jobs を再利用）でプロンプトを再生成してから `zle .reset-prompt` する。zsh の生 KEYMAP 値（`main`/`viins`/`vicmd`/`visual`/`vivis`/`vivli` 等）も解釈する。
- `cpu_arch` セグメントは Linux で `/proc/sys/kernel/arch` を直接読み、利用不可なら `uname -m` にフォールバックする。判定不能な場合は表示しない。
- `root_indicator` セグメントは `USER` 環境変数が設定されている場合、その値が `root` のときだけ表示する。`USER` 未設定時のみ uid 0 のフォールバック判定を使い、`is_root_user_with` はユーザー名とフォールバックを差し替え可能にしてテストに用いる。
- `dir_writable` セグメントは Unix で `access(2)` を直接呼び、書き込み不可ディレクトリでのみ警告を出す。`std::fs::Permissions` では sticky/ACL を考慮できないため `access` を使う。
- `ip` セグメントは `ZSH_TURBO_IP_INTERFACE` が指定されればそのインターフェース、未指定時は macOS で `en0`、Linux で `scope global` の最初の IPv4 を採用する。
- `shell/init.zsh` で `${VAR#PATTERN}` や `${VAR%PATTERN}` の右辺に `$BUFFER` を渡す際は必ず `"$BUFFER"` のように quote する。zsh はパラメータ展開の PATTERN 部分を glob パターンとして解釈するため、quote を忘れると `[`/`*`/`?` を含む入力でパターンエラーや想定外のマッチを引き起こす。
- `shell/init.zsh` の各関数（ZLE ウィジェット・フック）と `compinit` 呼び出しは先頭で `emulate -L zsh` を実行し、ユーザのシェルオプション（`SH_GLOB`/`KSH_ARRAYS`/`SH_WORD_SPLIT` 等）から隔離すること。これらを設定したユーザのもとでは `<->` 数値 glob や `(k)`/`(Ie)` などの zsh 固有展開・添字が壊れる。新しい関数を追加する際も必ず `emulate -L zsh` を付ける（`main.rs` のテストで全 `_zsh_turbo_*` 関数を検査）。唯一の例外は precmd の `local last_status=$?` — `emulate` 自身が `$?` を 0 に潰すため、終了ステータスの捕捉だけは emulate より前に置く（テストもこの 1 行のみ許容）。
- `shell/init.zsh` は関数本体も source 時のシェルオプションで構文解析されるため、`SH_GLOB` で無効になる裸の拡張 glob を関数内に書かず `case` などを使う。トップレベルで未設定の可能性がある変数は `${VAR:-}` で参照し、`NO_UNSET` 下でも初期化できるようにする。
- init.zsh のコマンド実行時間は `local -i duration_ms` への算術代入 `(( duration_ms = (EPOCHREALTIME - start) * 1000 ))` で切り捨てる。float の文字列パース（`${elapsed%.*}` 等）は 1e-4 秒未満で指数表記（`3.69e-05`）になり、瞬時に終わる builtin に「3.7s」等の誤表示を生む。
- init.zsh の aliases/functions 一覧は **非クオート代入の `${(F)${(k)aliases}}`** で実改行 join する。double quote 付きのネスト展開は zsh の展開規則（quoted joining が先行）で空白 join になり、`(j:\n:)` の `\n` はリテラル 2 文字のため、Rust 側 `split('\n')` と一致せず全エイリアス・関数が赤表示になる。
- init.zsh から `suggest`/`highlight` を呼ぶ際は positional の前に必ず `--` を置く。`allow_hyphen_values` は未登録の `-la` 等しか救済せず、BUFFER が `-h`/`--help` と完全一致すると clap の help が stdout へ漏れて候補・ハイライトに混入する。suggest には `--history-file "$HISTFILE"` も明示する（HISTFILE は export されないシェル変数のため子プロセスから見えない）。
- BUFFER 変更の検知は `zle-line-pre-redraw` フック（`_ZSH_TURBO_LAST_BUFFER` との比較ガード付き）で一元化する。個別ウィジェットのラップ（self-insert 等）は履歴検索・ペースト・補完・undo による変更を拾えず、古い ghost・構文スパンが残る。非同期コールバックの `zle -R` でも同フックが発火するため、ガード変数は `_zsh_turbo_after_modify` 呼び出しの**前に**更新して再帰を防ぐ。
- highlight の非同期コールバックはリクエスト時の BUFFER（`_ZSH_TURBO_HIGHLIGHT_BUFFER`）と照合し、変わっていたら古いスパンを適用しない（suggest 側の `_ZSH_TURBO_ASYNC_BUFFER` ガードと対）。
- ghost 表示（POSTDISPLAY）は候補が BUFFER の prefix 延長の場合のみ行う。substring/fuzzy 戦略は BUFFER で始まらない候補を返すことがあり、そのまま `${s#"$BUFFER"}` すると候補全文が POSTDISPLAY に入って表示・受け入れが壊れる。
- 入力欄の下の一覧（タスク・サブコマンド・ファイル）は隠しフラグ `suggest --ui-list` の行プロトコルで受け取る。1 行目 `v1<TAB>種類<TAB>件数<TAB>complete|partial<TAB>入力中の表示行数<TAB>見出し`、続けて `ghost<TAB>BUFFER`・`input<TAB>BUFFER`・`select<TAB>N`・`item<TAB>表示名<TAB>採用後のBUFFER`、最後に `end`。`input` はメニューで打った文字を足す位置までの入力（`make` なら区切りを補った `make `、`npm run` なら `npm run `、ファイル一覧なら BUFFER そのもの）で、Rust が決める。zsh は新しい応答ごとに一度だけ、空白以外で始まる入力（打った文字・貼り付け）の前に BUFFER をこれに置き換えてから挿入する。Backspace には使わない。`select` の N は送った item の 1 始まりの番号で、↓ でメニューを開いたときの初期選択。zsh は応答ごとに 1 に戻し、無効な値・範囲外は無視する。行の種類は追加で拡張し（1 行目の `v1` は変えない）、zsh は知らない種類の行を無視する。表示名と採用後の BUFFER は Rust が決め、zsh はそのまま並べて置き換えるだけにする（空白を含む名前があるため「最後の空白以降を表示名にする」ような zsh 側の加工をしない）。値に制御文字を含む行は送らない。`select` の番号は送った item で数えるため、送れない項目は番号を決める前に除く（`sendable_items`）。`suggest --project-list` は旧シェル連携との互換用、`complete --project-only` は公開 CLI として出力形式を変えない。
- タスク一覧の初期選択と ghost は Rust の `task_usage::preferred`（そのディレクトリの減衰付き利用回数 + 3 × 履歴全体での呼び出しの割合）で決め、ghost はその項目の BUFFER にする（Tab と ↓→Enter の結果をそろえる）。タスクは `project_tasks::LIST_LIMIT`（256 件）まで送り、`max_suggestions` は入力中の表示行数にだけ使う（件数で切ると後ろのタスクを ↓ のメニューで選べない）。commands・files の一覧は従来どおり履歴の続きを ghost にし、ghost が語や階層の区切りまで一致して経由する項目（`passes_through`）を初期選択にする。タスク一覧は ghost の項目（使用記録が無ければ先頭）を常に `select` で送る。
- zsh は `select` を受けた項目（ghost の項目）を、入力中の一覧で表示名だけ ↓ のメニューの選択と同じ色（`_ZSH_TURBO_LIST_SELECTED_STYLE`、シアン太字）で示す（`_ZSH_TURBO_LIST_MARKED`）。メニューに入ると行頭に `> ` が付く。選択の色は入力中の一覧とメニューでこの 1 つの値を使い、別々に書かない（反転表示 `standout` は多くの端末で白背景になり、メニューの選択と見た目がそろわない）。行頭に記号を足す印は使わない（利用者が色での強調を求めたため。記号にするなら `·` や `›` は East Asian Width が曖昧で、CJK 環境の端末が 2 セルで描くと行頭がずれる点に注意）。`select` が無い応答（旧版の CLI、ghost がどの項目も経由しないとき）では強調しない。
- 入力中の一覧の窓は `_zsh_turbo_list_window` で決める。行数は `max_suggestions` と端末の空き（`…` の行を含む）に収め、強調する項目が窓の外に出るときはメニューと同じく最下行に来るまでずらし、隠れた側に `  …` を出す。先頭の番号を `_ZSH_TURBO_LIST_FIRST` に残し、↓ で開くメニューはそこから表示する（窓が跳ねない）。メニューの窓は下に空きを残さないよう詰め、取り直しで項目が減っても位置を保つ。
- 応答は zsh の `read` で 1 行ずつ読まない。pipe からは 1 バイトずつ読むため 100KB 規模で 100ms を超える。`sysread` でまとめて読み、終端行 `end` で止める（子プロセスの終了待ちに依存しない）。
- ファイル一覧の判断はすべて Rust（`path_candidates`・`ui_list`）で行う: 最後の語の字句解析（クォート状態・`\` エスケープ・`~`/export 済み変数の展開）、NFC 正規化＋小文字化での前方一致（macOS のファイル名は NFD が多く IME 入力は NFC のため、正規化しないと日本語名に一致しない）、並び順、ディスク上の名前のままの挿入とクォート状態に応じたエスケープ、表示名の端末幅への切り詰め（`--columns`）、見出しの多言語化。zsh 側に判定ロジックを複製しない。
- 履歴移動（↑/↓・Ctrl+R 等）で呼び出した行ではファイル一覧を出さない。出すと次の ↓ が一覧に取られて履歴を戻れなくなる。zsh は `--last-widget "$LASTWIDGET"` を渡し、判定は Rust の `is_history_motion` が行う。↓ の widget（`_zsh_turbo_list_menu_or_history`）自身が履歴検索へ回したときは、実際に動いた widget 名（`_ZSH_TURBO_DOWN_WIDGET`）へ置き換えて渡す。タスク一覧は従来どおり履歴で呼び出した行でも表示し、↓ でメニューに入る。
- ↓ で一覧を開くとき、結果が未着なら同期で CLI を起動せず、実行中の非同期取得を `zselect` で最大 1 秒待つ（連続入力で再描画が省かれ取得が始まっていなければ、その場で開始して待つ）。
- 一覧の表示行数を `LINES - BUFFERLINES` で決めない。`BUFFERLINES` は直前に描いた POSTDISPLAY（一覧）の行も数えるため、描くたびに行数が交互に増減し、一覧の末尾が出たり消えたりする。`zle-line-pre-redraw` の最後に、これから描く POSTDISPLAY の改行数を `_ZSH_TURBO_DRAWN_LINES` に記録し、`_zsh_turbo_edit_lines`（`BUFFERLINES - _ZSH_TURBO_DRAWN_LINES`）で入力欄だけの行数を求める。選択メニューの行数は開いた時点で一度だけ決めて固定する。
- 選択メニューの Esc は選択だけを取り消してメニューを閉じ、打った文字と一覧は残す。Ctrl+C は打った文字を残したうえで、入力を変えるまで一覧を閉じる（`_ZSH_TURBO_LIST_DISMISSED`）。どちらも BUFFER を開いた時点の値へ戻さない（メニューで何も打っていなければ結果は以前と同じ）。Ctrl+C はキー（`_zsh_turbo_list_cancel` に割り当て）と SIGINT（INT の trap）の両方で届き得るので、どちらも同じ扱いにする。
- ↓ のメニューでは打った文字・貼り付け・Backspace で入力欄を編集し、メニューを開いたまま一覧を取り直す（`_zsh_turbo_list_menu_edit`）。取り直しは `suggest --ui-list --menu --selected <選んでいた値>` で、メニューを開いた直後にも一度行う（届くまでは入力中の一覧を出す）。`--menu` では入力済みの名前と完全一致する項目も含め（tasks: `project_tasks::task_list` の include_exact、commands: `command_items` の include_exact、files: `path_candidates::list` の include_exact）、選択は Rust の `ui_list::selection` が「完全一致 > `--selected` > 通常の推奨」で決める。完全一致には、入れると末尾に空白（サブコマンド・ファイル）か `/`（ディレクトリ）が付くだけの項目も含める（`alpha/` と `alpha.txt` があるときにメニューで `alpha` まで打つと、直前に `alpha.txt` を選んでいても `alpha/` を選ぶ）。zsh は要求の後に ↑↓ で選び直していた場合だけ、その項目を保つ。
- `zle .read-command` の最中は `zle -F` のハンドラが呼ばれない（実機で確認）。そのためメニューのループは、取り直し中は `zselect -t 2 -r fd`（20ms）で応答を待ちつつ、その合間に `$PENDING + $KEYS_QUEUED_COUNT` でキー入力の到着を見て、キーが先なら read-command へ進み、応答が先なら一覧を差し替える（`_zsh_turbo_list_menu_wait`）。期限は 1 秒。編集のたびに古い fd を閉じて取り直すので、古い応答は反映されない。
- メニューの Enter は、取り直し中なら期限まで応答を待って（`_zsh_turbo_list_menu_settle`）新しい一覧から選ぶ。打った文字に一覧が追いついていない（stale）ときに応答が空・期限切れなら、選ばずに打った文字を残して閉じる。メニューを開いた直後の取り直し（まだ何も打っていない）が期限切れ・空（`--menu` を知らない旧版の CLI など）なら、同じ入力に対する入力中の一覧（表示中の選択）をそのまま採用する。待っている間に Ctrl+C（SIGINT）が来たら取り消しとして扱い（打った文字を残し、入力を変えるまで一覧を閉じる）、trap が積んだ Esc は読み捨てる。
- メニューへの貼り付けは `zle .bracketed-paste 変数名` で受け取ってから挿入する（区切りを補うかを先に決めるため）。多バイト文字は `zle .self-insert` に任せる（残りのバイトも ZLE が読む）。メニューで編集した後は構文色を外し、閉じた後に取り直す。
- 実 ZLE のドライバ（`tests/zle-driver.zsh`）でメニューの状態を見るには、テスト用 zshrc（`tests/zle-init.zsh`）で `_zsh_turbo_list_menu_show` と `_zsh_turbo_list_menu` を包み、描画のたびに BUFFER と POSTDISPLAY を、閉じたときに `closed` をファイルへ書いて待ち合わせる。メニューのキーマップでは観測用の widget を呼べず、閉じる前に送った観測用のキー（`\e[24~`）はメニューが Esc と文字として読んでしまうため。ZLE は変化した部分だけを書き直すので、端末出力の文字列で一覧の状態を判定しない。
- パスの字句解析では語の区切りを半角空白・タブ・改行に限る（`char::is_whitespace` は使わない）。zsh は全角空白で語を分けず、候補側も全角空白をエスケープしないため、区切ると全角空白入りのディレクトリへ進めなくなる。ダブルクォート内の `\!` はエスケープとして扱う（対話 zsh の BANG_HIST では `"\!"` が `!` になり、候補側も `\!` を出す）。
- `npm`・`pnpm`・`bun`・`yarn`・`uv`・`deno`・`mise` のスクリプト・タスクは `run` 等のサブコマンドの後だけに出す（`project_tasks::run_keywords`）。名前だけ・2 語目の入力中は `--help` を解析したサブコマンド（`-` 始まりならオプション）を一覧にする。`make`・`just`・`task` はターゲットを直接取るので従来どおり。`shell/project_completion.zsh` の Tab 補完も同じ区切りに合わせる。
- サブコマンド一覧の `--help` はキー入力ごとに実行しない。`completion::top_level` が `$XDG_CACHE_HOME/zsh-turbo/subcommands/<cmd>.json` にキャッシュし、実行ファイルのスタンプが同じで 24 時間以内（失敗は 5 分）なら再利用する。取り直しは flock で 1 プロセスに絞り、待っていた側はロック取得後にキャッシュを読み直す（`try_lock` で諦めると、直後の入力の応答が一覧なしになる）。mise のシムは実体が mise なのでスタンプが変わらず、期限で取り直す。help の解析は npm の `All commands:`（カンマ区切り）と yarn v1 の字下げした `Commands:`・`- name` にも対応する。入力しただけで `--help` を実行するため、PATH の `.` や空要素（カレントディレクトリ）で見つかった実行ファイルは使わない（信頼できないリポジトリの `./uv` を起動しない）。`npm --help` は終了コード 1 で使い方を出すため、終了コードは問わず出力を読む（`run_cmd_any_status`）。何も読み取れなかった結果（mise の未信頼の設定のエラー文など）は失敗として短い期限で取り直す。
- テストでファイルロックを取るときは `try_lock` ではなく `lock` を使う。並列テストの別スレッドが fork した直後は、子プロセスが exec するまでロック用 fd を共有して flock が解放されず、`try_lock` が一時的に WouldBlock になる。
- タスクの呼び出しの判定は `project_tasks::invocation` の 1 か所にまとめ、利用記録と履歴の集計で同じ規則を使う。make 以外は、タスク名の後ろも含めて `--` より前に `-` 始まりの語がある行を対象外にする（`npm run build --prefix ../other`・`task build -d ../other` のように、オプションの位置を問わず実行先や定義ファイルが変わり得る）。`--` の後ろはタスクへ渡る引数なので判定に使わない。make は `-C`・`-f`・`--directory`・`--file`・`--makefile` 以外のオプションを許す。記録対象のコマンド名は `project_tasks::COMMANDS` を単一ソースとし、`zsh-turbo init` が `_ZSH_TURBO_TASK_COMMANDS` 配列として zsh へ渡す。ディレクトリ別履歴の記録が無効な場合だけ、zsh は preexec でこの名前で始まる行に `zsh-turbo record` の起動を絞る（タスクの判定は Rust が行う）。
- 実行行は `print -rn -- "$line" | zsh-turbo record &!` のように標準入力で渡す（引数にするとプロセス一覧に出る）。preexec の `$1` は `HIST_IGNORE_SPACE` でも先頭の空白を保持するので、先頭が空白の行は zsh と Rust の両方で記録しない。`record` 側でもタスクとディレクトリ別履歴それぞれの記録設定を確認する（再起動前のシェルから呼ばれ得るため）。`record` は `config::load_config_strict()` で設定を読み、読めなければ記録しない（`load_config()` は失敗時に既定値＝有効を返すため、無効化した記録が再開してしまう）。
- 利用記録の保存先（`task_usage::store_path`）は、`XDG_STATE_HOME` が未設定・空・相対パスなら `~/.local/state` を使う（XDG の仕様どおり相対パスは無効として扱う。`record` はコマンドを実行したディレクトリで動くため、相対パスを採るとプロジェクト内に記録ファイルを作ってしまう）。保存は固定名のロックファイル（`task-usage.lock`、置換も削除もしない）を flock し、読み取り・加算・一時ファイル（`create_new`、0600）への書き込み・rename までを一つの排他区間にする。読めない・別の版の記録ファイルは上書きせず記録をやめる。記録の処理順が前後しても同じ重みになるよう、加算は新しい方の時刻にそろえてから行う。保存上限（1000 組）で枝刈りするとき、今回加えた組は残す（残さないと、上限まで埋まった後は新しいタスクを何度使っても 1 回目として捨てられ続け、学習できない）。
- 候補表示と Ctrl+R はディレクトリ別履歴を使う。`directory_history` は実行時の cwd を canonicalize してハッシュした名前の履歴を state 配下へ保存し、先頭空白・制御文字・複数行・4096 バイト超を除く。各ディレクトリの直近 1000 回、最大 1 MiB を保持し、flock と 0600 の一時ファイルからのアトミック置換で保存する。zsh は `suggest`/`complete` に `--directory-history` を渡し、実行場所のない共通履歴へフォールバックしない。公開 CLI の明示的な `--history-file` は指定ファイルの検索を維持する。`record_directory_history` はタスクの利用記録と独立し、無効時は新規記録だけを止める。
- `ZSH_TURBO_COMPLETION_DIRS` は `:` 区切りの補完関数ディレクトリとして扱い、`compinit` より前に既存ディレクトリだけを `fpath` へ追加する。空要素・存在しないパスは無視し、同じパスを重複追加しないこと。

- CLI 補完の登録は `Config.completions` で管理する。help 解析・更新検知・SHA-256・キャッシュ生成は Rust 側、zsh 側は `shell/completion.zsh` の補完呼び出しと非同期更新の起動だけに限定する。CLI ごとの定義を本体に固定しない。
- 補完キャッシュは CLI ごとのファイルロックを保持し、生成中のバイナリ変更と zsh 構文を検証してからアトミックに公開する。失敗時に旧キャッシュを削除しない。メタデータが同じ間はハッシュを再計算せず、Tab では help や Rust の更新処理を同期実行しない。
- `_arguments` を呼ぶ補完関数は `emulate -L zsh` の後に `setopt extendedglob ${_comp_options[@]}` で補完用オプションを復元する。completion が有効化した EXTENDED_GLOB を emulate が解除するとオプション補完が無言で失敗する。生成定義の再読込時には、その定義が追加した関数も更新する。

## ローカル検証

Rust の開発用バージョンは `mise.toml` で固定する。

```bash
make setup
make ci
make release
make dist
```

## 補足

- 依存更新には `depup --install` を使う。
- 変更がユーザー向け挙動に影響する場合は `README.md` と `README.ja.md` も合わせて更新する。
