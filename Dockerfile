#A Docker file is a recipe for the application environment

FROM rust:1.96.0

WORKDIR /app

RUN apt-get update && apt-get install lld clang -y

COPY . .

ENV SQLX_OFFLINE true

RUN cargo build --release

ENTRYPOINT ["./target/release/zero2prod"]