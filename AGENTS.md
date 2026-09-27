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
