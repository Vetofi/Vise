FROM rust:1-slim-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release --locked -p vise-relay \
    && cp target/release/vise-relay /vise-relay

FROM debian:bookworm-slim
COPY --from=build /vise-relay /usr/local/bin/vise-relay
ENV PORT=7000
EXPOSE 7000
USER nobody
CMD ["vise-relay"]
