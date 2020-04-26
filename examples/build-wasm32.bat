@echo off
IF "%1"=="" (
    echo "Usage: build-wasm32 <example-name>"
) ELSE (
echo "Build WASM module"
setlocal
set RUSTFLAGS=-C linker=lld
cargo build --examples --target=wasm32-unknown-unknown

echo "Generate bindings"
wasm-bindgen --target web --out-dir "%~dp0\web\generated" --no-typescript "%~dp0\..\..\target\wasm32-unknown-unknown\debug\examples\%1.wasm"
wasm-opt "%~dp0\web\generated\%1_bg.wasm" -o "%~dp0\web\generated\%1.wasm" -O2 --disable-threads

echo "Success"
)
