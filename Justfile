# Build native crates (core, cli, sdk)
build-native:
    cargo build --release --package mochiclaw-cli

# Build WASM plugins
build-plugin:
    cargo build --release --target wasm32-unknown-unknown -p mochi-openai -p mochi-weixin -p mochi-fs

# Build test plugins (for integration tests)
build-test-plugin:
    cargo build --release --target wasm32-unknown-unknown -p test-kv -p test-random -p test-fs

# Test native crates (depends on test plugin WASM being built)
test-native: build-test-plugin
    cargo test -p mochiclaw-sdk -p mochiclaw-lambda -p mochiclaw-core

# Test everything
test: test-native

# Copy built plugins to ./target/plugins directory
setup-plugins:
    #!/bin/bash
    set -e
    mkdir -p ./target/plugins
    for plugin_dir in lambdas/*/; do
        if [ ! -f "$plugin_dir/manifest.toml" ]; then
            continue
        fi
        # Read plugin name from manifest.toml
        manifest_name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$plugin_dir/manifest.toml" | tr -d ' \r')
        if [ -z "$manifest_name" ]; then
            echo "Warning: could not read name from $plugin_dir/manifest.toml"
            continue
        fi
        target_dir="./target/plugins/$manifest_name"
        mkdir -p "$target_dir"
        cp "$plugin_dir/manifest.toml" "$target_dir/manifest.toml"
        # wasm filename: replace - with _ (mochi-weixin -> mochi_weixin)
        wasm_name="${manifest_name//-/_}"
        wasm_file="target/wasm32-unknown-unknown/release/${wasm_name}.wasm"
        if [ -f "$wasm_file" ]; then
            cp "$wasm_file" "$target_dir/${manifest_name}.wasm"
        fi
    done

# Build everything
build: build-native build-plugin setup-plugins

# Format code
fmt:
    cargo fmt

# Lint code
lint: build-test-plugin
    cargo clippy --all-targets --all-features -- -D warnings
