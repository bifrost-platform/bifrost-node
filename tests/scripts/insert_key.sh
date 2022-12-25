#!/usr/bin/env bash

# insert aura session key
curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0",
"method": "author_insertKey", "params":["aura", "'$1'", "'$2'"] }' $5

# insert grandpa session key
curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0",
"method": "author_insertKey", "params":["gran", "'$1'", "'$3'"] }' $5

# insert imonline session key
curl -H "Content-Type: application/json" -d '{"id":1, "jsonrpc":"2.0",
"method": "author_insertKey", "params":["imon", "'$1'", "'$4'"] }' $5
