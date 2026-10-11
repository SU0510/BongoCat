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
# `curl` is what the compose healthcheck probes /health with. `/data` has to
# exist and belong to the runtime user before `VOLUME` is declared: Docker
# copies the image's ownership into the volume it creates, and a root-owned
# one leaves the server unable to open its database.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates curl \
 && rm -rf /var/lib/apt/lists/* \
 && mkdir -p /data \
 && chown 1000:1000 /data
COPY --from=build /src/target/release/insync-server /usr/local/bin/insync-server
ENV INSYNC_DB=/data/insync.db INSYNC_PORT=8787
VOLUME /data
EXPOSE 8787
USER 1000:1000
CMD ["/usr/local/bin/insync-server"]
