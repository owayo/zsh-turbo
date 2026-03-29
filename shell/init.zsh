#!/usr/bin/env zsh
# zsh-turbo: zsh integration script
# Usage: eval "$(zsh-turbo init)"

# ─── Prompt Integration ───────────────────────────────────────────

typeset -g ZSH_TURBO_CMD="${commands[zsh-turbo]:-zsh-turbo}"
typeset -g ZSH_TURBO_TRANSIENT="${ZSH_TURBO_TRANSIENT:-0}"
typeset -g ZSH_TURBO_SUGGEST_STRATEGY="${ZSH_TURBO_SUGGEST_STRATEGY:-prefix}"
typeset -g ZSH_TURBO_SUGGEST_HIGHLIGHT="${ZSH_TURBO_SUGGEST_HIGHLIGHT:-fg=8}"

# Timer for command duration tracking
typeset -g _ZSH_TURBO_CMD_EXECUTED=0

function _zsh_turbo_preexec() {
    typeset -g _ZSH_TURBO_START_TIME="${EPOCHREALTIME}"
    _ZSH_TURBO_CMD_EXECUTED=1
}

function _zsh_turbo_precmd() {
    local last_status=$?
    local duration_ms=0

    if [[ -n "$_ZSH_TURBO_START_TIME" ]]; then
        local end_time="${EPOCHREALTIME}"
        local elapsed
        (( elapsed = end_time - _ZSH_TURBO_START_TIME ))
        (( duration_ms = ${elapsed%.*} * 1000 + 10#${${elapsed#*.}[1,3]:-0} ))
        unset _ZSH_TURBO_START_TIME
    fi

    local jobs_count=${(%):-%j}

    PROMPT="$("$ZSH_TURBO_CMD" prompt --last-status "$last_status" --duration-ms "$duration_ms" --jobs "$jobs_count" --side left 2>/dev/null)" || PROMPT='%~ %# '
    RPROMPT="$("$ZSH_TURBO_CMD" prompt --last-status "$last_status" --duration-ms "$duration_ms" --jobs "$jobs_count" --side right 2>/dev/null)" || RPROMPT=''
}

autoload -Uz add-zsh-hook
add-zsh-hook preexec _zsh_turbo_preexec
add-zsh-hook precmd _zsh_turbo_precmd

# Enable EPOCHREALTIME for sub-second timing
zmodload zsh/datetime 2>/dev/null

# ─── Transient Prompt ─────────────────────────────────────────────

function _zsh_turbo_zle_line_finish() {
    [[ "$ZSH_TURBO_TRANSIENT" != "1" ]] && return

    # Replace full prompt with minimal version before command executes
    PROMPT=$'%F{%(?.green.red)}\u{276f}%f '
    RPROMPT=""
    zle .reset-prompt 2>/dev/null
}
zle -N zle-line-finish _zsh_turbo_zle_line_finish

# ─── Autosuggestion Integration (Async) ──────────────────────────

typeset -g _ZSH_TURBO_SUGGESTION=""
typeset -gi _ZSH_TURBO_ASYNC_FD=0
typeset -g _ZSH_TURBO_ASYNC_BUFFER=""

function _zsh_turbo_autosuggest_fetch() {
    local prefix="$BUFFER"
    if [[ -z "$prefix" ]]; then
        _ZSH_TURBO_SUGGESTION=""
        POSTDISPLAY=""
        return
    fi

    # Cancel previous async request
    if (( _ZSH_TURBO_ASYNC_FD > 0 )); then
        zle -F "$_ZSH_TURBO_ASYNC_FD" 2>/dev/null
        exec {_ZSH_TURBO_ASYNC_FD}<&- 2>/dev/null
        _ZSH_TURBO_ASYNC_FD=0
    fi

    _ZSH_TURBO_ASYNC_BUFFER="$prefix"

    # Launch async fetch
    exec {_ZSH_TURBO_ASYNC_FD}< <("$ZSH_TURBO_CMD" suggest "$prefix" --strategy "$ZSH_TURBO_SUGGEST_STRATEGY" 2>/dev/null; printf '\0')
    zle -F "$_ZSH_TURBO_ASYNC_FD" _zsh_turbo_async_callback
}

function _zsh_turbo_async_callback() {
    local fd="$1"
    local suggestion=""

    # Read result
    if read -r -u "$fd" suggestion 2>/dev/null; then
        : # got it
    fi

    # Clean up fd
    zle -F "$fd" 2>/dev/null
    exec {fd}<&- 2>/dev/null
    _ZSH_TURBO_ASYNC_FD=0

    # Only apply if buffer hasn't changed since request
    if [[ "$BUFFER" == "$_ZSH_TURBO_ASYNC_BUFFER" && -n "$suggestion" ]]; then
        _ZSH_TURBO_SUGGESTION="$suggestion"
        _zsh_turbo_autosuggest_display
        zle -R
    fi
}

function _zsh_turbo_autosuggest_display() {
    if [[ -n "$_ZSH_TURBO_SUGGESTION" && "$_ZSH_TURBO_SUGGESTION" != "$BUFFER" ]]; then
        local suffix="${_ZSH_TURBO_SUGGESTION#$BUFFER}"
        if [[ -n "$suffix" ]]; then
            POSTDISPLAY="$suffix"
            region_highlight=("$(( ${#BUFFER} )) $(( ${#BUFFER} + ${#suffix} )) ${ZSH_TURBO_SUGGEST_HIGHLIGHT}")
            return
        fi
    fi
    POSTDISPLAY=""
}

function _zsh_turbo_after_modify() {
    _zsh_turbo_autosuggest_fetch
    # Inline display for immediate feedback (async callback updates later)
    _zsh_turbo_autosuggest_display
}

# Accept full suggestion (right arrow / end of line)
function _zsh_turbo_accept_suggestion() {
    if [[ -n "$POSTDISPLAY" ]]; then
        BUFFER="${BUFFER}${POSTDISPLAY}"
        CURSOR=${#BUFFER}
        POSTDISPLAY=""
    else
        zle forward-char
    fi
}

# Accept one word from suggestion (alt+f / ctrl+right)
function _zsh_turbo_accept_word() {
    if [[ -n "$POSTDISPLAY" ]]; then
        local word="${POSTDISPLAY%% *}"
        if [[ "$word" == "$POSTDISPLAY" ]]; then
            BUFFER="${BUFFER}${POSTDISPLAY}"
            CURSOR=${#BUFFER}
            POSTDISPLAY=""
        else
            BUFFER="${BUFFER}${word} "
            CURSOR=${#BUFFER}
            _zsh_turbo_after_modify
        fi
    else
        zle forward-word
    fi
}

function _zsh_turbo_clear_suggestion() {
    POSTDISPLAY=""
    _ZSH_TURBO_SUGGESTION=""
}

# Register widgets
zle -N _zsh_turbo_accept_suggestion
zle -N _zsh_turbo_accept_word
zle -N _zsh_turbo_clear_suggestion

# Key bindings
bindkey '^[[C'    _zsh_turbo_accept_suggestion  # Right arrow
bindkey '^[OC'    _zsh_turbo_accept_suggestion  # Right arrow (alt)
bindkey '^[f'     _zsh_turbo_accept_word        # Alt+f
bindkey '^[[1;5C' _zsh_turbo_accept_word        # Ctrl+Right

# Wrap standard editing widgets to trigger suggestions
function _zsh_turbo_self_insert() {
    zle .self-insert
    _zsh_turbo_after_modify
}

function _zsh_turbo_backward_delete_char() {
    zle .backward-delete-char
    _zsh_turbo_after_modify
}

function _zsh_turbo_backward_kill_word() {
    zle .backward-kill-word
    _zsh_turbo_after_modify
}

function _zsh_turbo_kill_line() {
    zle .kill-whole-line
    _zsh_turbo_clear_suggestion
}

function _zsh_turbo_yank() {
    zle .yank
    _zsh_turbo_after_modify
}

zle -N self-insert _zsh_turbo_self_insert
zle -N backward-delete-char _zsh_turbo_backward_delete_char
zle -N backward-kill-word _zsh_turbo_backward_kill_word
zle -N kill-whole-line _zsh_turbo_kill_line
zle -N yank _zsh_turbo_yank

# ─── Syntax Highlighting (Rust-powered) ──────────────────────────

typeset -g _ZSH_TURBO_HIGHLIGHT_ALIASES=""
typeset -g _ZSH_TURBO_HIGHLIGHT_FUNCTIONS=""
typeset -gi _ZSH_TURBO_HIGHLIGHT_FD=0

# Cache aliases/functions list once at init (refreshed on precmd)
function _zsh_turbo_refresh_shell_objects() {
    _ZSH_TURBO_HIGHLIGHT_ALIASES="${(j:\n:)${(k)aliases}}"
    _ZSH_TURBO_HIGHLIGHT_FUNCTIONS="${(j:\n:)${(k)functions}}"
}
_zsh_turbo_refresh_shell_objects
add-zsh-hook precmd _zsh_turbo_refresh_shell_objects

function _zsh_turbo_highlight() {
    [[ -z "$BUFFER" ]] && { region_highlight=(); return; }

    # Cancel previous async highlight
    if (( _ZSH_TURBO_HIGHLIGHT_FD > 0 )); then
        zle -F "$_ZSH_TURBO_HIGHLIGHT_FD" 2>/dev/null
        exec {_ZSH_TURBO_HIGHLIGHT_FD}<&- 2>/dev/null
        _ZSH_TURBO_HIGHLIGHT_FD=0
    fi

    # Launch async highlight
    exec {_ZSH_TURBO_HIGHLIGHT_FD}< <(
        "$ZSH_TURBO_CMD" highlight "$BUFFER" \
            --aliases "$_ZSH_TURBO_HIGHLIGHT_ALIASES" \
            --functions "$_ZSH_TURBO_HIGHLIGHT_FUNCTIONS" 2>/dev/null
        printf '\0'
    )
    zle -F "$_ZSH_TURBO_HIGHLIGHT_FD" _zsh_turbo_highlight_callback
}

function _zsh_turbo_highlight_callback() {
    local fd="$1"
    local -a new_highlights
    local line

    while IFS= read -r -u "$fd" line 2>/dev/null; do
        [[ -n "$line" ]] && new_highlights+=("$line")
    done

    zle -F "$fd" 2>/dev/null
    exec {fd}<&- 2>/dev/null
    _ZSH_TURBO_HIGHLIGHT_FD=0

    if (( ${#new_highlights} > 0 )); then
        # Preserve suggestion highlight (last entry), replace syntax highlights
        local -a suggest_hl
        local entry
        for entry in "${region_highlight[@]}"; do
            # Keep entries beyond buffer length (suggestion highlights)
            local start="${entry%% *}"
            if (( start >= ${#BUFFER} )); then
                suggest_hl+=("$entry")
            fi
        done
        region_highlight=("${new_highlights[@]}" "${suggest_hl[@]}")
        zle -R
    fi
}

# Hook into editing widgets to trigger highlighting
function _zsh_turbo_after_modify() {
    _zsh_turbo_autosuggest_fetch
    _zsh_turbo_autosuggest_display
    _zsh_turbo_highlight
}

# ─── Enhanced Completion ─────────────────────────────────────────

autoload -Uz compinit

if [[ -z "$ZSH_COMPDUMP" ]]; then
    ZSH_COMPDUMP="${ZDOTDIR:-$HOME}/.zcompdump-zsh-turbo"
fi

# Rebuild comp dump at most once per day
if [[ -f "$ZSH_COMPDUMP" ]] && [[ $(date +'%j') == $(date -r "$ZSH_COMPDUMP" +'%j' 2>/dev/null) ]]; then
    compinit -C -d "$ZSH_COMPDUMP"
else
    compinit -d "$ZSH_COMPDUMP"
fi

# Completion styling
zstyle ':completion:*' menu select
zstyle ':completion:*' matcher-list \
    'm:{a-zA-Z}={A-Za-z}' \
    'r:|=*' \
    'l:|=* r:|=*'
zstyle ':completion:*' list-colors "${(s.:.)LS_COLORS}"
zstyle ':completion:*' group-name ''
zstyle ':completion:*:descriptions' format '%F{blue}── %d ──%f'
zstyle ':completion:*:warnings' format '%F{red}no matches found%f'
zstyle ':completion:*:corrections' format '%F{yellow}%d (errors: %e)%f'
zstyle ':completion:*' squeeze-slashes true
zstyle ':completion:*' complete-options true

# Completion caching
zstyle ':completion:*' use-cache on
zstyle ':completion:*' cache-path "${XDG_CACHE_HOME:-$HOME/.cache}/zsh-turbo/completions"

# Directory completion
zstyle ':completion:*' special-dirs true
zstyle ':completion:*:cd:*' tag-order local-directories directory-stack path-directories

# Process completion
zstyle ':completion:*:*:kill:*' menu yes select
zstyle ':completion:*:kill:*' force-list always
zstyle ':completion:*:*:kill:*:processes' list-colors '=(#b) #([0-9]#)*=0=01;31'

# ─── Key Bindings ────────────────────────────────────────────────

# History search with arrow keys
autoload -Uz up-line-or-beginning-search down-line-or-beginning-search
zle -N up-line-or-beginning-search
zle -N down-line-or-beginning-search
bindkey '^[[A' up-line-or-beginning-search    # Up
bindkey '^[OA' up-line-or-beginning-search    # Up (alt)
bindkey '^[[B' down-line-or-beginning-search  # Down
bindkey '^[OB' down-line-or-beginning-search  # Down (alt)

# Word navigation
bindkey '^[[1;5D' backward-word  # Ctrl+Left
bindkey '^[b'     backward-word  # Alt+b
bindkey '^A'      beginning-of-line
bindkey '^E'      end-of-line

# ─── History Settings ────────────────────────────────────────────

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

# ─── Initialization Complete ─────────────────────────────────────

_zsh_turbo_precmd
