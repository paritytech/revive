FROM revive-llvm

WORKDIR /app

COPY . .
RUN RUSTFLAGS='-L /app/llvm18.0/lib/unknown -L /usr/lib -L /app/llvm18.0/lib' REVIVE_INSTALL_DIR='/app/release/revive/' make install-revive