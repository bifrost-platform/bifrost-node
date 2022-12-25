#!/usr/bin/env bash

# insert aura key
../target/release/bifrost-node key insert --base-path $1 \
  --chain ../specs/customSpecRaw.json \
  --scheme Sr25519 \
  --suri 0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a \
  --key-type aura

# insert grandpa key
../target/release/bifrost-node key insert --base-path $1 \
  --chain ../specs/customSpecRaw.json \
  --scheme Ed25519 \
  --suri 0xabf8e5bdbe30c65656c0a3cbd181ff8a56294a69dfedd27982aace4a76909115 \
  --key-type gran

# insert imonline key
../target/release/bifrost-node key insert --base-path $1 \
  --chain ../specs/customSpecRaw.json \
  --scheme Sr25519 \
  --suri 0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a \
  --key-type imon
