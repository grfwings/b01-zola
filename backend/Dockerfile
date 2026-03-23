FROM rust:1.87 AS builder
RUN apt-get update && apt-get install -y pip && pip install cargo-zigbuild --break-system-packages
RUN rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations

ARG TARGETARCH
RUN case "$TARGETARCH" in \
      amd64) RUST_TARGET=x86_64-unknown-linux-gnu ;; \
      arm64) RUST_TARGET=aarch64-unknown-linux-gnu ;; \
      *) echo "Unsupported arch: $TARGETARCH" && exit 1 ;; \
    esac && \
    cargo zigbuild --release --target "$RUST_TARGET" && \
    cp target/$RUST_TARGET/release/b01-backend /app/b01-backend

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/b01-backend /usr/local/bin/
EXPOSE 3001
CMD ["b01-backend"]
