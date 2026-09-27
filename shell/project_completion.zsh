
function _zsh_turbo_project_completion() {
    emulate -L zsh
    setopt extendedglob ${_comp_options[@]}
    local target="${words[1]:t}" query="" full delegate
    local -a names
    case "$target" in
        make|pnpm|bun|yarn|just|task)
            if (( CURRENT == 2 )); then
                query="$target $PREFIX"
            elif [[ "$target" == pnpm || "$target" == bun || "$target" == yarn ]] && [[ "${words[2]}" == run ]] && (( CURRENT == 3 )); then
                query="$target run $PREFIX"
            fi
            ;;
        uv|npm|deno|mise)
            if (( CURRENT == 3 )); then
                case "$target:${words[2]}" in
                    uv:run|npm:run|npm:run-script|deno:task|mise:run|mise:r)
                        query="$target ${words[2]} $PREFIX"
                        ;;
                esac
            fi
            ;;
    esac
    if [[ -n "$query" ]]; then
        while IFS= read -r full; do
            [[ -n "$full" ]] && names+=("${full##* }")
        done < <("$ZSH_TURBO_CMD" complete --project-only -- "$query" 2>/dev/null)
        if (( ${#names} )); then
            compadd -Q -a names && return 0
        fi
    fi
    delegate="${_ZSH_TURBO_PROJECT_FALLBACK[$target]:-}"
    if [[ -n "$delegate" && "$delegate" != _zsh_turbo_project_completion ]]; then
        "$delegate" "$@"
    else
        _default
    fi
}

typeset -gA _ZSH_TURBO_PROJECT_FALLBACK
() {
    emulate -L zsh
    local target
    for target in make pnpm bun uv npm yarn deno mise just task; do
        _ZSH_TURBO_PROJECT_FALLBACK[$target]="${_comps[$target]:-}"
        compdef _zsh_turbo_project_completion "$target"
    done
}
