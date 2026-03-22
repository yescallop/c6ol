#!/bin/bash
PROFILE=${1:-release-wasm}
BINARYEN_VERSION="128"

ensure_wasm_opt() {
    local current_version
    if [ -x "./bin/wasm-opt" ]; then
        current_version=$(./bin/wasm-opt --version 2>&1 | grep -oP '(?<=version )\d+' | head -1)
    fi
    if [ "$current_version" = "$BINARYEN_VERSION" ]; then
        return
    fi

    echo "Downloading wasm-opt version $BINARYEN_VERSION..."
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  arch="x86_64-linux" ;;
        aarch64) arch="aarch64-linux" ;;
        *)       echo "Unsupported architecture: $arch"; exit 1 ;;
    esac

    local url="https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/binaryen-version_${BINARYEN_VERSION}-${arch}.tar.gz"
    mkdir -p ./bin
    curl -fL "$url" | tar -xz -C ./bin --strip-components=2 "binaryen-version_${BINARYEN_VERSION}/bin/wasm-opt"
    echo "wasm-opt version $BINARYEN_VERSION installed."
}

mkdir -p pkg

if [ "$PROFILE" = "release-wasm" ]; then
    ensure_wasm_opt
    cargo build --profile release-wasm --target wasm32-unknown-unknown
    wasm-bindgen --out-dir pkg --target web "../target/wasm32-unknown-unknown/release-wasm/c6ol-client.wasm"
    ./bin/wasm-opt -Oz --enable-nontrapping-float-to-int --enable-bulk-memory-opt -o pkg/c6ol-client_bg.wasm pkg/c6ol-client_bg.wasm
elif [ "$PROFILE" = "dev" ]; then
    cargo build --profile dev --target wasm32-unknown-unknown
    wasm-bindgen --out-dir pkg --target web "../target/wasm32-unknown-unknown/debug/c6ol-client.wasm"
else
    echo "Unknown profile: $PROFILE"
    exit 1
fi
