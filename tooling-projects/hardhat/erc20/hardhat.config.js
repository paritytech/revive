require("@nomicfoundation/hardhat-toolbox");
require("@parity/hardhat-polkadot");

const RESOLC_PATH_ENV_NAME = "RESOLC_PATH";

function getEnv(name) {
    const value = process.env[name];
    if (!value) {
        throw new Error(`Required environment variable '${name}' is missing`);
    }

    return value;
}

/** @type {import('hardhat/config').HardhatUserConfig} */
module.exports = {
    solidity: "0.8.35",
    resolc: {
        compilerSource: "binary",
        settings: {
            resolcPath: getEnv(RESOLC_PATH_ENV_NAME),
            optimizer: {
                enabled: true,
                parameters: "z"
            },
        },
    },
    networks: {
        hardhat: {
            polkadot: true
        },
    },
};
