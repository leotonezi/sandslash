# ── Stage 1: Rust builder ────────────────────────────────────────────────────
FROM rust:1-bookworm-slim AS rust-builder
WORKDIR /build

# Cache dependency compilation separately from source changes
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir -p src && \
    echo 'fn main() {}' > src/main.rs && \
    cargo build --release --bin sandslash && \
    rm -rf src \
           target/release/sandslash \
           target/release/deps/sandslash* \
           target/release/.fingerprint/sandslash*

COPY src ./src
RUN cargo build --release --bin sandslash

# ── Stage 2: Node builder ─────────────────────────────────────────────────────
FROM node:20-bookworm-slim AS node-builder
WORKDIR /app

COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci

COPY frontend/ ./
RUN mkdir -p public && npm run build

# ── Stage 3: Runtime ─────────────────────────────────────────────────────────
FROM node:20-bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --system --uid 1001 --no-create-home sandslash

WORKDIR /app

COPY --from=rust-builder /build/target/release/sandslash /usr/local/bin/sandslash

# Next.js standalone server + static assets (standalone does NOT copy these automatically)
COPY --from=node-builder /app/.next/standalone ./
COPY --from=node-builder /app/.next/static ./.next/static
COPY --from=node-builder /app/public ./public

RUN chown -R sandslash:sandslash /app

USER sandslash

ENV SEO_RS_BIN=/usr/local/bin/sandslash
ENV HOSTNAME=0.0.0.0
ENV PORT=3000
ENV NODE_ENV=production

EXPOSE 3000
CMD ["node", "server.js"]
