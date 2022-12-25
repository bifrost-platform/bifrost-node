#!/usr/bin/env bash

../target/release/bifrost-node \
export-blocks \
--to $2 \
--dev \
--base-path $3 \
--pruning archive \
$4

rm -rf ../data/boot$1/chains/dev/db/*
rm -rf ../data/boot$1/chains/dev/frontier/*

../target/release/bifrost-node \
import-blocks \
--dev \
--base-path $3 \
--pruning archive \
$4
