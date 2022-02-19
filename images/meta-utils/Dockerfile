FROM alpine:3.15.0

LABEL org.opencontainers.image.authors="Mikkel Kroman <mk@maero.dk>"

RUN apk add --no-cache \
  bash \
  curl \
  aws-cli \
  xxhash \
  ruby

USER nobody

ENTRYPOINT ["/bin/bash"]
