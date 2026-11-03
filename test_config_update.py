import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("config_update.py")
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


if __name__ == "__main__":
    unittest.main()
