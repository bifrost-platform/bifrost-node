#!/usr/bin/env bash

# build mainnet raw chain spec
./target/release/bifrost-node build-spec --chain private-mainnet-hub-local --raw --disable-default-bootnode > ./specs/bifrost-private-mainnet-hub.json
