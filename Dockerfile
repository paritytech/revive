FROM ethereum/solc:0.8.26-alpine

COPY target/release/resolc /usr/local/bin/resolc

ENTRYPOINT ["/usr/local/bin/resolc"]
