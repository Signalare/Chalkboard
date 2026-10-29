# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Build environment for the Chalkboard OS image (Buildroot needs Linux).
# Used locally through `just os-build` and later as the CI build image (#5).

# debian:bookworm-slim, pinned by digest for reproducible builds.
FROM debian@sha256:99c0b25c46d0214d5a2091092e7908907dcaecd6a962cde0ac700acee222b6f3

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      bc bison build-essential bzip2 ca-certificates cpio file flex git \
      libncurses-dev libssl-dev locales patch perl python3 rsync unzip wget \
      xz-utils \
 && sed -i 's/^# *\(en_US.UTF-8\)/\1/' /etc/locale.gen && locale-gen \
 && rm -rf /var/lib/apt/lists/*

ENV LANG=en_US.UTF-8

# Build as an unprivileged user; /build is a Docker volume (case-sensitive,
# fast), /src is the repository mounted read-only, /out receives images.
RUN useradd --create-home --uid 1000 builder \
 && mkdir -p /build /out && chown builder:builder /build /out
USER builder
WORKDIR /build
