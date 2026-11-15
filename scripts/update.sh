#!/usr/bin/env sh
set -eu

echo "Updating Rust"
rustup update

echo "Updating cargo tools"
command -v cargo-install-update >/dev/null || cargo install cargo-update
cargo install-update -a

echo "Updating dependencies"
cargo update

scripts/ci.sh

echo "Running cargo audit"
command -v cargo-audit >/dev/null || cargo install cargo-audit
cargo audit
