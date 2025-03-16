FROM rust as build

WORKDIR /usr/src
RUN USER=root cargo new server
WORKDIR /usr/src/server

# Caches build dependencies by writing placeholder lib and main files.
COPY Cargo.toml Cargo.lock ./

RUN cargo build --release --locked

COPY src ./src

# To trigger cargo to recompile
RUN touch src/main.rs

RUN cargo install --path . --offline

FROM debian:buster-slim

RUN apt-get update

COPY --from=build /usr/local/cargo/bin/server /usr/local/bin/server

EXPOSE 8000
CMD ["/usr/local/bin/server"]

