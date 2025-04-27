FROM rust:1.86.0

WORKDIR /app

RUN apt update && apt install lld clang -y

RUN cargo new /app
COPY Cargo.lock .
COPY Cargo.toml .

RUN cargo fetch
RUN cargo build --release

COPY . .

ENV SQLX_OFFLINE true
ENV APP_ENVIRONMENT production
RUN cargo build --release

ENTRYPOINT ["./target/release/zero2prod"]