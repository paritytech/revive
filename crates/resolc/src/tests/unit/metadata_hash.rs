//! Unit tests for embedding the metadata hash into the linked PVM blob.

use crate::test_utils::compile_yul_blob;

const TEST_YUL: &str = r#"object "Test" {
    code {
        {
            let s := datasize("Test_deployed")
            codecopy(0, dataoffset("Test_deployed"), s)
            return(0, s)
        }
    }
    object "Test_deployed" {
        code {
            {
                mstore(0, 42)
                return(0, 32)
            }
        }
    }
}"#;

#[test]
fn metadata_hash_is_embedded_in_blob() {
    let blob = compile_yul_blob("Test", TEST_YUL);
    let hash = revive_llvm_context::polkavm_metadata_hash(&blob)
        .expect("the linked blob should parse")
        .expect("the linked blob should carry the keccak256 metadata hash");
    assert_eq!(hash.len(), revive_common::BYTE_LENGTH_WORD);
}
