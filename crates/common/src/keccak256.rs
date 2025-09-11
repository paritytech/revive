//! Keccak-256 hash utilities.

use sha3::digest::FixedOutput;
use sha3::Digest;

pub const DIGEST_BYTES: usize = 32;

/// Keccak-256 hash utilities.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Keccak256 {
    /// Binary representation.
    bytes: [u8; DIGEST_BYTES],
    /// Hexadecimal string representation.
    string: String,
}

impl Keccak256 {
    /// Computes the `keccak256` hash for `preimage`.
    pub fn from_slice(preimage: &[u8]) -> Self {
        let bytes = sha3::Keccak256::digest(preimage).into();
        let string = format!("0x{}", hex::encode(bytes));
        Self { bytes, string }
    }

    /// Computes the `keccak256` hash for an array of `preimages`.
    pub fn from_slices<R: AsRef<[u8]>>(preimages: &[R]) -> Self {
        let mut hasher = sha3::Keccak256::new();
        for preimage in preimages.iter() {
            hasher.update(preimage);
        }
        let bytes: [u8; DIGEST_BYTES] = hasher.finalize_fixed().into();
        let string = format!("0x{}", hex::encode(bytes));
        Self { bytes, string }
    }

    /// Returns a reference to the 32-byte SHA-3 hash.
    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    /// Returns a reference to the hexadecimal string representation of the IPFS hash.
    pub fn as_str(&self) -> &str {
        self.string.as_str()
    }

    /// Extracts the binary representation.
    pub fn to_vec(&self) -> Vec<u8> {
        self.bytes.to_vec()
    }
}

impl std::fmt::Display for Keccak256 {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hash_and_stringify_works() {
        assert_eq!(
            super::Keccak256::from_slices(&["foo".as_bytes(), "bar".as_bytes(),]).as_str(),
            "0x38d18acb67d25c8bb9942764b62f18e17054f66a817bd4295423adf9ed98873e"
        );
    }
}
