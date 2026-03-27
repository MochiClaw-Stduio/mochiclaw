# Build native crates (core, cli, sdk)
build-native:
    cargo build --release --package mochiclaw-cli

# Build WASM plugins
build-plugin:
    cargo build --release --target wasm32-unknown-unknown -p mochiclaw-openai -p mochiclaw-weixin

# Copy built plugins to ./target/plugins directory
setup-plugins:
    #!/bin/bash
    set -e
    mkdir -p ./target/plugins
    for plugin_dir in plugins/mochiclaw-*/; do
        plugin_name=$(basename "$plugin_dir")
        target_dir="./target/plugins/$plugin_name"
        mkdir -p "$target_dir"
        if [ -f "$plugin_dir/manifest.toml" ]; then
            cp "$plugin_dir/manifest.toml" "$target_dir/manifest.toml"
        fi
        wasm_file="target/wasm32-unknown-unknown/release/${plugin_name//-/_}.wasm"
        if [ -f "$wasm_file" ]; then
            cp "$wasm_file" "$target_dir/${plugin_name}.wasm"
        fi
    done

# Build everything
build: build-native build-plugin setup-plugins
