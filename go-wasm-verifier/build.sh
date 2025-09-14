#!/bin/bash

# Build script for Go WASM verifier
echo "Building Go WASM verifier..."

# Set GOOS and GOARCH for WASM
export GOOS=js
export GOARCH=wasm

# Build the WASM file
go build -o main.wasm main.go

# Copy wasm_exec.js if it doesn't exist
if [ ! -f "wasm_exec.js" ]; then
    echo "Copying wasm_exec.js..."
    GOROOT=$(go env GOROOT)
    if [ -f "$GOROOT/lib/wasm/wasm_exec.js" ]; then
        cp "$GOROOT/lib/wasm/wasm_exec.js" .
        echo "wasm_exec.js copied successfully"
    else
        echo "Warning: wasm_exec.js not found in $GOROOT/lib/wasm/"
        echo "Please copy it manually from your Go installation"
    fi
fi

# Copy files to docs public folder
echo "Copying files to docs public folder..."
mkdir -p ../docs/public/wasm
cp main.wasm ../docs/public/wasm/
cp wasm_exec.js ../docs/public/wasm/
echo "Files copied to docs/public/wasm/"

echo "Build complete! Output: main.wasm and wasm_exec.js in docs/public/wasm/"
