#!/usr/bin/env bash

# run a new boot node with the given arguments
../target/release/bifrost-node \
  --base-path $4 \
  --chain ../specs/customSpecRaw.json \
  --port $2 \
  --rpc-port $3 \
  --validator \
  --rpc-methods Unsafe \
  --rpc-cors all \
  --rpc-external \
  --ethapi debug,trace,txpool \
  --runtime-cache-size 64 \
  --name Boot$1
