# Tools for Bifrost Network

This repository contains a set of tools designed for Bifrost network.

It is written in typescript, using `yargs` as interactive CLI tool.

# CLI Tools

## 1. Node Setup

### 1.1. Generate your required validator accounts

**Full Nodes**

```
npm run create_accounts -- --full
```

**Basic Nodes**

```
npm run create_accounts
```

### 1.2. Set your node session keys

```
npm run set_session_keys -- \
  --controllerPrivate 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133
```

### 1.3. Self bond your initial stake and join as a Validator

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

## 2. Data Query

### 1.1. Query Extrinsics
This command will query historical extrinsics that matches the requested parameters.

**Query Parameters**
|Name      |Description                                     |Example                                                         |Required                                                         |
|----------|--------------------------------------------|--------------------------------------------------------------------|------------------------------------------------------|
|from        |The account address who submitted the extrinsic.  |0x81143D1d29B101B84FE87BCB2f684534b20EBaAd  |X|
|start       |The starting block number where query will start.  |5047917  |O|
|end         |The ending block number where the query will end. The default value will be the highest block of the connected provider.  |5048007  |X|
|provider    |The provider URL. The default value will be `http://localhost:9933`  |`https://public-01.testnet.bifrostnetwork.com/rpc`  |O|
|pallet      |The name of the pallet where the extrinsic locates.  |bfcStaking  |O|
|extrinsic   |The name of the extrinsic to query.  |nominate  |O|

**Example Request**
```
npm run query_extrinsics -- \
  --from 0x81143D1d29B101B84FE87BCB2f684534b20EBaAd \
  --start 5047917 \
  --end 5048007 \
  --provider https://public-01.testnet.bifrostnetwork.com/rpc \
  --pallet bfcStaking \
  --extrinsic nominate
```

**Example Response**
```
✨ Found extrinsics in block #5047917
     🔖 Extrinsic #5047917-1 hash(0x38aed5995f65f9fa49c4387ad7c5302b6c7e27443ca986e7c7861d429a34665b)
```

### 1.2. Query Events
This command will query historical events that matches the requested parameters.

**Query Parameters**
|Name      |Description                                     |Example                                                         |Required                                                         |
|----------|--------------------------------------------|--------------------------------------------------------------------|------------------------------------------------------|
|start       |The starting block number where query will start.  |5047917  |O|
|end         |The ending block number where the query will end. The default value will be the highest block of the connected provider.  |5048007  |X|
|provider    |The provider URL. The default value will be `http://localhost:9933`  |`https://public-01.testnet.bifrostnetwork.com/rpc`  |O|
|pallet      |The name of the pallet where the event locates.  |bfcStaking  |O|
|event   |The name of the event to query.  |Nomination  |O|

**Example Request**
```
npm run query_events -- \
  --start 5047917 \
  --end 5048007 \
  --provider https://public-01.testnet.bifrostnetwork.com/rpc \
  --pallet bfcStaking \
  --event Nomination
```

**Example Response**
```
✨ Found events in block #5047917
     🔖 Event emitted at extrinsic #5047917-1 hash(0x38aed5995f65f9fa49c4387ad7c5302b6c7e27443ca986e7c7861d429a34665b)
```

### 1.3. Query Extrinsic Details
This command will query the detail information of the extrinsic that matches the requested parameters.

**Query Parameters**
|Name      |Description                                     |Example                                                         |Required                                                         |
|----------|--------------------------------------------|--------------------------------------------------------------------|------------------------------------------------------|
|block       |The block number where the extrinsic is included.  |5047917  |O|
|index       |The index of the extrinsic.  |1  |O|
|provider    |The provider URL. The default value will be `http://localhost:9933`  |`https://public-01.testnet.bifrostnetwork.com/rpc`  |O|

**Example Request**
```
npm run query_extrinsic -- \
  --block 5047917 \
  --index 1 \
  --provider https://public-01.testnet.bifrostnetwork.com/rpc
```

**Example Response**
```
🔖 Extrinsic #5047917-1 hash(0x38aed5995f65f9fa49c4387ad7c5302b6c7e27443ca986e7c7861d429a34665b)
     Pallet: bfcStaking
     Extrinsic: nominate
     Signer: 0x81143D1d29B101B84FE87BCB2f684534b20EBaAd
     Arguments:
         candidate: 0x45A96ACA1Cd759306B05B05b40B082254E77699b
         amount: 1,000,000,000,000,000,000,000
         candidate_nomination_count: 100
         nomination_count: 100
     Events:
       #0
           Pallet: balances
           Event: Withdraw
           Data:
               "0x81143D1d29B101B84FE87BCB2f684534b20EBaAd"
               "1,390,000,098,974,000"
       #1
           Pallet: balances
           Event: Reserved
           Data:
               "0x81143D1d29B101B84FE87BCB2f684534b20EBaAd"
               "1,000,000,000,000,000,000,000"
       #2
           Pallet: bfcStaking
           Event: Nomination
           Data:
               "0x81143D1d29B101B84FE87BCB2f684534b20EBaAd"
               "1,000,000,000,000,000,000,000"
               "0x45A96ACA1Cd759306B05B05b40B082254E77699b"
               {"AddedToTop":{"newTotal":"669,047,377,978,039,362,458,861"}}
       #3
           Pallet: balances
           Event: Deposit
           Data:
               "0x6d6f646C70792f74727372790000000000000000"
               "695,000,049,487,000"
       #4
           Pallet: treasury
           Event: Deposit
           Data:
               "695,000,049,487,000"
       #5
           Pallet: transactionPayment
           Event: TransactionFeePaid
           Data:
               "0x81143D1d29B101B84FE87BCB2f684534b20EBaAd"
               "1,390,000,098,974,000"
               "0"
       #6
           Pallet: system
           Event: ExtrinsicSuccess
           Data:
               {"weight":{"refTime":"1,283,734,000","proofSize":"0"},"class":"Normal","paysFee":"Yes"}
```

## 3. Simple Transactions

### 1.1. Simple Transfer
This command will transfer the given value to the receiver.

**Query Parameters**
|Name      |Description                                     |Example                                                         |Required                                                         |
|----------|--------------------------------------------|--------------------------------------------------------------------|------------------------------------------------------|
|fromPrivate        |The private key of the sender.  |0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133  |O|
|to       |The address of the receiver.  |0x81143D1d29B101B84FE87BCB2f684534b20EBaAd  |O|
|provider    |The provider URL. The default value will be `http://localhost:9933`  |`https://public-01.testnet.bifrostnetwork.com/rpc`  |O|
|value      |The amount that will be transferred in wei units.  |1000000000000000000  |O|

**Example Request**
```
npm run transfer -- \
  --fromPrivate 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133 \
  --to 0x81143D1d29B101B84FE87BCB2f684534b20EBaAd \
  --provider https://public-01.testnet.bifrostnetwork.com/rpc \   
  --value 1000000000000000000
```

**Example Response**
```
🎁 Successfully transferred → 0xc186e8606d1d3fa77fe71976da916a7ed67c9014147741f0b25a9e5d09057755
```
