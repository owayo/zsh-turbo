#!/usr/bin/env zsh
# zsh-turbo: zsh 統合スクリプト
# 使い方: eval "$(zsh-turbo init)"

# ─── プロンプト連携 ─────────────────────────────────────────────

typeset -g ZSH_TURBO_CMD="${commands[zsh-turbo]:-zsh-turbo}"
typeset -g ZSH_TURBO_TRANSIENT="${ZSH_TURBO_TRANSIENT:-0}"
typeset -g ZSH_TURBO_SUGGEST_STRATEGY="${ZSH_TURBO_SUGGEST_STRATEGY:-prefix}"
typeset -g ZSH_TURBO_SUGGEST_HIGHLIGHT="${ZSH_TURBO_SUGGEST_HIGHLIGHT:-fg=8}"
typeset -g ZSH_TURBO_TASK_LIST_LABEL="${ZSH_TURBO_TASK_LIST_LABEL:-Tasks}"
typeset -g ZSH_TURBO_KEY_TAB="${ZSH_TURBO_KEY_TAB:-full}"
typeset -g ZSH_TURBO_KEY_RIGHT="${ZSH_TURBO_KEY_RIGHT:-step}"
typeset -g ZSH_TURBO_KEY_ALT_F="${ZSH_TURBO_KEY_ALT_F:-word}"
typeset -g ZSH_TURBO_KEY_CTRL_RIGHT="${ZSH_TURBO_KEY_CTRL_RIGHT:-word}"
typeset -g ZSH_TURBO_COMPLETION_DIRS="${ZSH_TURBO_COMPLETION_DIRS:-}"
typeset -g ZSH_TURBO_TERM_SHELL_INTEGRATION="${ZSH_TURBO_TERM_SHELL_INTEGRATION:-auto}"

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
typeset -ga _ZSH_TURBO_PROJECT_CANDIDATES=()
typeset -gi _ZSH_TURBO_ASYNC_FD=0
typeset -g _ZSH_TURBO_ASYNC_BUFFER=""

function _zsh_turbo_close_async_fd() {
    emulate -L zsh
    local fd="$1"
    (( fd > 0 )) || return 0
    zle -F "$fd" 2>/dev/null
    # exec 単体へのリダイレクトは呼び出し元の stderr まで変更する。
    { exec {fd}<&- } 2>/dev/null
}

function _zsh_turbo_autosuggest_fetch() {
    emulate -L zsh
    local prefix="$BUFFER"
    _zsh_turbo_close_async_fd "$_ZSH_TURBO_ASYNC_FD"
    _ZSH_TURBO_ASYNC_FD=0
    if [[ -z "$prefix" ]]; then
        _ZSH_TURBO_SUGGESTION=""
        _ZSH_TURBO_PROJECT_CANDIDATES=()
        _ZSH_TURBO_GHOST_SUFFIX=""
        POSTDISPLAY=""
        return
    fi

    # 新しい検索結果が返るまで古い候補を表示しない
    _ZSH_TURBO_SUGGESTION=""
    _ZSH_TURBO_PROJECT_CANDIDATES=()
    _ZSH_TURBO_GHOST_SUFFIX=""
    POSTDISPLAY=""

    _ZSH_TURBO_ASYNC_BUFFER="$prefix"

    # 非同期取得を開始。
    # - `--` 区切り: BUFFER が `-h` 等の登録済みフラグと一致したときに clap の
    #   help が stdout へ漏れ、候補として表示されるのを防ぐ。
    # - `--history-file "$HISTFILE"`: HISTFILE はシェル変数で export されないため、
    #   明示的に渡さないと子プロセスはカスタム履歴パスを解決できない。
    exec {_ZSH_TURBO_ASYNC_FD}< <("$ZSH_TURBO_CMD" suggest --project-list --strategy "$ZSH_TURBO_SUGGEST_STRATEGY" --history-file "$HISTFILE" -- "$prefix" 2>/dev/null; printf '\n')
    zle -F -w "$_ZSH_TURBO_ASYNC_FD" _zsh_turbo_async_callback
}

function _zsh_turbo_async_callback() {
    emulate -L zsh
    local fd="$1"
    local suggestion="" kind="" candidate
    local -a project_candidates

    IFS= read -r -u "$fd" kind 2>/dev/null
    if [[ "$kind" == project ]]; then
        while IFS= read -r -u "$fd" candidate 2>/dev/null; do
            [[ -n "$candidate" ]] && project_candidates+=("$candidate")
        done
        suggestion="${project_candidates[1]:-}"
    elif [[ "$kind" == history ]]; then
        IFS= read -r -u "$fd" suggestion 2>/dev/null
    fi

    # fd を片付ける
    _zsh_turbo_close_async_fd "$fd"
    _ZSH_TURBO_ASYNC_FD=0

    # リクエスト後にバッファが変わっていない場合だけ反映する
    if [[ "$BUFFER" == "$_ZSH_TURBO_ASYNC_BUFFER" ]]; then
        _ZSH_TURBO_PROJECT_CANDIDATES=("${project_candidates[@]}")
        if [[ -n "$suggestion" ]]; then
            _ZSH_TURBO_SUGGESTION="$suggestion"
        else
            _ZSH_TURBO_SUGGESTION=""
            POSTDISPLAY=""
        fi
        _zsh_turbo_autosuggest_display
        zle -R
    fi
}
zle -N _zsh_turbo_async_callback

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

    if (( ${#_ZSH_TURBO_PROJECT_CANDIDATES} )); then
        local candidate
        local -i count=0 limit=$(( LINES - BUFFERLINES - 4 ))
        (( limit > 0 )) || return 0
        POSTDISPLAY+=$'\n'"$ZSH_TURBO_TASK_LIST_LABEL (${#_ZSH_TURBO_PROJECT_CANDIDATES}):"
        for candidate in "${_ZSH_TURBO_PROJECT_CANDIDATES[@]}"; do
            POSTDISPLAY+=$'\n'"  ${candidate##* }"
            (( ++count >= limit )) && break
        done
    fi
    return 0
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
    POSTDISPLAY=""
    _ZSH_TURBO_SUGGESTION=""
    _ZSH_TURBO_PROJECT_CANDIDATES=()
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

function _zsh_turbo_line_pre_redraw() {
    emulate -L zsh
    [[ "${_ZSH_TURBO_MENU_ACTIVE:-0}" == 1 ]] && return
    if [[ "$BUFFER" != "$_ZSH_TURBO_LAST_BUFFER" ]]; then
        # 非同期コールバックの `zle -R` でも本フックは発火するため、
        # 先に更新して再帰・多重起動を防ぐ
        _ZSH_TURBO_LAST_BUFFER="$BUFFER"
        _ZSH_TURBO_LAST_CURSOR=$CURSOR
        _zsh_turbo_after_modify
        return
    fi
    (( CURSOR == _ZSH_TURBO_LAST_CURSOR )) && return
    _ZSH_TURBO_LAST_CURSOR=$CURSOR
    _zsh_turbo_autosuggest_display
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
    result="$("$ZSH_TURBO_CMD" complete --strategy fuzzy --history-file "$HISTFILE" -- "$BUFFER" 2>/dev/null)"
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
    zle -F -w "$_ZSH_TURBO_HIGHLIGHT_FD" _zsh_turbo_highlight_callback
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
