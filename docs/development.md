# Development

[Back to README](../README.md)

`make dist` writes a native build archive to `target/dist/` containing the binary, `LICENSE`, `THIRD_PARTY_NOTICES.md`, and `licenses/`. Distribute these together. After changing dependencies, run `make licenses` and review the generated notices; `make licenses-check` checks that they match the current lockfile. These license tasks require Python 3.7 or newer and Cargo, and may download crates into Cargo's cache. The notices cover default-feature normal/build dependencies across all targets, excluding development-only and inactive optional dependencies.
