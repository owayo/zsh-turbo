# Development

[Back to README](../README.md)

`make dist` writes a native build archive to `target/dist/` containing the binary, `LICENSE`, `THIRD_PARTY_NOTICES.md`, and `licenses/`. Distribute these together. After changing dependencies, run `make licenses` and review the generated notices; `make licenses-check` checks that they match the current lockfile. These license tasks require Python 3.7 or newer and Cargo, and may download crates into Cargo's cache. The notices cover default-feature normal/build dependencies across all targets, excluding development-only and inactive optional dependencies.

## History search benchmark

Build each revision with `make release` and save its binary under a distinct name. Run the comparison with Python 3.7 or newer:

```bash
mise exec -- python3 scripts/bench_history.py /tmp/zsh-turbo-before target/release/zsh-turbo > /tmp/history-bench.json
```

The script creates synthetic zsh histories in a temporary directory and isolates config, cache, and state from your shell. It measures CLI startup, decoding, and ranking together. Each case runs three warmups and 15 measured iterations per binary, alternating binaries. JSON output includes every sample, median, standard deviation, input size, and output hash. Different outputs stop the comparison. Covering 1,000 and 100,000 entries, the cases include prefix, substring, fuzzy, unsuccessful searches, and Japanese text. The larger fixtures exercise an explicit `--history-file`; directory history is capped at 1,000 entries. Compare builds with the same toolchain and dependency lockfile.
