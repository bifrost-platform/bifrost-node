#!/bin/bash

# volumes: ./tests/docker_data:/data

# 해당 디렉터리가 이미 존재하는지 확인
if [ -d "$DIR" ]; then
    # 디렉터리가 존재하면 삭제
    rm -rf $DIR
fi

#Step1. 체인 데이터를 저장할 디렉터리 생성
mkdir -p  $DIR

#Step 2. Chain spec JSON 파일 빌드
bifrost-node build-spec --chain $CHAIN --raw --disable-default-bootnode > $CHAIN_DIR

#Step 3. 키 세팅
## insert aura key
bifrost-node key insert --base-path $DIR \
  --chain $CHAIN_DIR \
  --scheme Sr25519 \
  --suri 0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a \
  --key-type aura

## insert grandpa key
bifrost-node key insert --base-path $DIR \
  --chain $CHAIN_DIR \
  --scheme Ed25519 \
  --suri 0xabf8e5bdbe30c65656c0a3cbd181ff8a56294a69dfedd27982aace4a76909115 \
  --key-type gran

## insert imonline key
bifrost-node key insert --base-path $DIR \
  --chain $CHAIN_DIR \
  --scheme Sr25519 \
  --suri 0xe5be9a5092b81bca64be81d212e7f2f9eba183bb7a90954f7b76361f6edb5c0a \
  --key-type imon

## run boot node node
bifrost-node \
  --base-path $DIR \
  --chain $CHAIN_DIR \
  --port $GENESIS_P2P_PORT \
  --rpc-port $GENESIS_RPC_PORT \
  --validator \
  --rpc-methods Unsafe \
  --rpc-cors all \
  --rpc-external \
  --ethapi debug,trace,txpool \
  --runtime-cache-size 64 \
  --name $NODE_NAME