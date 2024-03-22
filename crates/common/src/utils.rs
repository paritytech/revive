//!
//! The compiler common utils.
//!

///
/// Deserializes a `serde_json` object from slice with the recursion limit disabled.
///
/// Must be used for all JSON I/O to avoid crashes due to the aforementioned limit.
///
pub fn deserialize_from_slice<O>(input: &[u8]) -> anyhow::Result<O>
where
    O: serde::de::DeserializeOwned,
{
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    deserializer.disable_recursion_limit();
    let deserializer = serde_stacker::Deserializer::new(&mut deserializer);
    let result = O::deserialize(deserializer)?;
    Ok(result)
}

///
/// Deserializes a `serde_json` object from string with the recursion limit disabled.
///
/// Must be used for all JSON I/O to avoid crashes due to the aforementioned limit.
///
pub fn deserialize_from_str<O>(input: &str) -> anyhow::Result<O>
where
    O: serde::de::DeserializeOwned,
{
    let mut deserializer = serde_json::Deserializer::from_str(input);
    deserializer.disable_recursion_limit();
    let deserializer = serde_stacker::Deserializer::new(&mut deserializer);
    let result = O::deserialize(deserializer)?;
    Ok(result)
}
