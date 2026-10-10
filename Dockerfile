# InSync server: single static-ish binary + SQLite. Build via cargo-chef-less
# two-stage build (workspace has few binaries; full workspace build is OK).
FROM rust:1.97-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates ./crates
COPY shared ./shared
COPY vendor ./vendor
COPY resources ./resources
RUN cargo build --release -p insync-server

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/insync-server /usr/local/bin/insync-server
ENV INSYNC_DB=/data/insync.db INSYNC_PORT=8787
VOLUME /data
EXPOSE 8787
USER 1000:1000
CMD ["/usr/local/bin/insync-server"]
