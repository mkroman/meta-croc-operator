FROM rust:1.58.1 as builder

WORKDIR /usr/src

COPY . .

RUN cargo fetch
RUN cargo build --release
RUN strip --strip-unneeded target/release/meta-croc-operator

FROM debian:11.2-slim

ARG created

LABEL org.opencontainers.image.authors="Mikkel Kroman <mk@maero.dk>"

RUN apt update \
  && apt install -y openssl ca-certificates

COPY --from=builder /usr/src/target/release/meta-croc-operator /usr/local/bin/meta-croc-operator

EXPOSE 3000

USER nobody
CMD ["/usr/local/bin/meta-croc-operator"]
