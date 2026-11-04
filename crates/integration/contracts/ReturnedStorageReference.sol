// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
contract ReturnedStorageReference {
    struct Entry {
        uint256 first;
        uint256 second;
    }

    mapping(uint256 => Entry) entries;

    function entry(uint256 key, uint256 depth) internal returns (Entry storage result) {
        if (depth > 0) entry(key, depth - 1);
        result = entries[key];
        result.first = 1;
    }

    function write(uint256 key, uint256 depth) external returns (uint256) {
        Entry storage result = entry(key, depth);
        result.second = 2;
        return entries[key].second;
    }
}
