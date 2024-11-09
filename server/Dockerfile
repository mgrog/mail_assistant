# docker build --build-arg  --tag haze-service:latest .

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
RUN mv server/config.release.toml server/config.toml
RUN mv server/client_secret.release.toml server/client_secret.toml
RUN cargo build --release

FROM builder as check
RUN rustup component add clippy rustfmt
RUN cargo clippy --all-targets --all-features
RUN cargo test --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y openssl
COPY --from=builder /app/target/release/server /usr/local/bin/mailclerk-server
COPY --from=builder /app/server/config.toml /usr/local/bin/config.toml
COPY --from=builder /app/server/client_secret.toml /usr/local/bin/client_secret.toml
ENV APP_DIR="/usr/local/bin"
STOPSIGNAL SIGINT
ENTRYPOINT ["/usr/local/bin/mailclerk-server"]
