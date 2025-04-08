FROM rust:1.84.0 AS llvm-builder
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y cmake ninja-build curl git libssl-dev pkg-config clang lld musl

COPY . .

RUN make install-llvm-builder
RUN revive-llvm --target-env musl clone
RUN revive-llvm --target-env musl build  --llvm-projects lld --llvm-projects clang

FROM messense/rust-musl-cross@sha256:68b86bc7cb2867259e6b233415a665ff4469c28b57763e78c3bfea1c68091561 AS resolc-builder
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y pkg-config

COPY . .
COPY --from=llvm-builder /opt/revive/target-llvm /opt/revive/target-llvm

ENV LLVM_SYS_181_PREFIX=/opt/revive/target-llvm/musl/target-final
RUN make install-bin

FROM alpine:latest
ADD https://github.com/ethereum/solidity/releases/download/v0.8.28/solc-static-linux /usr/bin/solc
COPY --from=resolc-builder /root/.cargo/bin/resolc /usr/bin/resolc

RUN apk add --no-cache libc6-compat
RUN chmod +x /usr/bin/solc 
