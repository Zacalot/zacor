# List available recipes
default:
    @just --list

# Download a repo-local WASI SDK toolchain so C-backed wasm packages
# (for example `treesitter`) can build under `wasm32-wasip1`. The SDK is
# cached in `./wasi-sdk/` and ignored by git.
wasi-sdk:
    {{ if os_family() == "windows" { "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/bootstrap-wasi-sdk.ps1" } else { "sh scripts/bootstrap-wasi-sdk.sh" } }}

# Ensure the repo-local WASI SDK exists before building wasm packages that
# compile C code through `cc-rs`.
ensure-wasi-sdk:
    @if [ ! -f "wasi-sdk/bin/clang" ] && [ ! -f "wasi-sdk/bin/clang.exe" ]; then \
        just wasi-sdk; \
    fi

# Install the host CLIs (`zacor`, fat `zr`, and `zred`) and packages
# with a wasm-first default. When a package's wasm build fails, fall
# back to installing the local project natively instead of skipping it.
# Use a single package argument to install just one (e.g. `just install echo`).
install *pkg:
    @if [ -z "{{pkg}}" ]; then \
        just ensure-wasi-sdk; \
        cargo install --path crates/zacor; \
        cargo install --path crates/zr; \
        cargo install --path crates/zred; \
        for p in packages/*/; do \
            name=$(basename "$p"); \
            if [ ! -f "$p/Cargo.toml" ] && [ ! -f "$p/package.yaml" ]; then \
                echo "skipped ${name} (not an installable package directory)"; \
                continue; \
            fi; \
            if cargo build --release --target wasm32-wasip1 -p "zr-${name}" 2>/dev/null; then \
                wasm="target/wasm32-wasip1/release/${name}.wasm"; \
                alt_wasm="target/wasm32-wasip1/release/zr-${name}.wasm"; \
                if [ -f "$wasm" ]; then \
                    zacor install "$wasm" --force; \
                elif [ -f "$alt_wasm" ]; then \
                    zacor install "$alt_wasm" --force; \
                else \
                    echo "skipped zr-${name} (wasm build succeeded but no artifact found)"; \
                fi; \
            else \
                echo "wasm build failed for zr-${name}; falling back to local project install"; \
                cargo build --release -p "zr-${name}"; \
                zacor install "$p" --force; \
            fi; \
        done; \
    else \
        just ensure-wasi-sdk; \
        if [ ! -f "packages/{{pkg}}/Cargo.toml" ] && [ ! -f "packages/{{pkg}}/package.yaml" ]; then \
            echo "packages/{{pkg}} is not an installable package directory"; \
            exit 1; \
        elif cargo build --release --target wasm32-wasip1 -p "zr-{{pkg}}"; then \
            wasm="target/wasm32-wasip1/release/{{pkg}}.wasm"; \
            alt_wasm="target/wasm32-wasip1/release/zr-{{pkg}}.wasm"; \
            if [ -f "$wasm" ]; then \
                zacor install "$wasm" --force; \
            elif [ -f "$alt_wasm" ]; then \
                zacor install "$alt_wasm" --force; \
            else \
                echo "zr-{{pkg}} built but no wasm artifact was found"; \
                exit 1; \
            fi; \
        else \
            echo "wasm build failed for zr-{{pkg}}; falling back to local project install"; \
            cargo build --release -p "zr-{{pkg}}"; \
            zacor install "packages/{{pkg}}" --force; \
        fi; \
    fi

# Install the host CLIs (`zacor`, fat `zr`, and `zred`) and all
# packages as native binaries (legacy path — prefer `just install`
# for the wasm default).
install-native *pkg:
    @if [ -z "{{pkg}}" ]; then \
        cargo install --path crates/zacor; \
        cargo install --path crates/zr; \
        cargo install --path crates/zred; \
        cargo build --release --workspace; \
        for p in packages/*/; do \
            if [ -f "$p/Cargo.toml" ] || [ -f "$p/package.yaml" ]; then \
                zacor install "$p" --force; \
            else \
                echo "skipped $(basename "$p") (not an installable package directory)"; \
            fi; \
        done; \
    else \
        if [ ! -f "packages/{{pkg}}/Cargo.toml" ] && [ ! -f "packages/{{pkg}}/package.yaml" ]; then \
            echo "packages/{{pkg}} is not an installable package directory"; \
            exit 1; \
        fi; \
        if [ -f "packages/{{pkg}}/Cargo.toml" ]; then \
            cargo build --release -p "zr-{{pkg}}"; \
        fi; \
        zacor install "packages/{{pkg}}" --force; \
    fi

# Build packages as wasm32-wasip1 (host crates are excluded — they have
# native-only deps). Build failures are surfaced.
build-wasm *pkg:
    @if [ -z "{{pkg}}" ]; then \
        just ensure-wasi-sdk; \
        for p in packages/*/; do \
            name=$(basename "$p"); \
            if [ ! -f "$p/Cargo.toml" ]; then \
                echo "skipped ${name} (no Cargo.toml)"; \
                continue; \
            fi; \
            cargo build --release --target wasm32-wasip1 -p "zr-${name}" || \
                echo "FAIL: zr-${name} (hint: run `just wasi-sdk` if C-backed packages fail on missing stdlib.h/stdio.h)"; \
        done; \
    else \
        if [ ! -f "packages/{{pkg}}/Cargo.toml" ]; then \
            echo "packages/{{pkg}} has no Cargo.toml"; \
            exit 1; \
        fi; \
        just ensure-wasi-sdk; \
        cargo build --release --target wasm32-wasip1 -p "zr-{{pkg}}"; \
    fi

# Install the thin zr-client — minimal IPC-only dispatch binary that
# talks to a running daemon. 48× smaller than fat zr, ~24% faster
# warm. Requires `zacor daemon start` to be running.
install-client:
    cargo install --path crates/zr-client

# Run all tests, or tests for a specific package (e.g., just test kv)
test *pkg:
    @if [ -z "{{pkg}}" ]; then \
        cargo test --workspace; \
    else \
        cargo test -p "zr-{{pkg}}"; \
    fi

# Check everything compiles
check:
    cargo check --workspace

# Build all crates, or a specific package (e.g., just build kv)
build *pkg:
    @if [ -z "{{pkg}}" ]; then \
        cargo build --release --workspace; \
    else \
        cargo build --release -p "zr-{{pkg}}"; \
    fi
