#!/usr/bin/env python3
"""
This script generates JSON files for different platforms based on GitHub data.
Requires the GITHUB_SHA, FIRST_SOLC_VERSION, LAST_SOLC_VERSION, TAG and FILEPATH environment variables to be set.
Usage:
    python json_generator_nightly.py
"""
import os
import sys
import json
from datetime import datetime

def validate_env_variables():
    """Validate that environment variables are set."""
    if "GITHUB_SHA" not in os.environ:
        print("Error: GITHUB_SHA environment variable is not set.")
        sys.exit(1)
    if "FIRST_SOLC_VERSION" not in os.environ:
        print("Error: FIRST_SOLC_VERSION environment variable is not set.")
        sys.exit(1)
    if "LAST_SOLC_VERSION" not in os.environ:
        print("Error: LAST_SOLC_VERSION environment variable is not set.")
        sys.exit(1)
    if "TAG" not in os.environ:
        print("Error: TAG environment variable is not set.")
        sys.exit(1)
    if "FILEPATH" not in os.environ:
        print("Error: FILEPATH environment variable is not set.")
        sys.exit(1)


def fetch_data_file():
    """
    Fetch the data.json file with artifacts urls and sha256 checksums
    and parse it into a single dictionary mapping artifact names to their URLs and SHAs.
    """
    # read data.json file
    artifacts_data = {}
    data_file_path = os.environ["FILEPATH"]
    if not os.path.exists(data_file_path):
        print("Error: data.json file not found.")
        sys.exit(1)
    with open(data_file_path, 'r') as f:
        try:
            artifacts_data = json.load(f)
        except json.JSONDecodeError:
            print("Error: data.json file is not a valid JSON.")
            sys.exit(1)

    result = {}

    for item in artifacts_data:
        for key, value in item.items():
            if key.endswith('_url'):
                base_key = key.rsplit('_url', 1)[0]
                if base_key not in result:
                    result[base_key] = {}
                result[base_key]['url'] = value
            elif key.endswith('_sha'):
                base_key = key.rsplit('_sha', 1)[0]
                if base_key not in result:
                    result[base_key] = {}
                result[base_key]['sha'] = value

    return result





def extract_build_hash():
    """Extract the first 8 characters of the commit hash."""
    sha = os.environ.get("GITHUB_SHA")
    return f"commit.{sha[:8]}"

def generate_asset_json_nightly(name, url, checksum):
    """Generate JSON for a specific asset."""
    # Date in format YYYY-MM-DD
    date = datetime.now().strftime("%Y.%m.%d")
    last_version = os.environ.get("TAG").replace('v','')
    version = f"{last_version}-nightly.{date}"
    SHA = os.environ.get("GITHUB_SHA", "")[:8]
    build = f"commit.{SHA}"
    long_version = f"{version}+{build}"

    return {
        "name": name,
        "version": version,
        "build": build,
        "longVersion": long_version,
        "url": url,
        "sha256": checksum,
        "firstSolcVersion": os.environ.get("FIRST_SOLC_VERSION"),
        "lastSolcVersion": os.environ.get("LAST_SOLC_VERSION")
    }

def save_platform_json(platform_folder, asset_json):
    """Save asset JSON and update list.json for a specific platform."""
    # Create platform folder if it doesn't exist
    os.makedirs(platform_folder, exist_ok=True)

    # Update or create list.json
    list_file_path = os.path.join(platform_folder, "list.json")

    if os.path.exists(list_file_path):
        with open(list_file_path, 'r') as f:
            try:
                list_data = json.load(f)
            except json.JSONDecodeError:
                list_data = {"builds": [], "releases": {}, "latestRelease": ""}
    else:
        list_data = {"builds": [], "releases": {}, "latestRelease": ""}

    # Remove any existing entry with the same path
    list_data['builds'] = [
        build for build in list_data['builds']
        if build['version'] != asset_json['version']
    ]
    # Add the new build
    list_data['builds'].append(asset_json)

    # Update releases
    version = asset_json['version']
    list_data['releases'][version] = f"{asset_json['name']}+{asset_json['longVersion']}"

    # Update latest release
    list_data['latestRelease'] = version

    with open(list_file_path, 'w') as f:
        json.dump(list_data, f, indent=4)

def main():

    validate_env_variables()
    data = fetch_data_file()

    # Mapping of asset names to platform folders
    platform_mapping = {
        'resolc-x86_64-unknown-linux-musl': 'linux',
        'resolc-universal-apple-darwin': 'macos',
        'resolc-x86_64-pc-windows-msvc': 'windows',
        'resolc-web.js': 'wasm'
    }

    # Process each asset
    for asset in data.keys():
        platform_name = platform_mapping.get(asset)
        if platform_name:
            platform_folder = os.path.join(platform_name)
            asset_json = generate_asset_json_nightly(asset, data[asset]['url'], data[asset]['sha'])
            save_platform_json(platform_folder, asset_json)
            print(f"Processed {asset} for {platform_name}")

if __name__ == "__main__":
    main()