require("@nomicfoundation/hardhat-toolbox");
require("@parity/hardhat-polkadot");

const RESOLC_PATH_ENV_NAME = "RESOLC_PATH";
const SOLC_VERSION_ENV_NAME = "SOLC_VERSION";

function getEnv(name) {
    const value = process.env[name];
    if (!value) {
        throw new Error(`Required environment variable '${name}' is missing`);
    }

    return value;
}

/** @type {import('hardhat/config').HardhatUserConfig} */
module.exports = {
    solidity: getEnv(SOLC_VERSION_ENV_NAME),
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
