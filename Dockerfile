# build stage: where we create binary
FROM rust:1.94.0 AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    git clang curl llvm libclang-dev libssl-dev libudev-dev make protobuf-compiler pkg-config \
  && rm -rf /var/lib/apt/lists/*

WORKDIR /bifrost
COPY . .

RUN rustup target add wasm32v1-none --toolchain 1.94.0
RUN cargo build --release --locked -p bifrost-node

FROM node:22-slim AS tools
WORKDIR /tools
COPY tools/package.json tools/package-lock.json ./
RUN npm ci --omit=dev
COPY tools/ ./

# runtime stage: Ubuntu 24.04
FROM ubuntu:24.04

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3t64 libudev1 \
  && rm -rf /var/lib/apt/lists/*

COPY --from=tools /usr/local/ /usr/local/
COPY --from=tools /tools /tools

COPY --from=builder /bifrost/target/release/bifrost-node /usr/local/bin/
COPY --from=builder /bifrost/specs /specs

RUN mkdir -p /data /bifrost/.local/share && \
  ln -s /data /bifrost/.local/share/bifrost && \
  /usr/local/bin/bifrost-node --version

# 30333 for p2p
# 9944 for RPC/Websocket
# 9615 for Prometheus exporter
EXPOSE 30333 9944 9615

VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/bifrost-node"]
