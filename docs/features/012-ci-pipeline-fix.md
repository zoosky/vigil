# Feature 012: CI Pipeline Fix

## Problem Statement

The current GitHub Actions CI pipeline has issues:

1. **Invalid action reference**: Uses `dtolnay/rust-action` which doesn't exist (should be `dtolnay/rust-toolchain`)
2. **Unsupported platforms**: Tests on Windows, but Vigil uses macOS-specific commands (`ping -W`, `traceroute`, launchd)
3. **Overly complex**: 6 separate jobs when fewer would suffice
4. **Redundant caching**: Each job has its own cache configuration

## Goals

1. Fix the CI pipeline so it passes
2. Support only macOS (primary) and Ubuntu (for format/lint checks)
3. Simplify the pipeline structure
4. Reduce CI runtime and complexity

## Platform Support

| Platform | Support Level | Rationale |
|----------|--------------|-----------|
| macOS | Full | Primary target, uses native `ping`/`traceroute` and launchd |
| Ubuntu | Partial | Format/lint checks only, tests may fail due to different ping syntax |
| Windows | None | Not supported, incompatible system commands |

## Proposed Pipeline Structure

### Jobs

| Job | Runs On | Purpose |
|-----|---------|---------|
| `check` | ubuntu-latest | Format, clippy, docs (fast feedback) |
| `test` | macos-latest | Full test suite on primary platform |
| `build` | macos-latest | Release build verification |

### Simplified Structure

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  check:
    name: Check (fmt, clippy, docs)
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache
        uses: Swatinem/rust-cache@v2

      - name: Format
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --all-targets -- -D warnings

      - name: Docs
        run: cargo doc --no-deps
        env:
          RUSTDOCFLAGS: -D warnings

  test:
    name: Test (macOS)
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache
        uses: Swatinem/rust-cache@v2

      - name: Test
        run: cargo test --verbose

  build:
    name: Build (macOS)
    runs-on: macos-latest
    needs: [check, test]
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache
        uses: Swatinem/rust-cache@v2

      - name: Build release
        run: cargo build --release
```

## Key Changes

### 1. Fix Action Reference
```diff
- uses: dtolnay/rust-action@stable
+ uses: dtolnay/rust-toolchain@stable
```

### 2. Remove Windows from Matrix
```diff
  matrix:
-   os: [ubuntu-latest, macos-latest, windows-latest]
+   os: [macos-latest]
```

### 3. Use Swatinem/rust-cache
Replace manual cache configuration with the dedicated Rust caching action:
```diff
- - name: Cache cargo registry
-   uses: actions/cache@v4
-   with:
-     path: |
-       ~/.cargo/registry
-       ~/.cargo/git
-       target
-     key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
+ - name: Cache
+   uses: Swatinem/rust-cache@v2
```

### 4. Combine Lint Jobs
Merge `fmt`, `clippy`, and `docs` into a single `check` job to reduce overhead.

### 5. Remove MSRV Check (Optional)
The MSRV check adds CI time. Consider removing if not publishing to crates.io:
- Keep if: Publishing as a library, need to guarantee compatibility
- Remove if: Personal project, always use latest stable

## Implementation

### Phase 1: Fix Critical Issues
1. Change `dtolnay/rust-action` to `dtolnay/rust-toolchain`
2. Remove Windows from test/build matrix

### Phase 2: Simplify
1. Replace manual caching with `Swatinem/rust-cache@v2`
2. Combine format/clippy/docs into single job
3. Remove MSRV job (optional)

### Phase 3: Optimize
1. Add `needs:` dependencies to run build only after tests pass
2. Consider adding concurrency limits to cancel outdated runs

## Testing

1. Push to a feature branch
2. Verify all CI jobs pass
3. Check CI runtime is reasonable (target: < 5 minutes total)

## Future Considerations

### Ubuntu Test Support
If Ubuntu test support is desired later:
- Skip tests that use `ping`/`traceroute` on non-macOS
- Add conditional compilation: `#[cfg(target_os = "macos")]`
- Or mock the system commands in tests

### Release Workflow
Consider adding a separate release workflow that:
- Builds release binaries for macOS (Intel + Apple Silicon)
- Creates GitHub releases with attached binaries
- Generates changelog from commits

## Checklist

- [ ] Fix `dtolnay/rust-action` → `dtolnay/rust-toolchain`
- [ ] Remove Windows from matrix
- [ ] Replace manual cache with `Swatinem/rust-cache@v2`
- [ ] Combine lint jobs into single `check` job
- [ ] Remove or keep MSRV check (decide)
- [ ] Add job dependencies (`needs:`)
- [ ] Verify CI passes on PR
- [ ] Verify CI runtime is acceptable
