#!/bin/bash
PROFILE=${1:-release-wasm}

mkdir -p pkg

if [ "$PROFILE" = "release-wasm" ]; then
    cargo build --profile release-wasm --target wasm32-unknown-unknown
    
    WASM_FILE="../target/wasm32-unknown-unknown/release-wasm/c6ol-client.wasm"
    WASM_HASH=$(md5sum "$WASM_FILE" | cut -d' ' -f1)
    echo $WASM_HASH > pkg/src_hash.txt

    wasm-bindgen --out-dir pkg --target web "$WASM_FILE"
    ./bin/wasm-opt -Oz --enable-nontrapping-float-to-int --enable-bulk-memory-opt -o pkg/c6ol-client_bg.wasm pkg/c6ol-client_bg.wasm
elif [ "$PROFILE" = "dev" ]; then
    cargo build --profile dev --target wasm32-unknown-unknown

    WASM_FILE="../target/wasm32-unknown-unknown/debug/c6ol-client.wasm"

    wasm-bindgen --out-dir pkg --target web "$WASM_FILE"
else
    echo "Unknown profile: $PROFILE"
    exit 1
fi
