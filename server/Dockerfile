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
COPY Cargo.lock /app/Cargo.lock
COPY Cargo.toml /app/Cargo.toml
COPY /server /app/server
COPY /entity /app/entity
COPY /libs /app/libs
RUN cargo build --release

FROM builder AS configure
COPY /config /app/config
COPY /config/.env.release /app/config/.env.release

FROM builder AS check
RUN rustup component add clippy rustfmt
RUN cargo clippy --all-targets --all-features
RUN cargo test --release

FROM gcr.io/distroless/cc AS runtime
COPY --from=configure /app/target/release/server /app/mailclerk-server
COPY --from=configure /app/config/config.release.toml /app/config.toml
COPY --from=configure /app/config/client_secret.release.toml /app/client_secret.toml
COPY --from=configure /app/config/cert.pem /app/cert.pem
COPY --from=configure /app/config/.env.release /app/.env
ENV APP_DIR=/app
WORKDIR /app
STOPSIGNAL SIGINT
ENTRYPOINT ["./mailclerk-server"]
