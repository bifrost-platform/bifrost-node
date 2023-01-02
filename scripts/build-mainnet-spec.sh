#!/usr/bin/env bash

# build mainnet raw chain spec
./target/release/bifrost-node build-spec --chain mainnet-local --raw --disable-default-bootnode > ./specs/bifrost-mainnet.json
