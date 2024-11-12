# syntax=docker/dockerfile:1
# Dockerfile for building revive in a Debian container.
FROM alpine:3.20
RUN <<EOF
apk upgrade
apk add build-base git curl cmake make ninja-build python3 \
    mpfr-dev gmp-dev mpc1-dev ncurses-dev ncurses-static \
    g++ libstdc++ \
    llvm llvm-dev llvm-static llvm-runtimes \
    clang clang-libs clang-dev clang-static 
EOF
ARG RUST_VERSION=stable
RUN <<EOF
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain ${RUST_VERSION}
EOF
ENV PATH=/root/.cargo/bin:${PATH}
