#!/usr/bin/env bash

# generate node key
../target/release/bifrost-node key generate-node-key \
  --base-path $1 \
  --chain ../specs/customSpecRaw.json
