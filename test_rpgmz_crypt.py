import base64
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import rpgdata_crypt
from rpgdata_crypt import decrypt, extract_params_from_js


ROOT = Path(__file__).resolve().parent
GAME_DIR = ROOT / "game"
FIXTURES_DIR = ROOT / "tests" / "fixtures"
PYTHON_TOOL = ROOT / "rpgdata_crypt.py"
JS_PATH = GAME_DIR / "js" / "rpg_managers.js"
ACTORS_PATH = GAME_DIR / "data" / "Actors.json"
MZ_FIXTURE_JS = FIXTURES_DIR / "mz_rmmz_managers.js"
MV_FIXTURE_JS = FIXTURES_DIR / "mv_rpg_managers.js"
SAMPLE_ACTORS = [None, {"id": 1, "name": "Harold"}]


def _build_encrypted_actors_wrapper(filename: str, params: rpgdata_crypt.CryptoParams) -> dict:
    plaintext = json.dumps(SAMPLE_ACTORS, ensure_ascii=False).encode("utf-8")
    ciphertext = rpgdata_crypt.encrypt(plaintext, filename, params)
    return {
        "uid": "",
        "bid": "synthetic",
        "data": base64.b64encode(ciphertext).decode("ascii"),
    }


def _build_sample_game(game_root: Path, fixture_js: Path, manager_relpath: str) -> Path:
    (game_root / "data").mkdir(parents=True)
    (game_root / "js").mkdir(parents=True)
    shutil.copy2(fixture_js, game_root / manager_relpath)
    params = extract_params_from_js(str(fixture_js))
    wrapper = _build_encrypted_actors_wrapper("Actors.json", params)
    (game_root / "data" / "Actors.json").write_text(
        json.dumps(wrapper, ensure_ascii=False),
        encoding="utf-8",
    )
    return game_root


def build_sample_mz_game(game_root: Path) -> Path:
    return _build_sample_game(game_root, MZ_FIXTURE_JS, rpgdata_crypt.MZ_MANAGERS_JS)


def build_sample_mv_game(game_root: Path) -> Path:
    return _build_sample_game(game_root, MV_FIXTURE_JS, rpgdata_crypt.MV_MANAGERS_JS)


class CryptoParamExtractionTests(unittest.TestCase):
    def test_extracted_params_can_decrypt_real_actors_file(self):
        params = extract_params_from_js(str(JS_PATH))

        with ACTORS_PATH.open("r", encoding="utf-8") as f:
            wrapper = json.load(f)
        if not isinstance(wrapper, dict) or "data" not in wrapper:
            with (GAME_DIR / "data.encrypted" / "Actors.json").open("r", encoding="utf-8") as f:
                wrapper = json.load(f)

        plaintext = decrypt(base64.b64decode(wrapper["data"]), "Actors.json", params)
        text = plaintext.decode("utf-8")
        if text.startswith("﻿"):
            text = text[1:]

        data = json.loads(text)
        self.assertIsInstance(data, list)
        self.assertGreater(len(data), 1)
        self.assertIsNone(data[0])
        self.assertIsInstance(data[1], dict)

    def test_extract_mz_params_from_fixture(self):
        params = rpgdata_crypt.extract_mz_params_from_js(str(MZ_FIXTURE_JS))

        self.assertEqual(
            params,
            rpgdata_crypt.CryptoParams(
                k_value=247,
                xor_c=82,
                left_shift_p=2,
                right_shift_p=4,
                xor_k=146,
                add_k=46,
                lowercase_filename=True,
            ),
        )

    def test_extract_mv_params_from_fixture(self):
        params = rpgdata_crypt.extract_mv_params_from_js(str(MV_FIXTURE_JS))

        self.assertEqual(
            params,
            rpgdata_crypt.CryptoParams(
                k_value=152,
                xor_c=85,
                left_shift_p=2,
                right_shift_p=4,
                xor_k=180,
                add_k=36,
                lowercase_filename=False,
            ),
        )

    def test_cli_decrypt_file_auto_detects_mv_game_root(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = build_sample_mv_game(Path(tmpdir) / "game")

            output = Path(tmpdir) / "decrypted" / "Actors.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(PYTHON_TOOL),
                    "decrypt-file",
                    "data/Actors.json",
                    str(output),
                ],
                capture_output=True,
                text=True,
                cwd=str(game_root),
            )

            self.assertEqual(result.returncode, 0, msg=result.stderr + result.stdout)
            data = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(data, SAMPLE_ACTORS)


class PatchAndRestoreTests(unittest.TestCase):
    def test_patch_mz_manager_adds_plain_json_branch(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = build_sample_mz_game(Path(tmpdir) / "mz_game")
            js_path = game_root / rpgdata_crypt.MZ_MANAGERS_JS

            patched = rpgdata_crypt.patch_mz_managers_js(str(js_path))

            self.assertTrue(patched)
            content = js_path.read_text(encoding="utf-8")
            self.assertIn("if(c.bid){var b = Buffer.from(c.data, 'base64');", content)
            self.assertIn("}else{window[name] = c;}", content)

    def test_patch_mv_manager_adds_plain_json_branch(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = build_sample_mv_game(Path(tmpdir) / "mv_game")
            js_path = game_root / rpgdata_crypt.MV_MANAGERS_JS

            patched = rpgdata_crypt.patch_mv_managers_js(str(js_path))

            self.assertTrue(patched)
            content = js_path.read_text(encoding="utf-8")
            self.assertIn("if(c.bid){var b=Buffer.from(c.data,'base64');", content)
            self.assertIn("}else{window[name]=c;}", content)

    def test_patch_mv_manager_supports_escaped_ufeef_regex_variant(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = Path(tmpdir) / "mv_game"
            (game_root / "js").mkdir(parents=True)
            js_path = game_root / rpgdata_crypt.MV_MANAGERS_JS
            content = MV_FIXTURE_JS.read_text(encoding="utf-8").replace(
                "replace(/^﻿/, '')",
                "replace(/^\\uFEFF/, '')",
            )
            js_path.write_text(content, encoding="utf-8")

            patched = rpgdata_crypt.patch_mv_managers_js(str(js_path))

            self.assertTrue(patched)
            patched_content = js_path.read_text(encoding="utf-8")
            self.assertIn("if(c.bid){var b=Buffer.from(c.data,'base64');", patched_content)
            self.assertIn("}else{window[name]=c;}", patched_content)

    def test_patch_mv_manager_supports_real_world_escaped_ufeef_with_onload_variant(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = Path(tmpdir) / "mv_game"
            (game_root / "js").mkdir(parents=True)
            js_path = game_root / rpgdata_crypt.MV_MANAGERS_JS
            content = MV_FIXTURE_JS.read_text(encoding="utf-8").replace(
                "window[name]=JSON.parse(b.toString('utf8').replace(/^﻿/, ''));",
                "window[name]=JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));DataManager.onLoad(window[name]);",
            )
            js_path.write_text(content, encoding="utf-8")

            patched = rpgdata_crypt.patch_mv_managers_js(str(js_path))

            self.assertTrue(patched)
            patched_content = js_path.read_text(encoding="utf-8")
            self.assertIn("if(c.bid){var b=Buffer.from(c.data,'base64');", patched_content)
            self.assertIn("}else{window[name]=c;}DataManager.onLoad(window[name]);", patched_content)

    def test_restore_and_revert_work_for_mz_game(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = build_sample_mz_game(Path(tmpdir) / "mz_game")
            js_path = game_root / rpgdata_crypt.MZ_MANAGERS_JS
            js_original = js_path.read_text(encoding="utf-8")

            rpgdata_crypt.cmd_restore(str(game_root))

            self.assertTrue((game_root / rpgdata_crypt.DATA_BAK).is_dir())
            self.assertTrue((game_root / rpgdata_crypt.MZ_MANAGERS_JS_BAK).is_file())
            self.assertEqual(
                json.loads((game_root / "data" / "Actors.json").read_text(encoding="utf-8")),
                SAMPLE_ACTORS,
            )
            self.assertIn("if(c.bid){var b = Buffer.from(c.data, 'base64');", js_path.read_text(encoding="utf-8"))

            rpgdata_crypt.cmd_revert(str(game_root))

            self.assertFalse((game_root / rpgdata_crypt.DATA_BAK).exists())
            self.assertFalse((game_root / rpgdata_crypt.MZ_MANAGERS_JS_BAK).exists())
            self.assertIn("data", json.loads((game_root / "data" / "Actors.json").read_text(encoding="utf-8")))
            self.assertEqual(js_path.read_text(encoding="utf-8"), js_original)

    def test_restore_and_revert_work_for_mv_custom_game(self):
        with tempfile.TemporaryDirectory() as tmpdir:
            game_root = build_sample_mv_game(Path(tmpdir) / "mv_game")
            js_path = game_root / rpgdata_crypt.MV_MANAGERS_JS
            js_original = js_path.read_text(encoding="utf-8")

            rpgdata_crypt.cmd_restore(str(game_root))

            self.assertTrue((game_root / rpgdata_crypt.DATA_BAK).is_dir())
            self.assertTrue((game_root / rpgdata_crypt.MV_MANAGERS_JS_BAK).is_file())
            self.assertEqual(
                json.loads((game_root / "data" / "Actors.json").read_text(encoding="utf-8")),
                SAMPLE_ACTORS,
            )
            self.assertIn("if(c.bid){var b=Buffer.from(c.data,'base64');", js_path.read_text(encoding="utf-8"))

            rpgdata_crypt.cmd_revert(str(game_root))

            self.assertFalse((game_root / rpgdata_crypt.DATA_BAK).exists())
            self.assertFalse((game_root / rpgdata_crypt.MV_MANAGERS_JS_BAK).exists())
            self.assertIn("data", json.loads((game_root / "data" / "Actors.json").read_text(encoding="utf-8")))
            self.assertEqual(js_path.read_text(encoding="utf-8"), js_original)


if __name__ == "__main__":
    unittest.main()
