#!/bin/bash

# volumes: ./tests/docker_data:/data

sleep 5;

# 해당 디렉터리가 이미 존재하는지 확인
if [ -d "$DIR" ]; then
    # 디렉터리가 존재하면 삭제
    rm -rf $DIR
fi

#Step1. 체인 데이터를 저장할 디렉터리 생성
mkdir -p  $DIR

#Step 2. 제네시스 노드 bootnodes 가지고오기
# export BOOT_NODE_IP=$(hostname -i)
export BOOT_NODE_IP=$(getent hosts boot_node | awk '{ print $1 }')
export NETWORK_IDENTIFIER=$(curl boot_node:${GENESIS_RPC_PORT} -H "Content-Type:application/json;charset=utf-8" -d '{  "jsonrpc":"2.0", "id":1, "method":"system_localPeerId", "params": []}' | jq -r ".result")

# run a new peer node with the given arguments
bifrost-node \
  --base-path $DIR \
  --chain $CHAIN_DIR \
  --port $PEER_P2P_PORT \
  --rpc-port $PEER_RPC_PORT \
  --validator \
  --rpc-methods Unsafe \
  --rpc-cors all \
  --rpc-external \
  --bootnodes /ip4/${BOOT_NODE_IP}/tcp/${GENESIS_P2P_PORT}/p2p/${NETWORK_IDENTIFIER} \
  --runtime-cache-size 64 \
  --name $NODE_NAME
