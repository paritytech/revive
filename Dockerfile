FROM messense/rust-musl-cross:x86_64-musl
WORKDIR /opt/revive

RUN apt update && \ 
    apt upgrade -y && \
    apt install -y \
        build-essential \
        cmake \
        make \
        ninja-build \
        python3 \
        libmpfr-dev \
        libgmp-dev \
        libmpc-dev \
        ncurses-dev \
        git \
        curl \
        pkg-config

COPY . .

#ADD foo.tar.xz /opt/revive/llvm18.0

ENV PATH=/opt/revive/llvm18.0/bin:$PATH
RUN make install-bin
