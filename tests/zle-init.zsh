HISTFILE="$TEST_ROOT/history"
fpath=(
  ${(M)fpath:#*/zsh/functions}
  ${(M)fpath:#*/zsh/functions/*}
  ${(M)fpath:#*/zsh/*/functions}
  ${(M)fpath:#*/zsh/*/functions/*}
)
eval "$("$TEST_BINARY" init)"
bindkey -e
bindkey -M emacs '^[[B' down-line-or-beginning-search
bindkey -M emacs '^[OB' down-line-or-beginning-search
function history_redraw_fixture() {
    _zsh_turbo_line_pre_redraw
    print -rn -- "$BUFFER" > "$TEST_ROOT/history-buffer"
}
zle -N zle-line-pre-redraw history_redraw_fixture
function snapshot_fixture() {
    print -rn -- "$BUFFER" > "$TEST_ROOT/buffer"
    print -rn -- "$_ZSH_TURBO_GHOST_SUFFIX" > "$TEST_ROOT/ghost"
    print -rn -- "$POSTDISPLAY" > "$TEST_ROOT/display"
    print -rl -- "${_ZSH_TURBO_PROJECT_CANDIDATES[@]}" > "$TEST_ROOT/project"
    print -rl -- "${region_highlight[@]}" > "$TEST_ROOT/highlight"
    print -rn -- "$CURSOR" > "$TEST_ROOT/cursor"
    print -rn -- "$_ZSH_TURBO_ASYNC_FD $_ZSH_TURBO_HIGHLIGHT_FD" > "$TEST_ROOT/fds"
    print -r -- ready > "$TEST_ROOT/ready"
}
zle -N snapshot_fixture
bindkey '^[[24~' snapshot_fixture
bindkey -M viins '^[[24~' snapshot_fixture
function vi_fixture() {
    bindkey -v
    zle -K viins
}
zle -N vi_fixture
bindkey '^[[23~' vi_fixture
typeset -gi fixture_prompt_count=0
function fixture_prompt_ready() {
    (( fixture_prompt_count += 1 ))
    print -r -- "$fixture_prompt_count" > "$TEST_ROOT/boot"
}
autoload -Uz add-zle-hook-widget
add-zle-hook-widget line-init fixture_prompt_ready
