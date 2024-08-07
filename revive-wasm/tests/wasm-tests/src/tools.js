// Import the resolc module
import ResolcModule from '../../../../target/wasm32-unknown-emscripten/release/resolc.js';

// Provide the the resolc compiler
export async function runResolc(resolc_options) {
    const Resolc = await ResolcModule()
    // Run the 'resolc' compiler with option '--version'
    return Resolc.callMain(resolc_options);
}
