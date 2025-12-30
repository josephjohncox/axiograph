# syntax=docker/dockerfile:1

# Some transitive dependencies use the Rust 2024 edition, so we need a Cargo
# new enough to parse `edition = "2024"` in dependency manifests.
#
# Pin to a known-good toolchain (overrideable via `--build-arg RUST_IMAGE=...`).
ARG RUST_IMAGE=rust:1.88-slim-bookworm

FROM ${RUST_IMAGE} AS builder

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
