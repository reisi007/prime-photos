import builtins
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("config_update.py")
SCRIPT_DIR = str(MODULE_PATH.resolve().parent)
if SCRIPT_DIR not in sys.path:
    # These scripts are loaded by path below, so add their sibling-module
    # directory explicitly for the shared serializer import.
    sys.path.insert(0, SCRIPT_DIR)

SPEC = importlib.util.spec_from_file_location("config_update", MODULE_PATH)
config_update = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(config_update)

SETUP_MODULE_PATH = Path(__file__).with_name("config_setup.py")
SETUP_SPEC = importlib.util.spec_from_file_location("config_setup", SETUP_MODULE_PATH)
config_setup = importlib.util.module_from_spec(SETUP_SPEC)
SETUP_SPEC.loader.exec_module(config_setup)


class ConfigUpdateTests(unittest.TestCase):
    def test_fallback_loader_preserves_numeric_queue_ratio(self):
        config = config_update.parse_config_without_toml(
            'country = "de"\nbackup_queue_ratio = 0.63\n',
            {"country": "us", "backup_queue_ratio": 0.35},
        )

        self.assertEqual(config["country"], "de")
        self.assertEqual(config["backup_queue_ratio"], 0.63)

    def test_cookie_update_fallback_preserves_appearance_without_toml_modules(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "config.toml"
            path.write_text(' appearance = "dark"\n')
            original_import = builtins.__import__

            def import_without_toml(name, *args, **kwargs):
                if name in ("tomllib", "tomli"):
                    raise ImportError("TOML parser deliberately unavailable")
                return original_import(name, *args, **kwargs)

            with patch.object(config_update, "get_config_path", return_value=path):
                with patch("builtins.__import__", side_effect=import_without_toml):
                    config = config_update.load_config()

                config.update({"cookie_x_acb": "updated-cookie"})
                config_update.save_config(config)

            saved = path.read_text()
            self.assertIn('appearance = "dark"', saved)
            self.assertIn('cookie_x_acb = "updated-cookie"', saved)

    def test_save_keeps_queue_ratio_numeric_in_toml(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "config.toml"
            with patch.object(config_update, "get_config_path", return_value=path):
                config_update.save_config(
                    {"country": "de", "backup_queue_ratio": 0.63}
                )

            content = path.read_text()
            self.assertIn('country = "de"', content)
            self.assertIn("backup_queue_ratio = 0.63", content)

    def test_initial_setup_writes_the_numeric_queue_ratio_default(self):
        config = config_setup.create_default_config("de")
        self.assertEqual(config["backup_queue_ratio"], 0.35)

        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "config.toml"
            with patch.object(config_setup, "get_config_path", return_value=path):
                config_setup.save_config(config)

            self.assertIn("backup_queue_ratio = 0.35", path.read_text())

    def test_both_scripts_use_the_same_toml_scalar_helper(self):
        self.assertIs(config_update.toml_scalar, config_setup.toml_scalar)
        self.assertEqual(config_update.toml_scalar.__module__, "config_toml")

    def test_toml_scalar_keeps_types_and_nested_containers(self):
        self.assertEqual(config_update.toml_scalar(True), "true")
        self.assertEqual(config_update.toml_scalar(False), "false")
        self.assertEqual(config_update.toml_scalar(3), "3")
        self.assertEqual(config_update.toml_scalar(0.5), "0.5")
        self.assertEqual(config_update.toml_scalar(float("nan")), "nan")
        self.assertEqual(config_update.toml_scalar(float("inf")), "inf")
        self.assertEqual(config_update.toml_scalar(float("-inf")), "-inf")
        self.assertEqual(config_update.toml_scalar('a"b\\c'), '"a\\"b\\\\c"')
        self.assertEqual(config_update.toml_scalar([1, "x"]), '[1, "x"]')
        self.assertEqual(config_update.toml_scalar((1, "x")), '[1, "x"]')
        self.assertEqual(
            config_update.toml_scalar({"a\"b\\c": (1, {"inner": ["x", True]})}),
            '{ "a\\"b\\\\c" = [1, { "inner" = ["x", true] }] }',
        )
        self.assertEqual(config_update.toml_scalar([]), "[]")
        self.assertEqual(config_update.toml_scalar(()), "[]")
        self.assertEqual(config_update.toml_scalar({}), "{}")
        with self.assertRaisesRegex(TypeError, "Unsupported TOML config value: set"):
            config_update.toml_scalar({1, 2})

    def test_save_keeps_bool_int_and_list_types(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "config.toml"
            with patch.object(config_update, "get_config_path", return_value=path):
                config_update.save_config(
                    {
                        "country": "de",
                        "paused": True,
                        "concurrency": 4,
                        "tags": ["a", "b"],
                    }
                )

            content = path.read_text()
            self.assertIn("paused = true", content)
            self.assertIn("concurrency = 4", content)
            self.assertIn('tags = ["a", "b"]', content)
            self.assertIn('country = "de"', content)

    def test_initial_setup_serializes_scalar_types(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "config.toml"
            with patch.object(config_setup, "get_config_path", return_value=path):
                config_setup.save_config(
                    {
                        "country": "de",
                        "enabled": True,
                        "workers": 3,
                        "ratio": 0.35,
                    }
                )

            content = path.read_text()
            self.assertIn("enabled = true", content)
            self.assertIn("workers = 3", content)
            self.assertIn("ratio = 0.35", content)
            self.assertIn('country = "de"', content)


if __name__ == "__main__":
    unittest.main()
