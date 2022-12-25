#!/usr/bin/env bash

# build dev raw chain spec
./target/release/bifrost-node build-spec --chain dev --raw --disable-default-bootnode > ./specs/bifrost-dev.json
