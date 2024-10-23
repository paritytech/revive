# syntax=docker/dockerfile:1
# Dockerfile for building revive in a Debian container.
FROM debian:12
RUN <<EOF
apt-get update
apt-get install -q -y build-essential cmake make ninja-build python3 \
    libmpfr-dev libgmp-dev libmpc-dev ncurses-dev \
    git curl
EOF
ARG RUST_VERSION=stable
RUN <<EOF
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain ${RUST_VERSION}
EOF
ENV PATH=/root/.cargo/bin:${PATH}
