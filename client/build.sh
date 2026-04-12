#!/bin/bash
PROFILE=${1:-release-wasm}
WASM_BINDGEN_VERSION=$(grep -A1 'name = "wasm-bindgen"' ../Cargo.lock | grep -oP '(?<=version = ")\S+(?=")')
BINARYEN_VERSION="129"

ensure_wasm_bindgen() {
    local current_version
    if [ -x "./bin/wasm-bindgen" ]; then
        current_version=$(./bin/wasm-bindgen --version 2>&1 | grep -oP '(?<=wasm-bindgen )\S+')
    fi
    if [ "$current_version" = "$WASM_BINDGEN_VERSION" ]; then
        return
    fi

    echo "Downloading wasm-bindgen version $WASM_BINDGEN_VERSION..."
    local arch
    arch=$(uname -m)
    case "$arch" in
        x86_64)  arch="x86_64-unknown-linux-musl" ;;
        aarch64) arch="aarch64-unknown-linux-musl" ;;
        *)       echo "Unsupported architecture: $arch"; exit 1 ;;
    esac

    local name="wasm-bindgen-${WASM_BINDGEN_VERSION}-${arch}"
    local url="https://github.com/wasm-bindgen/wasm-bindgen/releases/download/${WASM_BINDGEN_VERSION}/${name}.tar.gz"
    curl -fL "$url" | tar -xz --strip-components=1 -C bin "${name}/wasm-bindgen"
    echo "wasm-bindgen version $WASM_BINDGEN_VERSION installed."
}

ensure_wasm_opt() {
    local current_version
    if [ -x "./bin/wasm-opt" ]; then
        current_version=$(./bin/wasm-opt --version 2>&1 | grep -oP '(?<=version )\d+')
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

    local name="binaryen-version_${BINARYEN_VERSION}"
    local url="https://github.com/WebAssembly/binaryen/releases/download/version_${BINARYEN_VERSION}/${name}-${arch}.tar.gz"
    curl -fL "$url" | tar -xz --strip-components=1 "${name}/bin/wasm-opt"
    echo "wasm-opt version $BINARYEN_VERSION installed."
}

mkdir -p bin pkg
ensure_wasm_bindgen

if [ "$PROFILE" = "release-wasm" ]; then
    ensure_wasm_opt
    cargo build --profile release-wasm --target wasm32-unknown-unknown
    ./bin/wasm-bindgen --out-dir pkg --target web "../target/wasm32-unknown-unknown/release-wasm/c6ol-client.wasm"
    ./bin/wasm-opt -Oz --enable-nontrapping-float-to-int --enable-bulk-memory-opt -o pkg/c6ol-client_bg.wasm pkg/c6ol-client_bg.wasm
elif [ "$PROFILE" = "dev" ]; then
    cargo build --profile dev --target wasm32-unknown-unknown
    ./bin/wasm-bindgen --out-dir pkg --target web "../target/wasm32-unknown-unknown/debug/c6ol-client.wasm"
else
    echo "Unknown profile: $PROFILE"
    exit 1
fi
