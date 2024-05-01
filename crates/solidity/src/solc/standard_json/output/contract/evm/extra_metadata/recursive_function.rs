//! The `solc --standard-json` output contract EVM recursive function.

use serde::Deserialize;
use serde::Serialize;

/// The `solc --standard-json` output contract EVM recursive function.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecursiveFunction {
    /// The function name.
    pub name: String,
    /// The creation code function block tag.
    pub creation_tag: Option<usize>,
    /// The runtime code function block tag.
    pub runtime_tag: Option<usize>,
    /// The number of input arguments.
    #[serde(rename = "totalParamSize")]
    pub input_size: usize,
    /// The number of output arguments.
    #[serde(rename = "totalRetParamSize")]
    pub output_size: usize,
}
