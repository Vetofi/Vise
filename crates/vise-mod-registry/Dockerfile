FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.toml
COPY crates/vise-mods crates/vise-mods
COPY crates/vise-shader-api crates/vise-shader-api
COPY crates/vise-mod-registry crates/vise-mod-registry
RUN cargo build --release --locked --manifest-path crates/vise-mod-registry/Cargo.toml

FROM debian:stable-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/crates/vise-mod-registry/target/release/vise-mod-registry /usr/local/bin/
ENV REGISTRY_DB=/data/registry.sqlite3
CMD ["sh", "-c", "mkdir -p /data && chown nobody:nogroup /data && exec setpriv --reuid=nobody --regid=nogroup --clear-groups vise-mod-registry"]
