import os
import sys
import json
import requests

def validate_github_token():
    """Validate that GITHUB_TOKEN environment variable is set."""
    if 'GITHUB_TOKEN' not in os.environ:
        print("Error: GITHUB_TOKEN environment variable is not set.")
        sys.exit(1)

def fetch_release_data(repo, tag):
    """Fetch release data from GitHub API."""
    url = f"https://api.github.com/repos/{repo}/releases/tags/{tag}"
    headers = {
        'Authorization': f"Bearer {os.environ['GITHUB_TOKEN']}",
        'Accept': 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28'
    }

    try:
        response = requests.get(url, headers=headers)
        response.raise_for_status()
        return response.json()
    except requests.RequestException as e:
        print(f"Error fetching release data: {e}")
        sys.exit(1)

def fetch_checksum_file(release_data):
    """
    Fetch the checksum.txt file from the release assets
    and parse it into a dictionary mapping file names to their SHA256 checksums.
    """
    checksums = {}

    # Find the checksum.txt asset URL
    checksum_asset = None
    for asset in release_data['assets']:
        if asset['name'] == 'checksums.txt':
            checksum_asset = asset
            break

    if not checksum_asset:
        print("Warning: checksum.txt file not found in release assets.")
        return checksums

    # Download the checksum file
    headers = {
        'Authorization': f"Bearer {os.environ['GITHUB_TOKEN']}",
        'Accept': 'application/octet-stream'
    }

    try:
        response = requests.get(checksum_asset['browser_download_url'], headers=headers)
        response.raise_for_status()

        # Parse checksum file
        for line in response.text.splitlines():
            if line.strip():
                checksum, filename = line.strip().split(None, 1)
                checksums[filename] = checksum

        return checksums
    except requests.RequestException as e:
        print(f"Error fetching checksum file: {e}")
        return checksums
    except Exception as e:
        print(f"Error parsing checksum file: {e}")
        return checksums

def extract_build_hash(target_commitish):
    """Extract the first 8 characters of the commit hash."""
    return f"commit.{target_commitish[:8]}"

def generate_asset_json(release_data, asset, checksums):
    """Generate JSON for a specific asset."""
    version = release_data['tag_name'].lstrip('v')
    build = extract_build_hash(release_data['target_commitish'])
    long_version = f"{version}+{build}"

    # Get SHA256 checksum if available
    sha256 = checksums.get(asset['name'], "")

    return {
        "name": asset['name'],
        "version": version,
        "build": build,
        "longVersion": long_version,
        "url": asset['browser_download_url'],
        "sha256": sha256,
        "firstSolcVersion": os.environ.get("FIRST_SOLC_VERSION", ""),
        "lastSolcVersion": os.environ.get("LAST_SOLC_VERSION", "")
    }

def save_platform_json(platform_folder, asset_json, tag):
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
    # Validate arguments
    if len(sys.argv) != 3:
        print("Usage: python script.py <repo> <tag>")
        sys.exit(1)

    repo, tag = sys.argv[1], sys.argv[2]

    # Validate GitHub token
    validate_github_token()

    # Fetch release data
    release_data = fetch_release_data(repo, tag)

    # Fetch checksums
    checksums = fetch_checksum_file(release_data)

    # Mapping of asset names to platform folders
    platform_mapping = {
        'resolc-x86_64-unknown-linux-musl': 'linux',
        'resolc-universal-apple-darwin': 'macos',
        'resolc-x86_64-pc-windows-msvc.exe': 'windows',
        'resolc_web.js': 'wasm'
    }

    # Process each asset
    for asset in release_data['assets']:
        platform_name = platform_mapping.get(asset['name'])
        if platform_name:
            platform_folder = os.path.join(platform_name)
            asset_json = generate_asset_json(release_data, asset, checksums)
            save_platform_json(platform_folder, asset_json, tag)
            print(f"Processed {asset['name']} for {platform_name}")

if __name__ == "__main__":
    main()