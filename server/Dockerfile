# docker build --tag mailclerk-server:latest .

# Leveraging the pre-built Docker images with 
# cargo-chef and the Rust toolchain
FROM lukemathwalker/cargo-chef:latest-rust-1.82 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
RUN cargo build --release

FROM builder AS check
RUN rustup component add clippy rustfmt
RUN cargo clippy --all-targets --all-features
RUN cargo test --release

FROM alpine:3 AS runtime
RUN apk update && apk add openssl
COPY --from=builder /app/target/release/server /app/mailclerk-server
COPY --from=builder /app/server/config.release.toml /app/config.toml
COPY --from=builder /app/server/client_secret.release.toml /app/client_secret.toml
COPY --from=builder /app/server/cert.pem /app/cert.pem
ENV APP_DIR=/app
STOPSIGNAL SIGINT
ENTRYPOINT ["/app/mailclerk-server"]
