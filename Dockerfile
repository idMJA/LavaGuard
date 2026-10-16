# Build stage
FROM oven/bun:1-alpine AS builder

WORKDIR /app

# Install build dependencies for Rust and Bun
RUN apk add --no-cache curl build-base

# Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

# Install JS dependencies
COPY package.json bun.lock ./
RUN bun install --frozen-lockfile

# Copy source files
COPY . .

# Build release binary
RUN cargo build --release

# Runtime stage
FROM alpine:3.20

WORKDIR /app

# Install minimal runtime dependencies (ca-certificates, libgcc)
RUN apk add --no-cache ca-certificates libgcc

# Copy compiled binary from builder
COPY --from=builder /app/target/release/lavaguard /usr/local/bin/lavaguard

# Copy default configuration
COPY config.toml /app/config.toml

EXPOSE 20015

ENTRYPOINT ["/usr/local/bin/lavaguard"]
