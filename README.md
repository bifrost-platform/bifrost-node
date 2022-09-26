# Tools for BIFROST Network

This repository contains a set of tools designed for BIFROST network.

It is written in typescript, using `yargs` as interactive CLI tool.

## Public RPC/Websocket Endpoints

### BIFROST Network Testnet (ChainID: 49088)
- https://public-01.testnet.thebifrost.io/rpc
- https://public-02.testnet.thebifrost.io/rpc
- wss://public-01.testnet.thebifrost.io/ws
- wss://public-02.testnet.thebifrost.io/ws

## CLI Tools

### 1. Generate your required validator accounts

**Full Nodes**

```
npm run create_accounts -- --full
```

**Basic Nodes**

```
npm run create_accounts
```

### 2. Set your node session keys

```
npm run set_session_keys -- \
  --controllerPrivate 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133
```

### 3. Self bond your initial stake and join as a Validator

**Full Node**

```
npm run join_validators -- \
  --controllerPrivate 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133 \
  --stashPrivate 0x234871e7f7520af0cfc9f8547057b283c628be93a90b393aa19be1279ee52b4a \
  --relayerPrivate 0xcc01ee486e8717dc3911ede9293b767e29ce66f5c987da45887cb61822700117 \
  --bond 1000
```

**Basic Node**

```
npm run join_validators -- \
  --controllerPrivate 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133 \
  --stashPrivate 0x234871e7f7520af0cfc9f8547057b283c628be93a90b393aa19be1279ee52b4a \
  --bond 1000
```
