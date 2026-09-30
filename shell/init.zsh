#!/usr/bin/env zsh
# zsh-turbo: zsh 統合スクリプト
# 使い方: eval "$(zsh-turbo init)"

# ─── プロンプト連携 ─────────────────────────────────────────────

typeset -g ZSH_TURBO_CMD="${commands[zsh-turbo]:-zsh-turbo}"
typeset -g ZSH_TURBO_TRANSIENT="${ZSH_TURBO_TRANSIENT:-0}"
typeset -g ZSH_TURBO_SUGGEST_STRATEGY="${ZSH_TURBO_SUGGEST_STRATEGY:-prefix}"
typeset -g ZSH_TURBO_SUGGEST_HIGHLIGHT="${ZSH_TURBO_SUGGEST_HIGHLIGHT:-fg=8}"
typeset -g ZSH_TURBO_KEY_TAB="${ZSH_TURBO_KEY_TAB:-full}"
typeset -g ZSH_TURBO_KEY_RIGHT="${ZSH_TURBO_KEY_RIGHT:-step}"
typeset -g ZSH_TURBO_KEY_ALT_F="${ZSH_TURBO_KEY_ALT_F:-word}"
typeset -g ZSH_TURBO_KEY_CTRL_RIGHT="${ZSH_TURBO_KEY_CTRL_RIGHT:-word}"
typeset -g ZSH_TURBO_COMPLETION_DIRS="${ZSH_TURBO_COMPLETION_DIRS:-}"
typeset -g ZSH_TURBO_TERM_SHELL_INTEGRATION="${ZSH_TURBO_TERM_SHELL_INTEGRATION:-auto}"
typeset -g ZSH_TURBO_RECORD_TASK_USAGE="${ZSH_TURBO_RECORD_TASK_USAGE:-1}"
typeset -g ZSH_TURBO_RECORD_DIRECTORY_HISTORY="${ZSH_TURBO_RECORD_DIRECTORY_HISTORY:-1}"

# コマンド実行時間を測定するタイマー

function _zsh_turbo_term_shell_integration_enabled() {
    emulate -L zsh
    [[ "$ZSH_TURBO_TERM_SHELL_INTEGRATION" == "1" ]] && return 0
    [[ "$ZSH_TURBO_TERM_SHELL_INTEGRATION" == "auto" ]] || return 1
    [[ "$TERM_PROGRAM" == "WarpTerminal" ]] || _zsh_turbo_iterm2_supports_prompt_kind
}

function _zsh_turbo_iterm2_supports_prompt_kind() {
    emulate -L zsh
    [[ "$TERM_PROGRAM" == "iTerm.app" ]] || return 1

    # case の選択肢は SH_GLOB が有効な状態で読み込んでも構文エラーにならない。
    case "$TERM_PROGRAM_VERSION" in
        3.<7->*|<4->.*) return 0 ;;
        *) return 1 ;;
    esac
}

function _zsh_turbo_apply_semantic_prompt_markers() {
    emulate -L zsh
    _zsh_turbo_term_shell_integration_enabled || return

    # OSC 133 の prompt marker は iTerm2/Warp のコマンド範囲抽出からプロンプトを除外する。
    if [[ "$PROMPT" != *$'\e]133;A\a'* ]]; then
        PROMPT=$'%{\e]133;A\a%}'$PROMPT$'%{\e]133;B\a%}'
    fi
    if [[ -n "$RPROMPT" && "$RPROMPT" != *$'\e]133;P;k=r\a'* ]]; then
        RPROMPT=$'%{\e]133;P;k=r\a%}'$RPROMPT$'%{\e]133;B\a%}'
    fi

    if _zsh_turbo_iterm2_supports_prompt_kind &&
       [[ -n "$PS2" && "$PS2" != *$'\e]133;A;k=s\a'* && -z ${ITERM2_SQUELCH_PS2_MARK-} ]]; then
        typeset -g ITERM2_PRECMD_PS2="$PS2"
        PS2=$'%{\e]133;A;k=s\a%}'$PS2$'%{\e]133;B\a%}'
    fi
}

function _zsh_turbo_preexec() {
    emulate -L zsh  # ユーザのシェルオプション(SH_GLOB/KSH_ARRAYS 等)から隔離する
    typeset -g _ZSH_TURBO_START_TIME="${EPOCHREALTIME}"
    _zsh_turbo_record_task_usage "$1"
}

# 実行行を Rust に渡し、履歴とタスクの利用をディレクトリごとに記録する。
function _zsh_turbo_record_task_usage() {
    emulate -L zsh
    local line="$1"
    [[ -n "$line" ]] || return 0
    # 先頭が空白の行は、履歴に残さない慣習に合わせて記録しない
    [[ "$line" == [[:space:]]* ]] && return 0
    if [[ "$ZSH_TURBO_RECORD_DIRECTORY_HISTORY" != 1 ]]; then
        [[ "$ZSH_TURBO_RECORD_TASK_USAGE" == 1 ]] || return 0
        (( ${_ZSH_TURBO_TASK_COMMANDS[(Ie)${line%%[[:space:]]*}]} )) || return 0
    fi
    # 実行行はプロセス一覧に出さないよう標準入力で渡す。記録の成否は実行に影響させない
    print -rn -- "$line" 2>/dev/null | "$ZSH_TURBO_CMD" record >/dev/null 2>&1 &!
}

# キャッシュ済みのプロンプト素材からプロンプトを再生成する。
# precmd と keymap-select の両方から呼ぶため素材はグローバルに保持する。
function _zsh_turbo_render_prompt() {
    emulate -L zsh
    local left right
    if { IFS= read -r -d '' left && IFS= read -r -d '' right } < <(
        "$ZSH_TURBO_CMD" prompt --last-status "${_ZSH_TURBO_LAST_STATUS:-0}" --duration-ms "${_ZSH_TURBO_DURATION_MS:-0}" --jobs "${_ZSH_TURBO_JOBS:-0}" --side both 2>/dev/null
    ); then
        PROMPT="$left"
        RPROMPT="$right"
    else
        PROMPT='%~ %# '
        RPROMPT=''
    fi
    # transient で表示を簡略化した後も、先頭の空行を維持する。
    local prompt_body="$PROMPT"
    typeset -g _ZSH_TURBO_PROMPT_PADDING=""
    while [[ "$prompt_body" == $'\n'* ]]; do
        _ZSH_TURBO_PROMPT_PADDING+=$'\n'
        prompt_body="${prompt_body#$'\n'}"
    done
    _zsh_turbo_apply_semantic_prompt_markers
}

function _zsh_turbo_precmd() {
    local last_status=$?  # emulate 自身が $? を 0 に潰すため、必ず emulate より前に捕捉する
    emulate -L zsh
    local -i duration_ms=0

    if [[ -n "$_ZSH_TURBO_START_TIME" ]]; then
        # 整数変数への算術代入で ms へ切り捨てる。文字列パース (${elapsed%.*} 等) は
        # 1e-4 秒未満の指数表記 (`3.69e-05`) を誤算して「3.7s」等と表示してしまう。
        (( duration_ms = (EPOCHREALTIME - _ZSH_TURBO_START_TIME) * 1000 ))
        (( duration_ms < 0 )) && duration_ms=0
        unset _ZSH_TURBO_START_TIME
    fi

    # vi モードセグメント連携: 行頭は insert モードから始まる。
    # Rust 側は環境変数として読むため export (-x) が必須。
    typeset -gx ZSH_TURBO_VI_MODE="${ZSH_TURBO_VI_MODE:-insert}"

    # keymap-select からの再生成でも同じ素材を使えるようにキャッシュする
    typeset -g _ZSH_TURBO_LAST_STATUS=$last_status
    typeset -g _ZSH_TURBO_DURATION_MS=$duration_ms
    typeset -g _ZSH_TURBO_JOBS=${(%):-%j}
    _zsh_turbo_render_prompt
}

# vi モードを `KEYMAP` 変化に追従させ、プロンプトに反映する
function _zsh_turbo_zle_keymap_select() {
    emulate -L zsh
    local previous_mode="$ZSH_TURBO_VI_MODE"
    case "${KEYMAP}" in
        vicmd) typeset -gx ZSH_TURBO_VI_MODE="normal" ;;
        visual|vivis|vivli) typeset -gx ZSH_TURBO_VI_MODE="visual" ;;
        main|viins|*) typeset -gx ZSH_TURBO_VI_MODE="insert" ;;
    esac
    # PROMPT は precmd 時点の静的文字列のため、reset-prompt だけでは
    # インジケータが変わらない。モードが変わった時だけ再生成する。
    if [[ "$ZSH_TURBO_VI_MODE" != "$previous_mode" ]]; then
        _zsh_turbo_render_prompt
        zle .reset-prompt 2>/dev/null
    fi
}
zle -N zle-keymap-select _zsh_turbo_zle_keymap_select

autoload -Uz add-zsh-hook
add-zsh-hook preexec _zsh_turbo_preexec
add-zsh-hook precmd _zsh_turbo_precmd

# サブ秒単位の計測のため EPOCHREALTIME を有効化
zmodload zsh/datetime 2>/dev/null

# ─── Transient プロンプト ───────────────────────────────────────

function _zsh_turbo_zle_line_finish() {
    emulate -L zsh
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_ASYNC_FD"
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_HIGHLIGHT_FD"
    _ZSH_TURBO_ASYNC_FD=0
    _ZSH_TURBO_HIGHLIGHT_FD=0
    _zsh_turbo_clear_suggestion
    zle -R
    _ZSH_TURBO_LAST_BUFFER=""
    _ZSH_TURBO_LAST_CURSOR=-1
    [[ "$ZSH_TURBO_TRANSIENT" != "1" ]] && return

    # コマンド実行前にフルプロンプトを最小表示へ置き換える
    PROMPT="${_ZSH_TURBO_PROMPT_PADDING:-}"$'%F{%(?.green.red)}\u276f%f '
    RPROMPT=""
    _zsh_turbo_apply_semantic_prompt_markers
    zle .reset-prompt 2>/dev/null
}
zle -N zle-line-finish _zsh_turbo_zle_line_finish

# ─── オートサジェスト連携（非同期） ────────────────────────────

typeset -g _ZSH_TURBO_SUGGESTION=""
typeset -g _ZSH_TURBO_GHOST_SUFFIX=""
# 入力欄の下に出す一覧 (tasks / commands / files)。表示名と採用後の BUFFER を同じ添字で持つ。
# 見出し・表示名の切り詰め・並び順は Rust 側 (`suggest --ui-list`) が決める。
typeset -g _ZSH_TURBO_LIST_KIND=""
typeset -g _ZSH_TURBO_LIST_TITLE=""
typeset -ga _ZSH_TURBO_LIST_LABELS=()
typeset -ga _ZSH_TURBO_LIST_VALUES=()
typeset -gi _ZSH_TURBO_LIST_TOTAL=0
typeset -gi _ZSH_TURBO_LIST_PARTIAL=0
typeset -gi _ZSH_TURBO_LIST_ROWS=10
# ↓ でメニューを開いたときに選ぶ項目 (1 始まり)。Rust が ghost と同じ項目を指す
typeset -gi _ZSH_TURBO_LIST_SELECTED=1
# Rust が select で項目を指したか。入力中の一覧でその項目を選択の色で示し、見える位置まで窓をずらす
typeset -gi _ZSH_TURBO_LIST_MARKED=0
# 一覧で選んでいる項目の色。↓ のメニューの選択と、入力中の一覧の ghost の項目で同じものを使う
typeset -g _ZSH_TURBO_LIST_SELECTED_STYLE='fg=cyan,bold'
# 入力中の一覧で先頭に表示した項目の番号。↓ で開くメニューも同じ位置から表示する
typeset -gi _ZSH_TURBO_LIST_FIRST=1
# メニューで打った文字を足す位置までの入力 (`make` なら区切りの空白を補った `make `)。Rust が決める
typeset -g _ZSH_TURBO_LIST_INPUT=""
typeset -gA _ZSH_TURBO_DOWN_FALLBACK
typeset -gi _ZSH_TURBO_LIST_KEYS_ACTIVE=${_ZSH_TURBO_LIST_KEYS_ACTIVE:-0}
# ↓ が一覧を開かず履歴検索へ回したときの widget 名 (開いたときは空)
typeset -g _ZSH_TURBO_DOWN_WIDGET=""
# 直前の再描画で出した POSTDISPLAY の改行数。BUFFERLINES はこの行も数えるため、
# 一覧の行数を決めるときに差し引く (差し引かないと描くたびに行数が増減する)
typeset -gi _ZSH_TURBO_DRAWN_LINES=0
# メニューを Ctrl+C で閉じたときの BUFFER。入力を変えるまで一覧を出さない
typeset -g _ZSH_TURBO_LIST_DISMISSED=""
typeset -gi _ZSH_TURBO_ASYNC_FD=0
typeset -g _ZSH_TURBO_ASYNC_BUFFER=""

# 非同期応答の一括読み込み (sysread) と、↓ で結果を待つ処理 (zselect) に使う
zmodload zsh/system 2>/dev/null
zmodload zsh/zselect 2>/dev/null

function _zsh_turbo_list_down_bindings() {
    emulate -L zsh
    local action="$1" keymap key binding widget fallback
    if [[ "$action" == on ]]; then
        (( _ZSH_TURBO_LIST_KEYS_ACTIVE )) && return 0
    else
        (( _ZSH_TURBO_LIST_KEYS_ACTIVE )) || return 0
    fi
    for keymap in emacs viins; do
        for key in $'\e[B' $'\eOB'; do
            binding="$(bindkey -M "$keymap" "$key")"
            widget="${binding##* }"
            if [[ "$action" == on ]]; then
                [[ "$widget" == _zsh_turbo_list_menu_or_history ]] && continue
                [[ "$widget" == '"'* ]] && continue
                _ZSH_TURBO_DOWN_FALLBACK[$keymap:$key]="$widget"
                bindkey -M "$keymap" "$key" _zsh_turbo_list_menu_or_history
            elif [[ "$widget" == _zsh_turbo_list_menu_or_history ]]; then
                fallback="${_ZSH_TURBO_DOWN_FALLBACK[$keymap:$key]:-undefined-key}"
                if [[ "$fallback" == undefined-key ]]; then
                    bindkey -r -M "$keymap" "$key"
                else
                    bindkey -M "$keymap" "$key" "$fallback"
                fi
            fi
        done
    done
    if [[ "$action" == on ]]; then
        _ZSH_TURBO_LIST_KEYS_ACTIVE=1
    else
        _ZSH_TURBO_LIST_KEYS_ACTIVE=0
    fi
}

function _zsh_turbo_reset_list() {
    emulate -L zsh
    _ZSH_TURBO_LIST_KIND=""
    _ZSH_TURBO_LIST_TITLE=""
    _ZSH_TURBO_LIST_LABELS=()
    _ZSH_TURBO_LIST_VALUES=()
    _ZSH_TURBO_LIST_TOTAL=0
    _ZSH_TURBO_LIST_PARTIAL=0
    _ZSH_TURBO_LIST_SELECTED=1
    _ZSH_TURBO_LIST_MARKED=0
    _ZSH_TURBO_LIST_FIRST=1
    _ZSH_TURBO_LIST_INPUT=""
}

function _zsh_turbo_close_async_fd() {
    emulate -L zsh
    local fd="$1"
    (( fd > 0 )) || return 0
    zle -F "$fd" 2>/dev/null
    # exec 単体へのリダイレクトは呼び出し元の stderr まで変更する。
    { exec {fd}<&- } 2>/dev/null
}

# 非同期描画を直前のキー操作として記録すると、連続する履歴検索が途切れる。
function _zsh_turbo_async_dispatch() {
    emulate -L zsh
    if [[ "$1" == "$_ZSH_TURBO_ASYNC_FD" ]]; then
        zle _zsh_turbo_async_callback -f nolast -- "$@"
    elif [[ "$1" == "$_ZSH_TURBO_HIGHLIGHT_FD" ]]; then
        zle _zsh_turbo_highlight_callback -f nolast -- "$@"
    fi
}

function _zsh_turbo_autosuggest_fetch() {
    emulate -L zsh
    local prefix="$BUFFER"
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_ASYNC_FD"
    _ZSH_TURBO_ASYNC_FD=0
    if [[ -z "$prefix" ]]; then
        _ZSH_TURBO_SUGGESTION=""
        _zsh_turbo_reset_list
        _ZSH_TURBO_GHOST_SUFFIX=""
        POSTDISPLAY=""
        _zsh_turbo_list_down_bindings off
        return
    fi

    # 新しい検索結果が返るまで古い候補を表示しない
    _ZSH_TURBO_SUGGESTION=""
    _zsh_turbo_reset_list
    _ZSH_TURBO_GHOST_SUFFIX=""
    POSTDISPLAY=""

    _ZSH_TURBO_ASYNC_BUFFER="$prefix"

    # 履歴移動で呼び出した行かどうかは Rust 側が widget 名から判定する。
    # ↓ が履歴検索へ回した場合は、実際に動いた widget 名を渡す。
    local last_widget="${LASTWIDGET:-}"
    [[ "$last_widget" == _zsh_turbo_list_menu_or_history ]] && last_widget="$_ZSH_TURBO_DOWN_WIDGET"

    local REPLY
    _zsh_turbo_list_request "$last_widget"
    _ZSH_TURBO_ASYNC_FD=$REPLY
    zle -F "$_ZSH_TURBO_ASYNC_FD" _zsh_turbo_async_dispatch
    _zsh_turbo_list_down_bindings on
}

# 現在の BUFFER の一覧を取る `suggest --ui-list` を起動し、応答を読む fd を REPLY に返す。
# $1 は直前の widget 名、残りは追加の引数。
# - `--` 区切り: BUFFER が `-h` 等の登録済みフラグと一致したときに clap の
#   help が stdout へ漏れ、候補として表示されるのを防ぐ。
# - `--history-file "$HISTFILE"`: HISTFILE はシェル変数で export されないため、
#   明示的に渡さないと子プロセスはカスタム履歴パスを解決できない。
function _zsh_turbo_list_request() {
    emulate -L zsh
    local last_widget="$1" fd
    shift
    exec {fd}< <("$ZSH_TURBO_CMD" suggest --directory-history --ui-list --columns "${COLUMNS:-0}" --last-widget "$last_widget" --strategy "$ZSH_TURBO_SUGGEST_STRATEGY" --history-file "$HISTFILE" "$@" -- "$BUFFER" 2>/dev/null)
    REPLY=$fd
}

# 応答を終端行 `end` か EOF まで読み、1 行ずつ reply に入れる。
# read は pipe から 1 バイトずつ読むため、100KB 規模の一覧で 100ms を超える。
function _zsh_turbo_read_response() {
    emulate -L zsh
    local fd="$1" data="" chunk line
    reply=()
    if (( ${+builtins[sysread]} )); then
        while sysread -i "$fd" -s 65536 chunk 2>/dev/null; do
            data+="$chunk"
            [[ "$data" == end$'\n' || "$data" == *$'\n'end$'\n' ]] && break
        done
        reply=("${(@f)data}")
    else
        while IFS= read -r -u "$fd" line 2>/dev/null; do
            reply+=("$line")
            [[ "$line" == end ]] && break
        done
    fi
}

# `suggest --ui-list` の応答を候補の状態へ反映する。
# 1 行目: v1<TAB>種類<TAB>件数<TAB>complete|partial<TAB>入力中の表示行数<TAB>見出し
# 続く行: ghost<TAB>BUFFER / input<TAB>BUFFER / select<TAB>N / item<TAB>表示名<TAB>BUFFER / end
# input はメニューで打った文字を足す位置までの入力、select は ↓ で開いたときに選ぶ項目の番号
# (item の 1 始まりの順番)。知らない行は無視する
function _zsh_turbo_apply_response() {
    emulate -L zsh
    local line rest ghost="" selected="" input=""
    local -a fields labels values
    fields=("${(@ps:\t:)${1:-}}")
    _zsh_turbo_reset_list
    _ZSH_TURBO_SUGGESTION=""
    [[ "${fields[1]:-}" == v1 ]] || return 0
    shift
    for line in "$@"; do
        case "$line" in
            end) break ;;
            ghost$'\t'*) ghost="${line#ghost$'\t'}" ;;
            input$'\t'*) input="${line#input$'\t'}" ;;
            select$'\t'*) selected="${line#select$'\t'}" ;;
            item$'\t'*$'\t'*)
                rest="${line#item$'\t'}"
                labels+=("${rest%%$'\t'*}")
                values+=("${rest#*$'\t'}")
                ;;
        esac
    done
    _ZSH_TURBO_SUGGESTION="$ghost"
    [[ "${fields[5]:-}" == <-> ]] && _ZSH_TURBO_LIST_ROWS=${fields[5]}
    case "${fields[2]:-}" in
        tasks|commands|files) ;;
        *) return 0 ;;
    esac
    [[ -n "$_ZSH_TURBO_LIST_DISMISSED" && "$BUFFER" == "$_ZSH_TURBO_LIST_DISMISSED" ]] && return 0
    (( ${#values} )) || return 0
    _ZSH_TURBO_LIST_KIND="${fields[2]}"
    _ZSH_TURBO_LIST_TITLE="${fields[6]:-}"
    _ZSH_TURBO_LIST_LABELS=("${labels[@]}")
    _ZSH_TURBO_LIST_VALUES=("${values[@]}")
    _ZSH_TURBO_LIST_INPUT="$input"
    if [[ "$selected" == <-> ]] && (( selected >= 1 && selected <= ${#values} )); then
        _ZSH_TURBO_LIST_SELECTED=$selected
        _ZSH_TURBO_LIST_MARKED=1
    fi
    if [[ "${fields[3]:-}" == <-> ]]; then
        _ZSH_TURBO_LIST_TOTAL=${fields[3]}
    else
        _ZSH_TURBO_LIST_TOTAL=${#values}
    fi
    [[ "${fields[4]:-}" == partial ]] && _ZSH_TURBO_LIST_PARTIAL=1
    return 0
}

function _zsh_turbo_async_callback() {
    emulate -L zsh
    local fd="$1"
    local -a reply
    _zsh_turbo_read_response "$fd"

    # fd を片付ける
    _zsh_turbo_close_async_fd "$fd"
    _ZSH_TURBO_ASYNC_FD=0

    # リクエスト後にバッファが変わっていない場合だけ反映する
    if [[ "$BUFFER" == "$_ZSH_TURBO_ASYNC_BUFFER" ]]; then
        _zsh_turbo_apply_response "${reply[@]}"
        _zsh_turbo_autosuggest_display
        zle -R
    fi
}
zle -N _zsh_turbo_async_callback

# 実行中の非同期取得を最大 1 秒待ち、届いた結果を反映する
function _zsh_turbo_await_async() {
    emulate -L zsh
    local fd="$_ZSH_TURBO_ASYNC_FD"
    (( fd > 0 )) || return 0
    if (( ${+builtins[zselect]} )); then
        zselect -t 100 -r "$fd" 2>/dev/null || return 0
    fi
    _zsh_turbo_async_callback "$fd"
}

# プロンプト・BUFFER・ghost が使う行数を REPLY に返す
function _zsh_turbo_edit_lines() {
    emulate -L zsh
    local -i lines=$(( BUFFERLINES - _ZSH_TURBO_DRAWN_LINES ))
    (( lines > 0 )) || lines=1
    REPLY=$lines
}

# 一覧の見出し。選択中は位置 (例: Files (3/19):) を添える。
function _zsh_turbo_list_header() {
    emulate -L zsh
    local title="$1" total="$2" partial="$3" index="${4:-}" count
    count="$total"
    (( partial )) && count+='+'
    [[ -n "$index" ]] && count="$index/$count"
    REPLY="$title ($count):"
}

function _zsh_turbo_autosuggest_display() {
    emulate -L zsh
    # 構文ハイライトを保持し、古いサジェスト範囲だけを差し替える。
    local -a syntax_hl
    local entry start
    for entry in "${region_highlight[@]}"; do
        start="${entry%% *}"
        if [[ "$start" != <-> ]] || (( start < ${#BUFFER} )); then
            syntax_hl+=("$entry")
        fi
    done
    region_highlight=("${syntax_hl[@]}")
    _ZSH_TURBO_GHOST_SUFFIX=""
    POSTDISPLAY=""

    (( CURSOR == ${#BUFFER} )) || return 0

    # 候補が BUFFER の延長 (prefix 一致) の場合だけ ghost 表示する。
    # substring/fuzzy 戦略は BUFFER で始まらない候補を返すことがあり、そのまま
    # suffix 計算すると候補全文が POSTDISPLAY に入って表示・受け入れが壊れる。
    if [[ -n "$_ZSH_TURBO_SUGGESTION" && "$_ZSH_TURBO_SUGGESTION" != "$BUFFER" \
          && "$_ZSH_TURBO_SUGGESTION" == "$BUFFER"* ]]; then
        # zsh の ${VAR#PATTERN} は右辺をパターンとして解釈するため、
        # BUFFER に `[`/`*`/`?` 等を含むと壊れる。クオートして literal 扱いにする。
        local suffix="${_ZSH_TURBO_SUGGESTION#"$BUFFER"}"
        if [[ -n "$suffix" ]]; then
            _ZSH_TURBO_GHOST_SUFFIX="$suffix"
            POSTDISPLAY="$suffix"
            region_highlight+=("$(( ${#BUFFER} )) $(( ${#BUFFER} + ${#suffix} )) ${ZSH_TURBO_SUGGEST_HIGHLIGHT}")
        fi
    fi

    if (( ${#_ZSH_TURBO_LIST_VALUES} )); then
        _zsh_turbo_edit_lines
        local -i avail=$(( LINES - REPLY - 4 )) i
        if (( avail <= 0 )); then
            _zsh_turbo_list_down_bindings off
            return 0
        fi
        _zsh_turbo_list_down_bindings on
        local -a reply
        _zsh_turbo_list_window "$avail"
        local -i first=${reply[1]} rows=${reply[2]} above=${reply[3]} below=${reply[4]}
        # ↓ で開くメニューも同じ位置から表示し、窓が跳ねないようにする
        _ZSH_TURBO_LIST_FIRST=$first
        local -i label_start
        _zsh_turbo_list_header "$_ZSH_TURBO_LIST_TITLE" "$_ZSH_TURBO_LIST_TOTAL" "$_ZSH_TURBO_LIST_PARTIAL"
        POSTDISPLAY+=$'\n'"$REPLY"
        (( above )) && POSTDISPLAY+=$'\n'"  …"
        for (( i = first; i < first + rows; i++ )); do
            POSTDISPLAY+=$'\n'"  ${_ZSH_TURBO_LIST_LABELS[i]}"
            # ghost の項目 (Tab で入る項目、↓ で最初に選ぶ項目) は、↓ のメニューで選んだ項目と
            # 同じ色で示す。メニューに入ると行頭に `> ` が付く
            if (( _ZSH_TURBO_LIST_MARKED && i == _ZSH_TURBO_LIST_SELECTED )); then
                label_start=$(( ${#BUFFER} + ${#POSTDISPLAY} - ${#_ZSH_TURBO_LIST_LABELS[i]} ))
                region_highlight+=("$label_start $(( label_start + ${#_ZSH_TURBO_LIST_LABELS[i]} )) $_ZSH_TURBO_LIST_SELECTED_STYLE")
            fi
        done
        (( below )) && POSTDISPLAY+=$'\n'"  …"
    fi
    return 0
}

# 入力中の一覧の窓を決め、reply に (先頭の番号 行数 上の… 下の…) を返す。行数は候補の最大数
# ($_ZSH_TURBO_LIST_ROWS) と、… の行を含めて端末の空き ($1) に収める。ghost の項目が
# 窓の外に出るときは、メニューと同じくその項目が最下行に来るまでずらす
function _zsh_turbo_list_window() {
    emulate -L zsh
    local -i avail=$1 count=${#_ZSH_TURBO_LIST_LABELS} rows first above below target=1
    (( _ZSH_TURBO_LIST_MARKED )) && target=$_ZSH_TURBO_LIST_SELECTED
    rows=$count
    (( _ZSH_TURBO_LIST_ROWS > 0 && rows > _ZSH_TURBO_LIST_ROWS )) && rows=$_ZSH_TURBO_LIST_ROWS
    while true; do
        first=1
        (( target > rows )) && first=$(( target - rows + 1 ))
        above=$(( first > 1 ))
        below=$(( first + rows - 1 < count || _ZSH_TURBO_LIST_TOTAL > count || _ZSH_TURBO_LIST_PARTIAL ))
        (( rows + above + below <= avail || rows <= 1 )) && break
        (( rows-- ))
    done
    # 端末がごく低いときは … を諦めて項目を優先する
    (( rows + above + below > avail )) && above=0
    (( rows + above + below > avail )) && below=0
    reply=($first $rows $above $below)
}

# 次の空白またはパス区切りまでを返す。パス区切りは今回の補完に含める。
function _zsh_turbo_suggestion_step() {
    emulate -L zsh
    local suffix="$1" char quote="" previous=""
    local -i i seen=0
    for (( i=1; i<=${#suffix}; i++ )); do
        char="${suffix[i]}"
        if [[ "$previous" == '\\' ]]; then
            previous=""
            seen=1
            continue
        fi
        if [[ "$char" == '\\' && "$quote" != "'" ]]; then
            previous='\'
            continue
        fi
        if [[ "$char" == "'" || "$char" == '"' ]]; then
            if [[ -z "$quote" ]]; then
                quote="$char"
            elif [[ "$quote" == "$char" ]]; then
                quote=""
            fi
            continue
        fi
        if [[ "$char" == '/' ]]; then
            if (( seen )); then
                (( i++ ))
                break
            fi
            continue
        fi
        if [[ -z "$quote" && "$char" == [[:space:]] ]]; then
            (( seen )) && break
            continue
        fi
        seen=1
    done
    REPLY="${suffix[1,$(( i - 1 ))]}"
}

# キーごとの通常操作は、候補がない場合やカーソルが行末以外の場合に使う。
function _zsh_turbo_accept_by_key() {
    emulate -L zsh
    local action="$1" fallback="$2" suffix="$_ZSH_TURBO_GHOST_SUFFIX" chunk
    if [[ "$action" == default || -z "$suffix" ]] || (( CURSOR != ${#BUFFER} )); then
        zle "$fallback"
        return
    fi
    case "$action" in
        full) chunk="$suffix" ;;
        word)
            chunk="${suffix%% *}"
            [[ "$chunk" == "$suffix" ]] || chunk+=' '
            ;;
        step)
            _zsh_turbo_suggestion_step "$suffix"
            chunk="$REPLY"
            ;;
        *) zle "$fallback"; return ;;
    esac
    [[ -n "$chunk" ]] || { zle "$fallback"; return; }
    _zsh_turbo_clear_suggestion
    BUFFER="${BUFFER}${chunk}"
    CURSOR=${#BUFFER}
    # BUFFER 変更後の再サジェスト・再ハイライトは zle-line-pre-redraw が行う
}

function _zsh_turbo_accept_tab() {
    emulate -L zsh
    _zsh_turbo_accept_by_key "$ZSH_TURBO_KEY_TAB" expand-or-complete
}

function _zsh_turbo_accept_right() {
    emulate -L zsh
    _zsh_turbo_accept_by_key "$ZSH_TURBO_KEY_RIGHT" forward-char
}

function _zsh_turbo_accept_alt_f() {
    emulate -L zsh
    _zsh_turbo_accept_by_key "$ZSH_TURBO_KEY_ALT_F" forward-word
}

function _zsh_turbo_accept_ctrl_right() {
    emulate -L zsh
    _zsh_turbo_accept_by_key "$ZSH_TURBO_KEY_CTRL_RIGHT" forward-word
}

function _zsh_turbo_clear_suggestion() {
    emulate -L zsh
    _zsh_turbo_list_down_bindings off
    POSTDISPLAY=""
    _ZSH_TURBO_SUGGESTION=""
    _zsh_turbo_reset_list
    _ZSH_TURBO_GHOST_SUFFIX=""
    _zsh_turbo_autosuggest_display
}

# ウィジェット登録
zle -N _zsh_turbo_accept_tab
zle -N _zsh_turbo_accept_right
zle -N _zsh_turbo_accept_alt_f
zle -N _zsh_turbo_accept_ctrl_right
zle -N _zsh_turbo_clear_suggestion

# キーバインド
bindkey '^I'      _zsh_turbo_accept_tab
bindkey '^[[C'    _zsh_turbo_accept_right
bindkey '^[OC'    _zsh_turbo_accept_right
bindkey '^[f'     _zsh_turbo_accept_alt_f
bindkey '^[[1;5C' _zsh_turbo_accept_ctrl_right

# BUFFER と CURSOR の変更を一元検知してサジェスト・ハイライトを更新する。
# 個別ウィジェットのラップでは履歴検索・ペースト・補完・undo 等による
# BUFFER 変更を拾えず、古い ghost や構文スパンが残ってしまう。
typeset -g _ZSH_TURBO_LAST_BUFFER=""
typeset -gi _ZSH_TURBO_LAST_CURSOR=-1
typeset -gi _ZSH_TURBO_LAST_HISTNO=-1

function _zsh_turbo_line_pre_redraw() {
    emulate -L zsh
    {
        [[ "${_ZSH_TURBO_MENU_ACTIVE:-0}" == 1 ]] && return
        # 同じ文字列の履歴へ移ると BUFFER は変わらないが HISTNO は変わる。取り直して
        # 入力中に出していたファイル・サブコマンドの一覧 (と取得中の応答) を捨てる
        if [[ "$BUFFER" != "$_ZSH_TURBO_LAST_BUFFER" ]] || (( HISTNO != _ZSH_TURBO_LAST_HISTNO )); then
            # 非同期コールバックの `zle -R` でも本フックは発火するため、
            # 先に更新して再帰・多重起動を防ぐ
            _ZSH_TURBO_LAST_BUFFER="$BUFFER"
            _ZSH_TURBO_LAST_HISTNO=$HISTNO
            _ZSH_TURBO_LAST_CURSOR=$CURSOR
            # Ctrl+C で閉じた一覧は、入力が変わったら再び出す
            [[ "$BUFFER" == "$_ZSH_TURBO_LIST_DISMISSED" ]] || _ZSH_TURBO_LIST_DISMISSED=""
            _zsh_turbo_after_modify
            return
        fi
        (( CURSOR == _ZSH_TURBO_LAST_CURSOR )) && return
        _ZSH_TURBO_LAST_CURSOR=$CURSOR
        _zsh_turbo_autosuggest_display
    } always {
        # この直後の再描画で出す一覧の行数 (_zsh_turbo_edit_lines が差し引く)
        _ZSH_TURBO_DRAWN_LINES=${#${POSTDISPLAY//[^$'\n']/}}
    }
}
zle -N zle-line-pre-redraw _zsh_turbo_line_pre_redraw

# 履歴検索と順位付けは Rust、選択操作と編集バッファへの挿入は ZLE が担当する。
function _zsh_turbo_history_menu_show() {
    emulate -L zsh
    BUFFER="${menu_candidates[$menu_index]}"
    CURSOR=${#BUFFER}
    region_highlight=()
    local -i page_size=$(( LINES > 8 ? LINES - 8 : 1 ))
    (( page_size > 10 )) && page_size=10
    local -i first=$(( (menu_index - 1) / page_size * page_size + 1 ))
    local -i last=$(( first + page_size - 1 ))
    (( last > ${#menu_candidates} )) && last=${#menu_candidates}
    local -a display
    local row marker
    local -i i
    for (( i=first; i<=last; i++ )); do
        if (( i == menu_index )); then
            marker='>'
        else
            marker=' '
        fi
        printf -v row '%3d %s %s' "$i" "$marker" "${menu_candidates[$i]}"
        display+=("$row")
    done
    zle -R "${ZSH_TURBO_HISTORY_MENU_HELP:-Up/Down Tab:select Enter:insert Esc:cancel} [$menu_index/${#menu_candidates}]" "${display[@]}"
}

function _zsh_turbo_history_menu() {
    emulate -L zsh
    local result
    result="$("$ZSH_TURBO_CMD" complete --directory-history --strategy fuzzy --history-file "$HISTFILE" -- "$BUFFER" 2>/dev/null)"
    [[ -n "$result" ]] || { zle -M "${ZSH_TURBO_HISTORY_MENU_EMPTY:-No matching history}"; return; }
    local -a menu_candidates=("${(@f)result}")
    local original_buffer="$BUFFER" original_keymap="$KEYMAP"
    local -i original_cursor=$CURSOR menu_index=1 _ZSH_TURBO_MENU_ACTIVE=1 menu_cancelled=0
    local menu_cancel_key=$'\e'
    setopt localtraps
    trap 'menu_cancelled=1; zle -U -- "$menu_cancel_key"' INT
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_ASYNC_FD"
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_HIGHLIGHT_FD"
    _ZSH_TURBO_ASYNC_FD=0
    _ZSH_TURBO_HIGHLIGHT_FD=0
    _zsh_turbo_clear_suggestion
    {
        zle -K zsh-turbo-history
        local REPLY
        while true; do
            _zsh_turbo_history_menu_show
            if ! zle .read-command; then
                REPLY=send-break
            fi
            (( menu_cancelled )) && REPLY=send-break
            case "$REPLY" in
                accept-line) break ;;
                send-break)
                    BUFFER="$original_buffer"
                    CURSOR=$original_cursor
                    break ;;
                down-line-or-history)
                    (( menu_index = menu_index % ${#menu_candidates} + 1 )) ;;
                up-line-or-history)
                    (( menu_index = (menu_index + ${#menu_candidates} - 2) % ${#menu_candidates} + 1 )) ;;
                *) zle beep ;;
            esac
        done
    } always {
        zle -K "$original_keymap"
        zle -R -c
        _ZSH_TURBO_LAST_BUFFER=""
    }
}
zle -N _zsh_turbo_history_menu

function _zsh_turbo_list_menu_show() {
    emulate -L zsh
    region_highlight=("${menu_syntax_hl[@]}")
    local -i count=${#menu_values} page=$menu_page
    (( page > 0 )) || page=1
    (( page > count )) && page=$count
    # 窓の下に空きを残さない (絞り込みで項目が減ったときも上へ詰める)
    (( menu_first > count - page + 1 )) && menu_first=$(( count - page + 1 ))
    (( menu_first < 1 )) && menu_first=1
    # 選択行が窓の外へ出たときだけ窓を動かす
    (( menu_index < menu_first )) && menu_first=$menu_index
    (( menu_index > menu_first + page - 1 )) && menu_first=$(( menu_index - page + 1 ))
    local -i last=$(( menu_first + page - 1 ))
    local row marker
    local -i row_start i
    _zsh_turbo_list_header "$menu_title" "$menu_total" "$menu_partial" "$menu_index"
    POSTDISPLAY=$'\n'"$REPLY"
    for (( i = menu_first; i <= last; i++ )); do
        if (( i == menu_index )); then
            marker='> '
        else
            marker='  '
        fi
        row="${marker}${menu_labels[i]}"
        row_start=$(( ${#BUFFER} + ${#POSTDISPLAY} + 1 ))
        POSTDISPLAY+=$'\n'"$row"
        if (( i == menu_index )); then
            region_highlight+=("$row_start $(( row_start + ${#row} )) $_ZSH_TURBO_LIST_SELECTED_STYLE")
        fi
    done
    zle -R
}

function _zsh_turbo_list_menu() {
    emulate -L zsh
    local -a menu_labels=("${_ZSH_TURBO_LIST_LABELS[@]}") menu_values=("${_ZSH_TURBO_LIST_VALUES[@]}")
    local menu_title="$_ZSH_TURBO_LIST_TITLE" menu_input="$_ZSH_TURBO_LIST_INPUT"
    local -i menu_total=$_ZSH_TURBO_LIST_TOTAL menu_partial=$_ZSH_TURBO_LIST_PARTIAL
    local original_buffer="$BUFFER" original_keymap="$KEYMAP"
    # 入力中の一覧と同じ位置から表示する (選択行が見えなければ _zsh_turbo_list_menu_show がずらす)
    local -i menu_index=$_ZSH_TURBO_LIST_SELECTED menu_first=$_ZSH_TURBO_LIST_FIRST _ZSH_TURBO_MENU_ACTIVE=1 menu_cancelled=0
    # 一覧の取り直し: 応答を読む fd と期限、一覧が編集後の BUFFER に追いついていないか、
    # 要求の後に ↑↓ で選び直したか、メニューを閉じるか
    local -i menu_fetch_fd=0 menu_stale=0 menu_moved=0 menu_done=0
    local -F menu_deadline=0
    (( menu_index >= 1 && menu_index <= ${#menu_values} )) || menu_index=1
    local menu_cancel_key=$'\e'
    # 表示行数は開いた時点で決めて固定する。描くたびに測り直すと、BUFFERLINES が
    # 直前のメニューの行を含むため行数が交互に増減する
    _zsh_turbo_edit_lines
    local -i menu_page=$(( LINES - REPLY - 4 ))
    setopt localtraps
    trap 'menu_cancelled=1; zle -U -- "$menu_cancel_key"' INT
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_ASYNC_FD"
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_HIGHLIGHT_FD"
    _ZSH_TURBO_ASYNC_FD=0
    _ZSH_TURBO_HIGHLIGHT_FD=0
    _zsh_turbo_clear_suggestion
    # 自分で開いた一覧なので、以前 Ctrl+C で閉じた記録は解く
    _ZSH_TURBO_LIST_DISMISSED=""
    local -a menu_syntax_hl=("${region_highlight[@]}")
    {
        zle -K zsh-turbo-list
        local REPLY
        # 打ち切った名前の項目も含むメニュー用の一覧に取り直す。届くまでは入力中の一覧を出す
        _zsh_turbo_list_menu_request
        while (( ! menu_done )); do
            _zsh_turbo_list_menu_show
            # 取り直し中は応答かキー入力を待つ。応答が先なら描き直す
            _zsh_turbo_list_menu_wait && continue
            if ! zle .read-command; then
                REPLY=send-break
            fi
            # 端末が Ctrl+C を SIGINT として送った場合も、キー入力と同じく取り消す
            (( menu_cancelled )) && REPLY=_zsh_turbo_list_cancel
            case "$REPLY" in
                accept-line)
                    # 取り直し中は古い一覧から選ばず、届くのを待ってから採用する。打った文字に
                    # 追いついていない一覧 (stale) からは選ばない。開いた直後の取り直しが間に合わ
                    # なかったときは、同じ入力に対する入力中の一覧 (表示中の選択) を採用する
                    _zsh_turbo_list_menu_settle
                    if (( menu_cancelled )); then
                        # 待っている間の Ctrl+C (SIGINT) は取り消し。trap が積んだ Esc は読み捨てる
                        (( KEYS_QUEUED_COUNT )) && zle .read-command
                        _ZSH_TURBO_LIST_DISMISSED="$BUFFER"
                    elif (( ! menu_done && ! menu_stale )); then
                        BUFFER="${menu_values[$menu_index]}"
                    fi
                    CURSOR=${#BUFFER}
                    break ;;
                send-break)
                    # 選択だけ取り消し、打った文字は残す
                    CURSOR=${#BUFFER}
                    break ;;
                _zsh_turbo_list_cancel)
                    # Ctrl+C は打った文字を残し、入力を変えるまで一覧も閉じる
                    CURSOR=${#BUFFER}
                    _ZSH_TURBO_LIST_DISMISSED="$BUFFER"
                    break ;;
                down-line-or-history)
                    (( menu_index = menu_index % ${#menu_values} + 1 ))
                    menu_moved=1 ;;
                up-line-or-history)
                    (( menu_index = (menu_index + ${#menu_values} - 2) % ${#menu_values} + 1 ))
                    menu_moved=1 ;;
                self-insert|bracketed-paste|backward-delete-char)
                    _zsh_turbo_list_menu_edit "$REPLY" ;;
                *) zle beep ;;
            esac
        done
    } always {
        _zsh_turbo_close_async_fd "$menu_fetch_fd"
        zle -K "$original_keymap"
        POSTDISPLAY=""
        if [[ "$BUFFER" == "$original_buffer" ]]; then
            region_highlight=("${menu_syntax_hl[@]}")
        else
            region_highlight=()
        fi
        zle -R -c
        _ZSH_TURBO_LAST_BUFFER=""
    }
}

# メニューの一覧を現在の BUFFER で取り直す。応答は _zsh_turbo_list_menu_wait がキー入力の合間に受け取る
function _zsh_turbo_list_menu_request() {
    emulate -L zsh
    _zsh_turbo_close_async_fd "$menu_fetch_fd"
    menu_fetch_fd=0
    menu_moved=0
    if [[ -z "$BUFFER" ]]; then
        menu_done=1
        return 0
    fi
    local REPLY
    # メニューの中で打っているので、履歴移動の直後としては扱わない
    _zsh_turbo_list_request "" --menu --selected "${menu_values[$menu_index]:-}"
    menu_fetch_fd=$REPLY
    (( menu_deadline = EPOCHREALTIME + 1 ))
}

# 取り直した一覧の応答を受け取り、メニューの項目と選択を差し替える
function _zsh_turbo_list_menu_receive() {
    emulate -L zsh
    local previous="${menu_values[$menu_index]:-}"
    local -a reply
    _zsh_turbo_read_response "$menu_fetch_fd"
    _zsh_turbo_close_async_fd "$menu_fetch_fd"
    menu_fetch_fd=0
    _zsh_turbo_apply_response "${reply[@]}"
    if (( ! ${#_ZSH_TURBO_LIST_VALUES} )); then
        # 打った文字で候補が無くなったら閉じる。開いた直後の取り直しが空なら
        # (--menu を知らない旧版の CLI など)、入力中の一覧をそのまま使う
        (( menu_stale )) && menu_done=1
        return 0
    fi
    menu_labels=("${_ZSH_TURBO_LIST_LABELS[@]}")
    menu_values=("${_ZSH_TURBO_LIST_VALUES[@]}")
    menu_title="$_ZSH_TURBO_LIST_TITLE"
    menu_total=$_ZSH_TURBO_LIST_TOTAL
    menu_partial=$_ZSH_TURBO_LIST_PARTIAL
    menu_input="$_ZSH_TURBO_LIST_INPUT"
    menu_stale=0
    # 窓の位置は保つ (項目が減ったときは _zsh_turbo_list_menu_show が詰める)
    # 要求の後に ↑↓ で選び直していれば、その項目を選び続ける。ほかは Rust が決めた項目
    local -i kept=0
    (( menu_moved )) && kept=${menu_values[(Ie)$previous]}
    if (( kept )); then
        menu_index=$kept
    else
        menu_index=$_ZSH_TURBO_LIST_SELECTED
    fi
    menu_moved=0
}

# 取り直し中なら、応答かキー入力のどちらかが来るまで待つ。応答を受け取った (または取り直しを
# 打ち切った) ら 0、キー入力が先に来た・取り直していないなら 1 を返す。
# read-command の最中は zle -F の処理が呼ばれないため、ここで応答を見張る
function _zsh_turbo_list_menu_wait() {
    emulate -L zsh
    if (( ! ${+builtins[zselect]} )); then
        (( menu_fetch_fd )) || return 1
        _zsh_turbo_list_menu_receive
        return 0
    fi
    while (( menu_fetch_fd )); do
        (( PENDING + KEYS_QUEUED_COUNT )) && return 1
        if (( EPOCHREALTIME > menu_deadline )); then
            _zsh_turbo_close_async_fd "$menu_fetch_fd"
            menu_fetch_fd=0
            # 打った文字に一覧が追いつかないまま、古い項目を選ばせないよう閉じる
            (( menu_stale )) && menu_done=1
            return 0
        fi
        zselect -t 2 -r "$menu_fetch_fd" 2>/dev/null
        case $? in
            0) _zsh_turbo_list_menu_receive; return 0 ;;
            1) ;;
            *) menu_deadline=0 ;;
        esac
    done
    return 1
}

# Enter の後は古い一覧から選ばないよう、取り直し中の応答を期限まで待って受け取る
function _zsh_turbo_list_menu_settle() {
    emulate -L zsh
    (( menu_fetch_fd )) || return 0
    if (( ! ${+builtins[zselect]} )); then
        _zsh_turbo_list_menu_receive
        return 0
    fi
    local -i centis=0
    (( centis = (menu_deadline - EPOCHREALTIME) * 100 ))
    if (( centis > 0 )) && zselect -t "$centis" -r "$menu_fetch_fd" 2>/dev/null; then
        _zsh_turbo_list_menu_receive
        return 0
    fi
    _zsh_turbo_close_async_fd "$menu_fetch_fd"
    menu_fetch_fd=0
}

# メニューで打った文字・貼り付け・Backspace で入力欄を編集し、一覧を取り直す
function _zsh_turbo_list_menu_edit() {
    emulate -L zsh
    local widget="$1" before="$BUFFER" pasted=""
    CURSOR=${#BUFFER}
    case "$widget" in
        backward-delete-char)
            zle .backward-delete-char ;;
        bracketed-paste)
            zle .bracketed-paste pasted
            # 空白始まりでなければ、区切りを補った位置 (`make` なら `make `) に続ける
            [[ -n "$menu_input" && "$pasted" != [[:space:]]* ]] && BUFFER="$menu_input"
            BUFFER+="$pasted" ;;
        *)
            [[ -n "$menu_input" && "$KEYS" != [[:space:]]* ]] && BUFFER="$menu_input"
            CURSOR=${#BUFFER}
            # 多バイト文字の残りのバイトも ZLE に読ませる
            zle .self-insert ;;
    esac
    CURSOR=${#BUFFER}
    [[ "$BUFFER" == "$before" ]] && return 0
    # 区切りの補い方は次の応答で決め直す。編集した行の構文色は古いので外す
    menu_input=""
    menu_stale=1
    menu_syntax_hl=()
    _zsh_turbo_list_menu_request
}

# 一覧が出ていれば ↓ でメニューへ入り、なければ元の ↓ (履歴検索) を実行する。
function _zsh_turbo_list_menu_or_history() {
    emulate -L zsh
    _ZSH_TURBO_DOWN_WIDGET=""
    if (( CURSOR == ${#BUFFER} )) && [[ -n "$BUFFER" ]]; then
        # 連続入力で再描画 (と取得の開始) が省かれた直後にも一覧を開けるよう、
        # 現在の BUFFER の取得を始めて結果を待つ
        [[ "$_ZSH_TURBO_ASYNC_BUFFER" == "$BUFFER" ]] || _zsh_turbo_autosuggest_fetch
        _zsh_turbo_await_async
        if (( ${#_ZSH_TURBO_LIST_VALUES} )); then
            _zsh_turbo_list_menu
            return
        fi
    fi
    local key="$KEYS" binding fallback
    _zsh_turbo_list_down_bindings off
    binding="$(bindkey -M "$KEYMAP" "$key")"
    fallback="${binding##* }"
    _ZSH_TURBO_DOWN_WIDGET="$fallback"
    # キーを再投入すると LASTWIDGET が変わり、検索開始時の prefix を失う。
    zle "$fallback"
}
zle -N _zsh_turbo_list_menu_or_history
zle -N _zsh_turbo_list_menu

# メニューの Ctrl+C を Esc と区別するための名前。read-command が返すだけで実行はしない
function _zsh_turbo_list_cancel() {
    emulate -L zsh
}
zle -N _zsh_turbo_list_cancel
bindkey -N zsh-turbo-history
bindkey -M zsh-turbo-history '^I' down-line-or-history
bindkey -M zsh-turbo-history '^[[B' down-line-or-history
bindkey -M zsh-turbo-history '^[OB' down-line-or-history
bindkey -M zsh-turbo-history '^N' down-line-or-history
bindkey -M zsh-turbo-history '^[[A' up-line-or-history
bindkey -M zsh-turbo-history '^[OA' up-line-or-history
bindkey -M zsh-turbo-history '^[[Z' up-line-or-history
bindkey -M zsh-turbo-history '^P' up-line-or-history
bindkey -M zsh-turbo-history '^M' accept-line
bindkey -M zsh-turbo-history '^J' accept-line
bindkey -M zsh-turbo-history '^[' send-break
bindkey -M zsh-turbo-history '^C' send-break
bindkey -N zsh-turbo-list
# 打った文字・貼り付け・Backspace は入力欄を編集して一覧を絞り込む
bindkey -M zsh-turbo-list -R ' '-'~' self-insert
bindkey -M zsh-turbo-list -R '\M-^@'-'\M-^?' self-insert
bindkey -M zsh-turbo-list '^?' backward-delete-char
bindkey -M zsh-turbo-list '^H' backward-delete-char
bindkey -M zsh-turbo-list '^[[200~' bracketed-paste
bindkey -M zsh-turbo-list '^I' down-line-or-history
bindkey -M zsh-turbo-list '^[[B' down-line-or-history
bindkey -M zsh-turbo-list '^[OB' down-line-or-history
bindkey -M zsh-turbo-list '^N' down-line-or-history
bindkey -M zsh-turbo-list '^[[A' up-line-or-history
bindkey -M zsh-turbo-list '^[OA' up-line-or-history
bindkey -M zsh-turbo-list '^[[Z' up-line-or-history
bindkey -M zsh-turbo-list '^P' up-line-or-history
bindkey -M zsh-turbo-list '^M' accept-line
bindkey -M zsh-turbo-list '^J' accept-line
bindkey -M zsh-turbo-list '^[' send-break
bindkey -M zsh-turbo-list '^C' _zsh_turbo_list_cancel
bindkey -M emacs '^R' _zsh_turbo_history_menu
bindkey -M viins '^R' _zsh_turbo_history_menu
for _zsh_turbo_keymap in emacs viins; do
    bindkey -M "$_zsh_turbo_keymap" '^I' _zsh_turbo_accept_tab
    bindkey -M "$_zsh_turbo_keymap" '^[[C' _zsh_turbo_accept_right
    bindkey -M "$_zsh_turbo_keymap" '^[OC' _zsh_turbo_accept_right
    bindkey -M "$_zsh_turbo_keymap" '^[f' _zsh_turbo_accept_alt_f
    bindkey -M "$_zsh_turbo_keymap" '^[[1;5C' _zsh_turbo_accept_ctrl_right
done
unset _zsh_turbo_keymap

# ─── シンタックスハイライト（Rust 実装） ───────────────────────

typeset -g _ZSH_TURBO_HIGHLIGHT_ALIASES=""
typeset -g _ZSH_TURBO_HIGHLIGHT_FUNCTIONS=""
typeset -gi _ZSH_TURBO_HIGHLIGHT_FD=0

# 初期化時に aliases/functions をキャッシュし、precmd で更新する
function _zsh_turbo_refresh_shell_objects() {
    emulate -L zsh
    # ${(F)...} = 実改行での join。double quote 付きのネスト展開は zsh の展開規則
    # (quoted joining が先に走る) により空白 join になり、さらに (j:\n:) の \n は
    # リテラル 2 文字のため、Rust 側の split('\n') と一致せず全エイリアス・関数が
    # 未知コマンド (赤) になる。代入の右辺は word split されないため quote 不要。
    _ZSH_TURBO_HIGHLIGHT_ALIASES=${(F)${(k)aliases}}
    _ZSH_TURBO_HIGHLIGHT_FUNCTIONS=${(F)${(k)functions}}
}
_zsh_turbo_refresh_shell_objects
add-zsh-hook precmd _zsh_turbo_refresh_shell_objects

function _zsh_turbo_highlight() {
    emulate -L zsh
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_HIGHLIGHT_FD"
    _ZSH_TURBO_HIGHLIGHT_FD=0
    [[ -z "$BUFFER" ]] && { region_highlight=(); return; }

    # リクエスト時点の BUFFER を保存し、応答が古くなっていないか callback で照合する
    typeset -g _ZSH_TURBO_HIGHLIGHT_BUFFER="$BUFFER"

    # 非同期ハイライトを開始 (`--` は BUFFER が `-h` 等と一致した際の help 漏れを防ぐ)
    exec {_ZSH_TURBO_HIGHLIGHT_FD}< <(
        "$ZSH_TURBO_CMD" highlight \
            --aliases "$_ZSH_TURBO_HIGHLIGHT_ALIASES" \
            --functions "$_ZSH_TURBO_HIGHLIGHT_FUNCTIONS" -- "$BUFFER" 2>/dev/null
        printf '\n'
    )
    zle -F "$_ZSH_TURBO_HIGHLIGHT_FD" _zsh_turbo_async_dispatch
}

function _zsh_turbo_highlight_callback() {
    emulate -L zsh
    local fd="$1"
    local -a new_highlights
    local line

    while IFS= read -r -u "$fd" line 2>/dev/null; do
        [[ -n "$line" ]] && new_highlights+=("$line")
    done

    _zsh_turbo_close_async_fd "$fd"
    _ZSH_TURBO_HIGHLIGHT_FD=0

    # リクエスト時から BUFFER が変わっていたら、古いスパンを適用しない
    [[ "$BUFFER" == "$_ZSH_TURBO_HIGHLIGHT_BUFFER" ]] || return

    # サジェストのハイライト（末尾要素）を残し、構文ハイライトを差し替える。
    # 空スパンも正しい結果なので、古いハイライトを必ず消す。
    local -a suggest_hl
    local entry
    for entry in "${region_highlight[@]}"; do
        # バッファ長以降のエントリ（サジェストハイライト）を残す
        local start="${entry%% *}"
        if (( start >= ${#BUFFER} )); then
            suggest_hl+=("$entry")
        fi
    done
    region_highlight=("${new_highlights[@]}" "${suggest_hl[@]}")
    zle -R
}
zle -N _zsh_turbo_highlight_callback

# 編集ウィジェットに連動してハイライトを更新する
function _zsh_turbo_after_modify() {
    emulate -L zsh
    _zsh_turbo_autosuggest_fetch
    _zsh_turbo_autosuggest_display
    _zsh_turbo_highlight
}

# ─── 補完強化 ───────────────────────────────────────────────────

# 追加の `_command` 補完ファイルを置いたディレクトリを compinit 前に追加する。
function _zsh_turbo_setup_completion_dirs() {
    emulate -L zsh
    local completion_dir
    for completion_dir in "${(@s.:.)ZSH_TURBO_COMPLETION_DIRS}"; do
        [[ -n "$completion_dir" && -d "$completion_dir" ]] || continue
        (( ${fpath[(Ie)$completion_dir]} == 0 )) && fpath+=("$completion_dir")
    done
}
_zsh_turbo_setup_completion_dirs

autoload -Uz compinit

if [[ -z "${ZSH_COMPDUMP:-}" ]]; then
    ZSH_COMPDUMP="${ZDOTDIR:-$HOME}/.zcompdump-zsh-turbo"
fi

# comp dump の再生成は 1 日 1 回まで。同じ通日でも年が違うファイルは再生成する。
# compinit はユーザのシェルオプション(SH_GLOB 等)下で誤動作するため、
# 無名関数 + emulate -L zsh でクリーンな zsh オプション下で実行する。
() {
    emulate -L zsh
    if [[ -f "$ZSH_COMPDUMP" ]] && [[ $(date +'%Y-%j') == $(date -r "$ZSH_COMPDUMP" +'%Y-%j' 2>/dev/null) ]]; then
        compinit -C -d "$ZSH_COMPDUMP"
    else
        compinit -d "$ZSH_COMPDUMP"
    fi
}

# 補完スタイリング
zmodload zsh/complist 2>/dev/null
zstyle ':completion:*' menu select
zstyle ':completion:*' matcher-list \
    'm:{a-zA-Z}={A-Za-z}' \
    'r:|=*' \
    'l:|=* r:|=*'
zstyle ':completion:*' list-colors "${(s.:.)${LS_COLORS:-}}"
zstyle ':completion:*' group-name ''
zstyle ':completion:*:descriptions' format '%F{blue}── %d ──%f'
zstyle ':completion:*:warnings' format '%F{red}no matches found%f'
zstyle ':completion:*:corrections' format '%F{yellow}%d (errors: %e)%f'
zstyle ':completion:*' squeeze-slashes true
zstyle ':completion:*' complete-options true

# 補完キャッシュ
zstyle ':completion:*' use-cache on
zstyle ':completion:*' cache-path "${XDG_CACHE_HOME:-$HOME/.cache}/zsh-turbo/completions"

# ディレクトリ補完
zstyle ':completion:*' special-dirs true
zstyle ':completion:*:cd:*' tag-order local-directories directory-stack path-directories

# プロセス補完
zstyle ':completion:*:*:kill:*' menu yes select
zstyle ':completion:*:kill:*' force-list always
zstyle ':completion:*:*:kill:*:processes' list-colors '=(#b) #([0-9]#)*=0=01;31'

# ─── キーバインド ───────────────────────────────────────────────

# 矢印キーで履歴検索
autoload -Uz up-line-or-beginning-search down-line-or-beginning-search
zle -N up-line-or-beginning-search
zle -N down-line-or-beginning-search
bindkey '^[[A' up-line-or-beginning-search    # 上矢印
bindkey '^[OA' up-line-or-beginning-search    # 上矢印（代替）
bindkey '^[[B' down-line-or-beginning-search  # 下矢印
bindkey '^[OB' down-line-or-beginning-search  # 下矢印（代替）

# 単語単位の移動
bindkey '^[[1;5D' backward-word  # Ctrl+左矢印
bindkey '^[b'     backward-word  # Alt+b
bindkey '^A'      beginning-of-line
bindkey '^E'      end-of-line

# ─── 履歴設定 ───────────────────────────────────────────────────

HISTSIZE=50000
SAVEHIST=50000
HISTFILE="${HISTFILE:-$HOME/.zsh_history}"
setopt EXTENDED_HISTORY
setopt HIST_EXPIRE_DUPS_FIRST
setopt HIST_IGNORE_DUPS
setopt HIST_IGNORE_SPACE
setopt HIST_VERIFY
setopt SHARE_HISTORY
setopt INC_APPEND_HISTORY

# ─── 初期化完了 ─────────────────────────────────────────────────

_zsh_turbo_precmd
