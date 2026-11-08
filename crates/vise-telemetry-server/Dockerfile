FROM rust:1-slim AS build
WORKDIR /src
COPY Cargo.toml Cargo.toml
COPY crates/vise-telemetry crates/vise-telemetry
COPY crates/vise-telemetry-server crates/vise-telemetry-server
RUN cargo build --release --locked --manifest-path crates/vise-telemetry-server/Cargo.toml

FROM debian:stable-slim
COPY --from=build /src/crates/vise-telemetry-server/target/release/vise-telemetry-server /usr/local/bin/
ENV VISE_TELEMETRY_DB=/data/telemetry.sqlite3 VISE_TELEMETRY_TRUST_PROXY=1
CMD ["vise-telemetry-server"]
