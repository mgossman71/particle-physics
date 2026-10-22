# ─── Build Stage ────────────────────────────────────────────────────────────────
FROM rust:1.84-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Add wasm target for client-side build
RUN rustup target add wasm32-unknown-unknown

# Install cargo-leptos (matching Leptos 0.8)
RUN cargo install cargo-leptos --version 0.8.0 --locked

WORKDIR /app

# Copy source
COPY . .

# Build (produces SSR binary + WASM client bundle)
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

EXPOSE 3000

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s \
    CMD curl -f http://localhost:3000 || exit 1

CMD ["./my_leptos_app"]
