#!/bin/sh
### To build, first install wasm-pack, then run this script
# cargo install wasm-pack
set -e
set -x

wasm-pack build --release -t nodejs