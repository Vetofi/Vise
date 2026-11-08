FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p vise-client --no-default-features --bin vise-client \
    && cargo build --release --locked -p vise-server --bin vise-server \
    && mkdir /out && cp target/release/vise-client /out/vise-engine && cp target/release/vise-server /out/

FROM debian:bookworm-slim
RUN useradd --system --home-dir /data --shell /usr/sbin/nologin vise \
    && mkdir -p /data /opt/vise/.local/vise-client \
    && chown vise:vise /data
COPY --from=build /out/ /opt/vise/
ENV PATH=/opt/vise:$PATH \
    VISE_NO_AUDIO=1
WORKDIR /data
VOLUME /data
EXPOSE 25565
USER vise
CMD ["sh", "-c", "test -f server.toml || vise-server init --yes || exit 1; exec vise-server run"]
