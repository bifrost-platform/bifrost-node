#!/usr/bin/env bash

# build private testnet hub raw chain spec
./target/release/bifrost-node build-spec --chain private-testnet-hub-local --raw --disable-default-bootnode > ./specs/bifrost-private-testnet-hub.json
