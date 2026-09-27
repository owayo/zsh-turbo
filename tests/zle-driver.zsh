emulate -L zsh
zmodload zsh/zpty || exit 1
zmodload zsh/zselect || exit 1
local_output=''
function drain_fixture() {
    while zpty -r fixture local_output 2>/dev/null; do
        print -rn -- "$local_output" >> "$TEST_ROOT/terminal"
    done
}
function snapshot_fixture() {
    : > "$TEST_ROOT/ready"
    zpty -w -n fixture $'\e[24~'
    repeat 100; do
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
    repeat 100; do
        snapshot_fixture || return 1
        actual="$(<"$TEST_ROOT/$field")"
        [[ "$actual" == "$expected" ]] && return 0
        [[ "$mode" == contains && "$actual" == *"$expected"* ]] && return 0
        zselect -t 5
    done
    print -u2 -- "unexpected $field: $(<"$TEST_ROOT/$field") (expected $expected)"
    return 1
}
zpty -b fixture zsh -di || exit 1
{
    repeat 100; do
        drain_fixture
        [[ -s "$TEST_ROOT/boot" ]] && break
        zselect -t 5
    done
    [[ -s "$TEST_ROOT/boot" ]] || exit 2
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
    repeat 100; do
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
    zpty -w -n fixture $'\x15echo sam\x12\e[B'
    repeat 100; do
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
    repeat 100; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 3 ]] && break
        zselect -t 5
    done
    await_fixture buffer '' || exit 24
    [[ "$(<"$TEST_ROOT/terminal")" == *'stderr-visible-25'* ]] || exit 25
    zpty -w -n fixture $'\e[23~echo sam'
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
    zpty -w -n fixture $'\t'
    await_fixture buffer 'make build' || exit 36
    zpty -w -n fixture $'\x15pnpm de'
    await_fixture ghost 'v' || exit 37
    zpty -w -n fixture $'\x15bun run de'
    await_fixture ghost 'v' || exit 38
    zpty -w -n fixture $'\x15uv run he'
    await_fixture ghost 'llo' || exit 39
    zpty -w -n fixture $'\x15npm run de'
    await_fixture ghost 'v' || exit 39
    zpty -w -n fixture $'\x15yarn de'
    await_fixture ghost 'v' || exit 39
    zpty -w -n fixture $'\x15deno task ch'
    await_fixture ghost 'eck' || exit 39
    zpty -w -n fixture $'\x15mise run bu'
    await_fixture ghost 'ild' || exit 39
    zpty -w -n fixture $'\x15just ch'
    await_fixture ghost 'eck' || exit 39
    zpty -w -n fixture $'\x15task bu'
    await_fixture ghost 'ild' || exit 39
    zpty -w -n fixture $'\x15ZSH_TURBO_KEY_TAB=default\r'
    repeat 100; do
        drain_fixture
        [[ "$(<"$TEST_ROOT/boot")" == 4 ]] && break
        zselect -t 5
    done
    [[ "$(<"$TEST_ROOT/boot")" == 4 ]] || exit 40
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
    zpty -w -n fixture $'\x15yarn de\t'
    await_fixture buffer 'yarn dev ' || exit 46
    zpty -w -n fixture $'\x15deno task ch\t'
    await_fixture buffer 'deno task check ' || exit 47
    zpty -w -n fixture $'\x15mise run bu\t'
    await_fixture buffer 'mise run build ' || exit 48
    zpty -w -n fixture $'\x15just ch\t'
    await_fixture buffer 'just check ' || exit 49
    zpty -w -n fixture $'\x15task bu\t'
    await_fixture buffer 'task build ' || exit 50
    zpty -w -n fixture $'\x15exit\r'
    print 'ZLE OK'
} always {
    zpty -d fixture 2>/dev/null
}
