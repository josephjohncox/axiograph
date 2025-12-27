# syntax=docker/dockerfile:1

FROM rust:1.75-slim-bookworm AS builder

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        pkg-config \
        libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app/rust

COPY rust/ ./

ENV CARGO_PROFILE_RELEASE_LTO=thin \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_PANIC=abort

RUN cargo build -p axiograph-cli --release

FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -u 10001 -g root axiograph \
    && mkdir -p /data/accepted \
    && chown -R axiograph:root /data

COPY --from=builder /app/rust/target/release/axiograph /usr/local/bin/axiograph

EXPOSE 7878

USER axiograph

ENTRYPOINT ["axiograph"]
CMD ["db", "serve", "--dir", "/data/accepted", "--listen", "0.0.0.0:7878"]
