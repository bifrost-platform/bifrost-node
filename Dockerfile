# build stage: where we create binary
FROM rust:1.77 AS builder

RUN apt update && apt install -y make clang pkg-config libssl-dev protobuf-compiler
RUN rustup default stable && \
  rustup update && \
  rustup update nightly && \
  rustup target add wasm32-unknown-unknown --toolchain nightly

WORKDIR /bifrost
ENV CARGO_HOME=/bifrost/.cargo
COPY . /bifrost
RUN cargo build --release

# 2nd stage: where we run bifrost-node binary
FROM ubuntu:22.04

RUN apt update && apt install -y curl unzip jq

RUN curl -fsSL https://fnm.vercel.app/install | bash -s -- --install-dir "/root/.fnm"
RUN /root/.fnm/fnm install 16.19.1

COPY --from=builder /bifrost/target/release/bifrost-node /usr/local/bin
COPY --from=builder /bifrost/tools /tools
COPY --from=builder /bifrost/specs /specs

RUN mkdir -p /data /bifrost/.local/share/bifrost && \
  ln -s /data /bifrost/.local/share/bifrost && \
  /usr/local/bin/bifrost-node --version

# 30333 for p2p
# 9933 for RPC call
# 9944 for Websocket
# 9615 for Prometheus exporter
EXPOSE 30333 9933 9944 9615

VOLUME ["/data"]

ENTRYPOINT ["/usr/local/bin/bifrost-node"]
