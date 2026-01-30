FROM rust:1.92.0 AS llvm-builder
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y cmake ninja-build curl git libssl-dev pkg-config clang lld musl

COPY . .

RUN make install-llvm-builder
RUN revive-llvm --target-env musl build  --llvm-projects lld --llvm-projects clang

FROM messense/rust-musl-cross@sha256:2a8837c43bf12e246f1ebd05191de9ee27fcd22f9ca81511ccd4cf75dc16d71c AS resolc-builder
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y pkg-config

COPY . .
COPY --from=llvm-builder /opt/revive/target-llvm /opt/revive/target-llvm

ENV LLVM_SYS_211_PREFIX=/opt/revive/target-llvm/musl/target-final
RUN make install-bin

FROM alpine:latest
ADD https://github.com/ethereum/solidity/releases/download/v0.8.33/solc-static-linux /usr/bin/solc
COPY --from=resolc-builder /root/.cargo/bin/resolc /usr/bin/resolc

RUN apk add --no-cache libc6-compat
RUN chmod +x /usr/bin/solc 
