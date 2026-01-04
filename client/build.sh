#!/bin/bash
PROFILE=${1:-release-wasm}

rm -rf pkg

if [ "$PROFILE" = "release-wasm" ]; then
    cargo build --profile release-wasm --target wasm32-unknown-unknown
    wasm-bindgen --out-dir pkg --target web ../target/wasm32-unknown-unknown/release-wasm/c6ol-client.wasm
    ./bin/wasm-opt -Oz --enable-nontrapping-float-to-int --enable-bulk-memory-opt -o pkg/c6ol-client_bg.wasm pkg/c6ol-client_bg.wasm
elif [ "$PROFILE" = "dev" ]; then
    cargo build --profile dev --target wasm32-unknown-unknown
    wasm-bindgen --out-dir pkg --target web ../target/wasm32-unknown-unknown/debug/c6ol-client.wasm
else
    echo "Unknown profile: $PROFILE"
    exit 1
fi
