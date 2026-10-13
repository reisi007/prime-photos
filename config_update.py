#!/usr/bin/env python3
"""
Script to ingest cookies from browser session and update the config.toml file.
"""

import os
import re
import sys
from pathlib import Path


def get_config_path() -> Path:
    """
    Get OS-independent config path, similar to Rust's dirs::config_dir().

    Returns the appropriate config directory based on the OS:
    - macOS: ~/Library/Application Support/amzn_photos_uploader/config.toml
    - Linux: ~/.config/amzn_photos_uploader/config.toml
    - Windows: %APPDATA%/amzn_photos_uploader/config.toml
    """
    if sys.platform == "darwin":  # macOS
        config_dir = Path.home() / "Library" / "Application Support"
    elif sys.platform == "win32":  # Windows
        config_dir = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming"))
    else:  # Linux and other Unix-like systems
        config_dir = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))

    return config_dir / "amzn_photos_uploader" / "config.toml"


def parse_cookies(cookie_string: str) -> dict:
    """
    Parse cookies from a browser cookie string format.

    Expected format: "name1=value1; name2=value2; ..."
    """
    cookies = {}
    # Split by semicolon and process each cookie
    for cookie in cookie_string.split(';'):
        cookie = cookie.strip()
        if not cookie:
            continue
        # Split on first '=' to handle cookie values that may contain '='
        if '=' in cookie:
            name, value = cookie.split('=', 1)
            name = name.strip()
            value = value.strip()
            # Remove quotes if present
            if value.startswith('"') and value.endswith('"'):
                value = value[1:-1]
            cookies[name] = value
    return cookies


def extract_required_cookies(cookies: dict) -> dict:
    """
    Extract the required cookies for the configuration.

    Maps browser cookie names to config field names.
    """
    required_mapping = {
        'session-id': 'session_id',
        'x-acb': 'cookie_x_acb',
        'at-acb': 'cookie_at_acb',
        'ubid-acb': 'cookie_ubid_acb',
        'sess-at-acb': 'cookie_x_amz_access_token',
    }

    extracted = {}
    missing = []

    for cookie_name, config_field in required_mapping.items():
        # Find cookies that start with the required key (handles country variants like x-acbfr, x-acbus, etc.)
        matched_cookies = [cookie for cookie in cookies if cookie.startswith(cookie_name)]
        if matched_cookies:
            # Use the first match
            extracted[config_field] = cookies[matched_cookies[0]]
        else:
            missing.append(cookie_name)

    if missing:
        print(f"Warning: Missing cookies: {', '.join(missing)}", file=sys.stderr)

    return extracted


def load_config() -> dict:
    """Load existing configuration or return default values."""
    config_path = get_config_path()

    config = {
        'country': 'us',
        'zone': 'eu',
        'user_agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15',
        'session_id': '',
        'cookie_x_acb': '',
        'cookie_at_acb': '',
        'cookie_ubid_acb': '',
        'cookie_x_amz_access_token': '',
    }

    if config_path.exists():
        try:
            import tomllib
        except ImportError:
            # Python < 3.11
            try:
                import tomli as tomllib
            except ImportError:
                print("Warning: tomli/tomllib not available, using regex parsing", file=sys.stderr)
                # Simple regex-based TOML parsing for basic key=value pairs
                with open(config_path, 'r') as f:
                    content = f.read()
                    for key in config.keys():
                        pattern = rf'{key}\s*=\s*["\']([^"\']*)["\']'
                        match = re.search(pattern, content)
                        if match:
                            config[key] = match.group(1)
                return config

        try:
            with open(config_path, 'rb') as f:
                loaded = tomllib.load(f)
                config.update(loaded)
        except Exception as e:
            print(f"Warning: Could not load existing config: {e}", file=sys.stderr)

    return config


def save_config(config: dict) -> None:
    """Save configuration to TOML file."""
    config_path = get_config_path()
    config_path.parent.mkdir(parents=True, exist_ok=True)

    # Generate TOML content
    toml_content = []
    for key, value in config.items():
        # Escape quotes in values
        escaped_value = value.replace('"', '\\"')
        toml_content.append(f'{key} = "{escaped_value}"')

    with open(config_path, 'w') as f:
        f.write('\n'.join(toml_content))

    print(f"Configuration saved to {config_path}")


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        print("Usage: python3 config_update.py '<cookie_string>'")
        print("\nExample:")
        print("  python3 config_update.py 'session-id=123; x-acbfr=abc; ...'")
        sys.exit(1)

    cookie_string = sys.argv[1]

    # Parse cookies
    cookies = parse_cookies(cookie_string)

    if not cookies:
        print("Error: No cookies found in input", file=sys.stderr)
        sys.exit(1)

    # Extract required cookies
    extracted = extract_required_cookies(cookies)

    if not extracted:
        print("Error: No required cookies found in input", file=sys.stderr)
        sys.exit(1)

    # Load existing config
    config = load_config()

    # Update with new cookies
    config.update(extracted)

    # Save updated config
    save_config(config)

    # Print summary
    print("\nUpdated configuration with the following cookies:")
    for key in extracted.keys():
        print(f"   {key}")


if __name__ == "__main__":
    main()
