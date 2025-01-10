FROM rust:1.84.0 AS llvm-builder
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y cmake ninja-build curl git libssl-dev pkg-config clang lld

COPY . .

RUN make install-llvm-builder && \
    revive-llvm --target-env musl clone && \
    revive-llvm --target-env musl build --enable-assertions --llvm-projects clang --llvm-projects lld

FROM messense/rust-musl-cross:x86_64-musl
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y pkg-config

COPY . .
COPY --from=llvm-builder /opt/revive/target-llvm /opt/revive/target-llvm

ENV PATH=/opt/revive/target-llvm/musl/target-final/bin:$PATH
RUN make install-bin
