# ─── Build Stage ────────────────────────────────────────────────────────────────
FROM rust:1.99-slim-bookworm AS builder

# Constrain build RAM in the 8 GB Docker VM
ENV CARGO_BUILD_JOBS=2

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Build cargo-leptos 0.3.x (supports Leptos 0.8) as a plain root package.
# Its manifest sets [profile.release] lto=true + codegen-units=1, which OOMs
# the 8 GB build VM on this dependency tree - override all three. Lands in
# $CARGO_HOME/bin (on PATH).
RUN curl -fsSL https://static.crates.io/crates/cargo-leptos/cargo-leptos-0.3.11.crate \
        -o /tmp/cargo-leptos.crate \
    && tar -xzf /tmp/cargo-leptos.crate -C /opt \
    && rm /tmp/cargo-leptos.crate \
    && cd /opt/cargo-leptos-0.3.11 \
    && cargo build --release \
        --config profile.release.opt-level=0 \
        --config profile.release.lto=false \
        --config profile.release.codegen-units=256 \
    && install -m 755 target/release/cargo-leptos /usr/local/cargo/bin/cargo-leptos

WORKDIR /app

# Copy source
COPY . .

# Add the WASM target for the toolchain that this directory resolves to
# (rust-toolchain.toml here selects a specific toolchain; `rustup target add`
# must run in this context so wasm32 lands on the active toolchain).
RUN rustup target add wasm32-unknown-unknown

# Build (produces SSR binary + WASM client bundle).
# The wasm-release profile also uses lto=true; soften it to stay inside the
# 8 GB VM budget (slightly larger WASM, same output).
ENV CARGO_PROFILE_WASM_RELEASE_LTO=false \
    CARGO_PROFILE_WASM_RELEASE_CODEGEN_UNITS=16
RUN cargo leptos build --release

# ─── Runtime Stage ──────────────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m appuser

# Preserve relative path structure expected by leptos_axum
COPY --from=builder /app/target/release/my_leptos_app /app/my_leptos_app
COPY --from=builder /app/target/site /app/target/site

WORKDIR /app
USER appuser

# get_configuration(None) reads these at runtime; without them Leptos binds
# 127.0.0.1 (unreachable through Docker port publishing) and runs in DEV mode.
ENV LEPTOS_OUTPUT_NAME=my_leptos_app \
    LEPTOS_SITE_ROOT=target/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:3000 \
    LEPTOS_ENV=PROD \
    RUST_LOG=info

EXPOSE 3000

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s \
    CMD curl -f http://localhost:3000 || exit 1

CMD ["./my_leptos_app"]
