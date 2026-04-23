# List available recipes
default:
    @just --list

# Install the host CLIs (`zacor`, fat `zr`, and `zred`) and packages
# as wasm (default). Some packages may not yet port; build failures
# are skipped with a warning. Use a single package argument to
# install just one (e.g. just install echo).
install *pkg:
    @if [ -z "{{pkg}}" ]; then \
        cargo install --path crates/zacor; \
        cargo install --path crates/zr; \
        cargo install --path crates/zred; \
        for p in packages/*/; do \
            name=$(basename "$p"); \
            if cargo build --release --target wasm32-wasip1 -p "zr-${name}" 2>/dev/null; then \
                wasm="target/wasm32-wasip1/release/${name}.wasm"; \
                if [ -f "$wasm" ]; then \
                    zacor install "$wasm" --force; \
                fi; \
            else \
                echo "skipped zr-${name} (wasm build failed — not yet portable)"; \
            fi; \
        done; \
    else \
        cargo build --release --target wasm32-wasip1 -p "zr-{{pkg}}"; \
        zacor install "target/wasm32-wasip1/release/{{pkg}}.wasm" --force; \
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
            zacor install "$p" --force; \
        done; \
    else \
        if [ -f "packages/{{pkg}}/Cargo.toml" ]; then \
            cargo build --release -p "zr-{{pkg}}"; \
        fi; \
        zacor install "packages/{{pkg}}" --force; \
    fi

# Build packages as wasm32-wasip1 (host crates are excluded — they have
# native-only deps). Build failures are surfaced.
build-wasm *pkg:
    @if [ -z "{{pkg}}" ]; then \
        for p in packages/*/; do \
            name=$(basename "$p"); \
            cargo build --release --target wasm32-wasip1 -p "zr-${name}" || \
                echo "FAIL: zr-${name}"; \
        done; \
    else \
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
