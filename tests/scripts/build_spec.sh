#!/usr/bin/env bash

# build dev custom chain spec
../target/release/bifrost-node build-spec --disable-default-bootnode --chain dev > ../specs/customSpec.json

# build dev custom raw chain spec
../target/release/bifrost-node build-spec --chain=../specs/customSpec.json --raw --disable-default-bootnode > ../specs/customSpecRaw.json
