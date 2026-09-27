# 補完時はキャッシュの inode だけを確認し、変更時だけ定義を読み直す。
zmodload zsh/stat 2>/dev/null
typeset -gA _ZSH_TURBO_COMPLETION_FALLBACK _ZSH_TURBO_COMPLETION_VERSION
typeset -gA _ZSH_TURBO_COMPLETION_DELEGATE _ZSH_TURBO_COMPLETION_FUNCTIONS
function _zsh_turbo_registered_completion() {
    emulate -L zsh
    setopt extendedglob ${_comp_options[@]}
    local target=${words[1]:t}
    local cached="$_ZSH_TURBO_COMPLETION_CACHE/$target.zsh"
    local delegate version name
    local -A info before
    local -a added
    local -i result=1 reloaded=0
    if [[ -r "$cached" ]]; then
        zstat -H info "$cached" 2>/dev/null
        version="${info[inode]:-0}:${info[mtime]:-0}:${info[size]:-0}"
        {
            if [[ "${_ZSH_TURBO_COMPLETION_VERSION[$target]:-}" != "$version" ]]; then
                for name in ${(f)_ZSH_TURBO_COMPLETION_FUNCTIONS[$target]:-}; do
                    unfunction "$name" 2>/dev/null
                done
                for name in ${(k)functions}; do before[$name]=1; done
                reloaded=1
                source "$cached"
                result=$?
                delegate=${_comps[$target]:-}
                _ZSH_TURBO_COMPLETION_DELEGATE[$target]=$delegate
                _ZSH_TURBO_COMPLETION_VERSION[$target]=$version
            else
                delegate=${_ZSH_TURBO_COMPLETION_DELEGATE[$target]:-}
            fi
            if [[ -n "$delegate" && "$delegate" != _zsh_turbo_registered_completion ]]; then
                "$delegate" "$@"
                result=$?
            elif (( ! reloaded )); then
                # autoload 用の本体だけを出力する生成コマンドにも対応する。
                source "$cached"
                result=$?
            fi
        } always {
            if (( reloaded )); then
                for name in ${(k)functions}; do
                    (( ${+before[$name]} )) || added+=("$name")
                done
                _ZSH_TURBO_COMPLETION_FUNCTIONS[$target]=${(F)added}
            fi
            compdef _zsh_turbo_registered_completion "$target"
        }
        return $result
    fi
    delegate=${_ZSH_TURBO_COMPLETION_FALLBACK[$target]:-}
    if [[ -n "$delegate" && "$delegate" != _zsh_turbo_registered_completion ]]; then
        "$delegate" "$@"
    else
        _default
    fi
}

function _zsh_turbo_refresh_completions() {
    emulate -L zsh
    local -i now=${EPOCHSECONDS:-0}
    (( now - ${_ZSH_TURBO_COMPLETION_CHECKED:-0} >= 30 )) || return 0
    typeset -g _ZSH_TURBO_COMPLETION_CHECKED=$now
    ( "$ZSH_TURBO_CMD" completion-refresh </dev/null >/dev/null 2>&1 ) &!
}

() {
    emulate -L zsh
    local target
    for target in "${_ZSH_TURBO_COMPLETION_COMMANDS[@]}"; do
        _ZSH_TURBO_COMPLETION_FALLBACK[$target]=${_comps[$target]:-}
        compdef _zsh_turbo_registered_completion "$target"
    done
}
add-zsh-hook precmd _zsh_turbo_refresh_completions
_zsh_turbo_refresh_completions
