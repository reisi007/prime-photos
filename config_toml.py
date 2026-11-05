"""Shared TOML value serialization for the configuration scripts."""

import math


def toml_scalar(value) -> str:
    """Serialize config values as TOML scalars, arrays, or inline tables."""
    # bool must precede int because bool is a subclass of int in Python.
    if isinstance(value, bool):
        return 'true' if value else 'false'
    if isinstance(value, int):
        return str(value)
    if isinstance(value, float):
        if math.isnan(value):
            return 'nan'
        if math.isinf(value):
            return 'inf' if value > 0 else '-inf'
        return str(value)
    if isinstance(value, str):
        escaped = value.replace('\\', '\\\\').replace('"', '\\"')
        return f'"{escaped}"'
    if isinstance(value, (list, tuple)):
        return '[' + ', '.join(toml_scalar(item) for item in value) + ']'
    if isinstance(value, dict):
        entries = ', '.join(
            f'{toml_scalar(str(key))} = {toml_scalar(item)}'
            for key, item in value.items()
        )
        return '{ ' + entries + ' }' if entries else '{}'
    raise TypeError(f'Unsupported TOML config value: {type(value).__name__}')
