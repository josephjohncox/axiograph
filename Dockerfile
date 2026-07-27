# syntax=docker/dockerfile:1@sha256:87999aa3d42bdc6bea60565083ee17e86d1f3339802f543c0d03998580f9cb89

# Rust remains the runtime; Lean builds the trusted certificate checker shipped
# in the same image. Override the Rust image only with an explicitly tested
# toolchain.
ARG RUST_IMAGE=rust:1.88.0-slim-bookworm@sha256:38bc5a86d998772d4aec2348656ed21438d20fcdce2795b56ca434cf21430d89
ARG ELAN_COMMIT=464c9d28395000a2a0128e07081e4956d50eced2
ARG ELAN_INIT_SHA256=a620ff1641616222c8d37c54845492004bb84d6877cdbc944dd65c1aa685bf53

FROM ${RUST_IMAGE} AS builder
ARG ELAN_COMMIT
ARG ELAN_INIT_SHA256

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        git \
        libssl-dev \
        pkg-config \
        python3 \
    && rm -rf /var/lib/apt/lists/*

ENV PATH=/root/.elan/bin:${PATH}

RUN curl -fsSL \
        "https://raw.githubusercontent.com/leanprover/elan/${ELAN_COMMIT}/elan-init.sh" \
        -o /tmp/elan-init.sh \
    && echo "${ELAN_INIT_SHA256}  /tmp/elan-init.sh" | sha256sum -c - \
    && sh /tmp/elan-init.sh -y \
    && rm /tmp/elan-init.sh

COPY lean/ /app/lean/
COPY release/fixtures.json /app/release/fixtures.json
COPY fixtures/verification/ /app/fixtures/verification/
COPY scripts/bounded_io.py scripts/bounded_subprocess.py scripts/run_release_fixture_suite.py /app/scripts/
WORKDIR /app/lean
RUN lake exe cache get \
    && lake build axiograph_verify \
    && python3 /app/scripts/run_release_fixture_suite.py \
        --checker /app/lean/.lake/build/bin/axiograph_verify \
        --manifest /app/release/fixtures.json \
        --repo-root /app \
    && checker_sha256="$(sha256sum /app/lean/.lake/build/bin/axiograph_verify)" \
    && printf '%s  axiograph_verify\n' "${checker_sha256%% *}" \
        > /tmp/axiograph_verify.sha256

COPY rust/ /app/rust/
WORKDIR /app/rust
ENV CARGO_PROFILE_RELEASE_LTO=thin \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
    CARGO_PROFILE_RELEASE_PANIC=abort
# The final image has no mutation-authority shortcut. These builder tests exercise
# one authenticated accepted-manifest materialization and one same-repository
# unaccepted-manifest rejection through the public AxiStore contract.
RUN cargo test -p axiograph-store --test materialization \
        store_family_receipt_is_required_and_revalidated --locked \
    && cargo test -p axiograph-store --test materialization \
        same_repository_unaccepted_manifest_anchors_cannot_be_materialized --locked \
    && printf '%s\n' \
        '{' \
        '  "accepted_manifest": "verified",' \
        '  "schema": "axiograph-container-accepted-plane-smoke-v2",' \
        '  "unaccepted_manifest": "rejected"' \
        '}' \
        > /tmp/axiograph-accepted-plane-smoke.json \
    && cargo build -p axiograph-cli --release --locked

FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

# hadolint ignore=DL3008
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 10001 axiograph \
    && useradd -M -u 10001 -g axiograph -s /usr/sbin/nologin axiograph \
    && mkdir -p /data/store /usr/local/share/axiograph \
    && chown -R axiograph:axiograph /data

COPY --from=builder /app/rust/target/release/axiograph \
    /usr/local/bin/axiograph
COPY --from=builder /app/lean/.lake/build/bin/axiograph_verify \
    /usr/local/bin/axiograph_verify
COPY --from=builder /tmp/axiograph_verify.sha256 \
    /usr/local/share/axiograph/axiograph_verify.sha256
COPY --from=builder /tmp/axiograph-accepted-plane-smoke.json \
    /usr/local/share/axiograph/accepted-plane-smoke.json

WORKDIR /usr/local/bin
RUN sha256sum -c /usr/local/share/axiograph/axiograph_verify.sha256 \
    && chown root:root axiograph axiograph_verify \
        /usr/local/share/axiograph/axiograph_verify.sha256 \
        /usr/local/share/axiograph/accepted-plane-smoke.json \
    && chmod 0755 axiograph axiograph_verify \
    && chmod 0644 /usr/local/share/axiograph/axiograph_verify.sha256 \
        /usr/local/share/axiograph/accepted-plane-smoke.json

EXPOSE 7878

USER axiograph

ENTRYPOINT ["axiograph"]
# Serving requires an explicit AxiStore root and MaterializationIdV2; the image
# never guesses a mutable HEAD or opens a bare `.axpd` file.
CMD ["db", "--help"]
