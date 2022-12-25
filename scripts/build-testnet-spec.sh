#!/usr/bin/env bash

# build testnet raw chain spec
./target/release/bifrost-node build-spec --chain testnet-local --raw --disable-default-bootnode > ./specs/bifrost-testnet.json
