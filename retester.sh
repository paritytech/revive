WORKDIR=workdir
rm -r ${WORKDIR}
mkdir -p ${WORKDIR}

ulimit -n 524288

retester test \
    --test revive-differential-tests/resolc-compiler-tests/fixtures/solidity/simple \
    --test revive-differential-tests/resolc-compiler-tests/fixtures/solidity/complex \
    --test revive-differential-tests/resolc-compiler-tests/fixtures/solidity/translated_semantic_tests \
    --platform revive-dev-node-polkavm-resolc \
    --report.file-name report.json \
    --concurrency.number-of-nodes 24 \
    --concurrency.number-of-threads 32 \
    --concurrency.number-of-concurrent-tasks 1024 \
    --working-directory ${WORKDIR} \
    --revive-dev-node.consensus manual-seal-200 \
    --revive-dev-node.path revive-dev-node \
    --eth-rpc.path eth-rpc \
    --resolc.path "$(which resolc)" \
    --resolc.heap-size 500000
