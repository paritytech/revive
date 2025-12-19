use revive_solc_json_interface::SolcStandardJsonInputSettingsMetadataHash;

#[test]
fn accepts_ipfs_metadata_hash_in_standard_json() {
    let parsed: SolcStandardJsonInputSettingsMetadataHash =
        serde_json::from_str("\"ipfs\"").expect("should deserialize 'ipfs'");
    assert_eq!(parsed, SolcStandardJsonInputSettingsMetadataHash::IPFS);
}
