zpty -b fixture zsh -di || exit 1
{
    repeat 200; do
        drain_fixture
        [[ -s "$TEST_ROOT/boot" && -f "$XDG_CACHE_HOME/zsh-turbo/managed-completions/demofile.zsh" ]] && break
        zselect -t 5
    done
    zpty -w -n fixture $'demohelp --bef\t'
    await_fixture buffer 'demohelp --before ' || exit 2
    zpty -w -n fixture $'\x15demohelp remote --trans\t'
    await_fixture buffer 'demohelp remote --transport=' || exit 3
    zpty -w -n fixture $'\x15demogen --nat\t'
    await_fixture buffer 'demogen --native ' || exit 4
    zpty -w -n fixture $'\x15demofile --file-o\t'
    await_fixture buffer 'demofile --file-option ' || exit 5
    print -r -- $'#!/bin/sh\nprintf \'Options:\\n  --after  After update\\n\'\n' > "$TEST_ROOT/bin/demohelp"
    zpty -w -n fixture $'\x15_ZSH_TURBO_COMPLETION_CHECKED=0\r'
    repeat 200; do
        drain_fixture
        [[ "$(<"$XDG_CACHE_HOME/zsh-turbo/managed-completions/demohelp.zsh")" == *--after* ]] && break
        zselect -t 5
    done
    zpty -w -n fixture $'demohelp --aft\t'
    await_fixture buffer 'demohelp --after ' || exit 6
    zpty -w -n fixture $'\x15exit\r'
    print 'MANAGED COMPLETION OK'
} always {
    zpty -d fixture 2>/dev/null
}
