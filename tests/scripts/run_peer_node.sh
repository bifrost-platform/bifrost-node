#!/usr/bin/env bash

# run a new peer node with the given arguments
../target/release/bifrost-node \
  --base-path $5 \
  --chain ../specs/customSpecRaw.json \
  --port $2 \
  --ws-port $3 \
  --rpc-port $4 \
  --validator \
  --rpc-methods Unsafe \
  --rpc-cors all \
  --rpc-external \
  --ws-external \
  --bootnodes $6 \
  --runtime-cache-size 64 \
  --name $1
