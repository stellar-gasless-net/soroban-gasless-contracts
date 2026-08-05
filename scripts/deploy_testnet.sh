#!/bin/bash
# Soroban Testnet Deployment Script for Gasless Smart Contracts

set -e

echo "🚀 Building and optimizing Soroban WASM smart contracts..."
cargo build --target wasm32-unknown-unknown --release

echo "✨ Soroban WASM artifacts compiled successfully!"
ls -lh target/wasm32-unknown-unknown/release/*.wasm
