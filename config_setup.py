#!/usr/bin/env python3
"""
Script to perform initial configuration setup for Amazon Photos Uploader.
Infers country from cookie names and provides default values.
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


def infer_country_from_cookies(cookies: dict) -> str:
    """
    Infer the country code from cookie names.

    Looks for country-specific cookies like x-acbfr, x-acbus, x-acbgb, etc.
    Returns the inferred country code or 'us' as default.
    """
    # Common country-specific cookie patterns
    country_patterns = [
        'x-acb',
        'at-acb',
        'ubid-acb',
        'sess-at-acb',
    ]

    for pattern in country_patterns:
        for cookie_name in cookies.keys():
            if cookie_name.startswith(pattern) and len(cookie_name) > len(pattern):
                # Extract country code (usually the suffix after the pattern)
                country_code = cookie_name[len(pattern):]
                if country_code and len(country_code) <= 3:  # Country codes are typically 2 chars
                    return country_code.lower()

    # Default to 'us' if no country-specific cookies found
    return 'us'


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


def create_default_config(country: str) -> dict:
    """Create a default configuration with inferred country."""
    return {
        'country': country,
        'zone': 'eu',  # Default zone, may need to be updated
        'user_agent': 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.2 Safari/605.1.15',
        'session_id': '',
        'cookie_x_acb': '',
        'cookie_at_acb': '',
        'cookie_ubid_acb': '',
        'cookie_x_amz_access_token': '',
    }


def save_config(config: dict) -> None:
    """Save configuration to TOML file."""
    config_path = get_config_path()
    config_path.parent.mkdir(parents=True, exist_ok=True)

    # Generate TOML content
    toml_content = []
    for key, value in config.items():
        # Escape quotes in values
        escaped_value = str(value).replace('"', '\\"')
        toml_content.append(f'{key} = "{escaped_value}"')

    with open(config_path, 'w') as f:
        f.write('\n'.join(toml_content))

    return config_path


def print_setup_summary(config_path: Path, country: str, zone: str):
    """Print a summary of the setup and helpful hints."""
    print("\n" + "="*70)
    print("Configuration Setup Complete!")
    print("="*70)
    print(f"\nConfiguration file created at:")
    print(f"  {config_path}")
    print(f"\nDetected country: {country}")
    print(f"Default zone: {zone}")

    print("\n" + "-"*70)
    print("IMPORTANT: Zone Configuration")
    print("-"*70)
    print("""
If the tool fails to upload photos, you may need to update the 'zone'
parameter in your configuration file.

To find the correct zone:

  1. Open Amazon Photos in your web browser
  2. Log in with your account
  3. Open Developer Tools (F12 or Right-click → Inspect)
  4. Go to the 'Network' tab
  5. Upload a photo through the web interface
  6. Look for a request to a URL like:
     https://content-XX.drive.amazonaws.com/v2/upload

     Where XX is your zone (e.g., 'eu', 'na', 'fe')

  7. Edit your config file and update the 'zone' parameter:
""")
    print(f"     zone = \"XX\"")
    print(f"\nConfig file location: {config_path}")
    print("-"*70)


def main():
    """Main entry point."""
    if len(sys.argv) < 2:
        print("Usage: python3 config_setup.py '<cookie_string>'")
        print("\nExample:")
        print("  python3 config_setup.py 'session-id=123; x-acbfr=abc; ...'")
        print("\nThis script performs the initial configuration setup for")
        print("Amazon Photos Uploader. It will:")
        print("  - Infer your country from cookie names")
        print("  - Set default values for user agent and zone")
        print("  - Create the configuration file")
        sys.exit(1)

    cookie_string = sys.argv[1]

    # Parse cookies
    cookies = parse_cookies(cookie_string)

    if not cookies:
        print("Error: No cookies found in input", file=sys.stderr)
        sys.exit(1)

    print("Parsed cookies successfully.")
    print(f"Found {len(cookies)} cookie(s).")

    # Infer country from cookies
    country = infer_country_from_cookies(cookies)
    print(f"\nInferred country: {country}")

    # Extract required cookies
    extracted = extract_required_cookies(cookies)

    if not extracted:
        print("Error: No required cookies found in input", file=sys.stderr)
        print("\nRequired cookies (with country suffix):", file=sys.stderr)
        print("  - session-id", file=sys.stderr)
        print("  - x-acb{country}", file=sys.stderr)
        print("  - at-acb{country}", file=sys.stderr)
        print("  - ubid-acb{country}", file=sys.stderr)
        print("  - sess-at-acb{country}", file=sys.stderr)
        sys.exit(1)

    # Create default config with inferred country
    config = create_default_config(country)

    # Update with extracted cookies
    config.update(extracted)

    # Check if config already exists
    config_path = get_config_path()
    if config_path.exists():
        print(f"\nWarning: Configuration file already exists at:", file=sys.stderr)
        print(f"  {config_path}", file=sys.stderr)
        response = input("\nOverwrite existing configuration? [y/N]: ")
        if response.lower() not in ['y', 'yes']:
            print("Setup cancelled.")
            sys.exit(0)

    # Save config
    saved_path = save_config(config)

    # Print summary with helpful hints
    print_setup_summary(saved_path, country, config['zone'])

    print("\nExtracted cookies:")
    for key in extracted.keys():
        print(f"  ✓ {key}")


if __name__ == "__main__":
    main()
