#!/bin/bash
# Build script for AEGIS edge function example

set -e

echo "Building AEGIS Edge Function Example..."

# Check if wasm32 target is installed
if ! rustup target list | grep -q "wasm32-unknown-unknown (installed)"; then
    echo "Installing wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
fi

# Build the Wasm module
echo "Compiling to WebAssembly..."
cargo build --release --target wasm32-unknown-unknown

WASM_FILE="target/wasm32-unknown-unknown/release/aegis_edge_function_example.wasm"

# Show file size
if [ -f "$WASM_FILE" ]; then
    SIZE=$(ls -lh "$WASM_FILE" | awk '{print $5}')
    echo "✓ Build successful!"
    echo "  Output: $WASM_FILE"
    echo "  Size: $SIZE"

    # Optional: optimize with wasm-opt if available
    if command -v wasm-opt &> /dev/null; then
        echo ""
        echo "Optimizing with wasm-opt..."
        wasm-opt -Oz -o "${WASM_FILE%.wasm}_optimized.wasm" "$WASM_FILE"
        OPT_SIZE=$(ls -lh "${WASM_FILE%.wasm}_optimized.wasm" | awk '{print $5}')
        echo "✓ Optimized: ${WASM_FILE%.wasm}_optimized.wasm ($OPT_SIZE)"
    fi
else
    echo "✗ Build failed - Wasm file not found"
    exit 1
fi

echo ""
echo "To test the edge function:"
echo "  cd ../node"
echo "  cargo test --test edge_function_test -- --ignored"
