#!/usr/bin/env bash

# run a new boot node with the given arguments
../target/release/bifrost-node \
  --base-path $4 \
  --chain ../specs/customSpecRaw.json \
  --port $2 \
  --rpc-port $3 \
  --validator \
  --state-pruning archive \
  --rpc-methods Unsafe \
  --rpc-cors all \
  --rpc-external \
  --ethapi debug,trace,txpool \
  --trie-cache-size 0 \
  --runtime-cache-size 64 \
  --name Boot$1
  # explicitly set bootnode when discovery does not work
  #--bootnodes /ip4/127.0.0.1/tcp/30333/p2p/<node-id>