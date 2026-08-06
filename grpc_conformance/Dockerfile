FROM lukemathwalker/cargo-chef:0.1.77-rust-1.97.1-slim-bookworm AS chef
RUN apt-get update \
    && apt-get install --yes --no-install-recommends \
        build-essential \
        cmake \
        git \
        libssl-dev \
        pkg-config \
        protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin rsnano --features grpc
RUN cargo build --release --bin grpc-conformance

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install --yes --no-install-recommends libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --create-home nanocurrency
COPY --from=builder /app/target/release/rsnano /usr/bin/rsnano
COPY --from=builder /app/target/release/grpc-conformance /usr/bin/grpc-conformance
COPY grpc_conformance/container/config/ /data/
RUN chown -R nanocurrency:nanocurrency /data
USER nanocurrency
WORKDIR /data
CMD ["/usr/bin/rsnano", "--network=dev", "--data-path=/data", "node", "run"]
