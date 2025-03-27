import os
import sys
import json
import requests
from datetime import datetime

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

def extract_build_hash(target_commitish):
    """Extract the first 8 characters of the commit hash."""
    return f"commit.{target_commitish[:8]}"

def generate_asset_json(release_data, asset):
    """Generate JSON for a specific asset."""
    version = release_data['tag_name'].lstrip('v')
    build = extract_build_hash(release_data['target_commitish'])
    long_version = f"{version}+{build}"
    path = f"{asset['name']}+{long_version}"

    return {
        "path": path,
        "name": asset['name'],
        "version": version,
        "build": build,
        "longVersion": long_version,
        "url": asset['browser_download_url']
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
        if build['path'] != asset_json['path']
    ]
    # Add the new build
    list_data['builds'].append(asset_json)

    # Update releases
    version = asset_json['version']
    list_data['releases'][version] = asset_json['path']

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

    # Mapping of asset names to platform folders
    platform_mapping = {
        'resolc-x86_64-unknown-linux-musl': 'linux',
        'resolc-universal-apple-darwin': 'macos',
        'resolc-x86_64-pc-windows-msvc.exe': 'windows',
        'resolc.wasm': 'wasm',
        'resolc.js': 'js',
        'resolc_web.js': 'js_web'
    }

    # Process each asset
    for asset in release_data['assets']:
        platform_name = platform_mapping.get(asset['name'])
        if platform_name:
            platform_folder = os.path.join(platform_name)
            asset_json = generate_asset_json(release_data, asset)
            save_platform_json(platform_folder, asset_json, tag)
            print(f"Processed {asset['name']} for {platform_name}")

if __name__ == "__main__":
    main()