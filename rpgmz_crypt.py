#!/usr/bin/env python3
"""
RPG Maker MZ Data File Encrypt/Decrypt Tool

Decrypts or encrypts RPG Maker MZ data files using the same algorithm
as the game engine.  Also supports one-click restoration: decrypt all data
files and patch the engine so the game runs directly with plain JSON.

Algorithm: XOR stream cipher running backwards, with plaintext feedback.
Key derived from filename hash + constant 247 (sqrt(61009)).

Usage:
    python rpgmz_crypt.py decrypt <input_dir> <output_dir> [--pretty]
    python rpgmz_crypt.py encrypt <input_dir> <output_dir>
    python rpgmz_crypt.py decrypt-file <input.json> <output.json> [--pretty]
    python rpgmz_crypt.py encrypt-file <input.json> <output.json>
    python rpgmz_crypt.py restore <game_dir>
    python rpgmz_crypt.py revert <game_dir>
    python rpgmz_crypt.py patch-js <game_dir>
"""

import sys
import os
import json
import base64
import re
import shutil
import argparse
from pathlib import Path
from dataclasses import dataclass


# ── Encryption parameters (version-dependent) ────────────────────────────────

@dataclass
class CryptoParams:
    """Encryption constants extracted from rmmz_managers.js.

    Different RPG Maker MZ builds use different constants for the XOR
    stream cipher. The obfuscation tool can vary both the constants and
    the shift counts inside the feedback transform, so we extract the
    full parameter set from the engine rather than hard-coding it.
    """
    k_value: int            # window._K = Math.sqrt(N) | 0
    xor_c: int              # fk XOR xor_c → _c
    left_shift_p: int       # ls << left_shift_p → contributes to _p
    right_shift_p: int      # ls >>> right_shift_p → contributes to _p
    xor_k: int              # (_c + _m + _p) XOR xor_k
    add_k: int              # (… XOR xor_k) + add_k
    lowercase_filename: bool  # whether n ends with .toLowerCase()

    # Default matches RPG Maker MZ v1.9.x
    @classmethod
    def default(cls) -> "CryptoParams":
        return cls(k_value=247, xor_c=82, left_shift_p=2, right_shift_p=4,
                   xor_k=146, add_k=46, lowercase_filename=True)


def extract_params_from_js(js_path: str) -> CryptoParams:
    """Parse encryption constants from rmmz_managers.js.

    Instead of searching each constant independently, parse the
    decryption loop as one unit so we don't accidentally mix values from
    unrelated obfuscated expressions elsewhere in the file.
    """
    with open(js_path, "r", encoding="utf-8") as f:
        content = f.read()

    # ── window._K = (Math.sqrt(43681)|0) ──
    m = re.search(r"window\._K\s*=\s*\(Math\.sqrt\((\d+)\)\|0\)", content)
    if not m:
        raise ValueError(f"Cannot find window._K assignment in {js_path}")
    k_value = int(m.group(1)) ** 0.5
    if not k_value.is_integer():
        raise ValueError(f"Math.sqrt({m.group(1)}) is not a perfect square")
    k_value = int(k_value)

    # Parse filename normalisation from the same onXhrLoad block.
    m = re.search(
        r"var\s+n\s*=\s*src\.split\(/.*?/\)\.pop\(\)\.replace\('\.json',\s*''\)(\.toLowerCase\(\))?",
        content,
    )
    if not m:
        raise ValueError(f"Cannot find filename normalisation pattern in {js_path}")
    lowercase_filename = bool(m.group(1))

    # Parse the full decryption loop so all constants come from the same block.
    m = re.search(
        r"var\s+_c\s*=\s*\(fk\|(\d+)\)&~\(fk&\1\),\s*"
        r"_m\s*=\s*\(i%128\),\s*"
        r"_p\s*=\s*\(\(ls<<(\d+)\)\|\(ls>>>(\d+)\)\)&~\(\(ls<<\2\)&\(ls>>>\3\)\);\s*"
        r"var\s+_k\s*=\s*\(\(\(\(_c\+_m\+_p\)\|(\d+)\)&~\(\(\(_c\+_m\+_p\)&\4\)\)\)\+(\d+)\)&255;",
        content,
    )
    if not m:
        raise ValueError(f"Cannot find decryption loop pattern in {js_path}")

    xor_c = int(m.group(1))
    left_shift_p = int(m.group(2))
    right_shift_p = int(m.group(3))
    xor_k = int(m.group(4))
    add_k = int(m.group(5))

    return CryptoParams(
        k_value=k_value,
        xor_c=xor_c,
        left_shift_p=left_shift_p,
        right_shift_p=right_shift_p,
        xor_k=xor_k,
        add_k=add_k,
        lowercase_filename=lowercase_filename,
    )

# ── JS 32-bit integer emulation ────────────────────────────────────────────

def js_signed32(v: int) -> int:
    """Emulate JavaScript |0 — truncate to signed 32-bit integer."""
    v = v & 0xFFFFFFFF
    if v >= 0x80000000:
        v -= 0x100000000
    return v


def js_ushift_r(v: int, n: int) -> int:
    """Emulate JavaScript >>> (unsigned right shift)."""
    return (v & 0xFFFFFFFF) >> n


def js_xor(a: int, b: int) -> int:
    """
    Emulate JavaScript XOR expression (a|b)&~(a&b) in 32-bit signed.
    Matches the obfuscated pattern used in the engine's decryption code.
    """
    return js_signed32((a | b) & ~(a & b))


def compute_k(ls: int, i: int, fk: int, params: CryptoParams) -> int:
    """
    Compute the key byte for position i.

    Exact replication of the engine's obfuscated computation:
        _c = fk XOR params.xor_c
        _m = i % 128
        _p = (ls<<params.left_shift_p) XOR (ls>>>params.right_shift_p)
        _k = (((_c+_m+_p) XOR params.xor_k) + params.add_k) & 255
    """
    _c = js_xor(fk, params.xor_c)
    _m = i % 128
    _p = js_xor(ls << params.left_shift_p, js_ushift_r(ls, params.right_shift_p))
    return (js_xor(_c + _m + _p, params.xor_k) + params.add_k) & 255


def filename_hash(name: str) -> int:
    """
    Compute JS-style string hash: t = ((t << 5) - t + charCode) | 0

    This is Java's standard string hash adopted by many JS engines.
    """
    t = 0
    for ch in name:
        t = js_signed32((t << 5) - t + ord(ch))
    return t


def get_fk(filename: str, params: CryptoParams) -> int:
    """
    Derive the initial feedback key from the filename.

    The engine extracts the base name, removes '.json', optionally
    lowercases it, then computes a hash and XORs with _K.
    """
    basename = Path(filename).stem
    if params.lowercase_filename:
        basename = basename.lower()
    t = filename_hash(basename)
    return js_xor(params.k_value, t & 255) & 0xFF


# ── Core crypto ────────────────────────────────────────────────────────────

def decrypt(ciphertext: bytes, filename: str, params: CryptoParams) -> bytes:
    """
    Decrypt RPG Maker MZ encrypted data.

    Runs backwards through the data.  The key for position i depends on
    the *plaintext* byte at position i+1 (already recovered), creating
    a backward-chaining stream cipher.
    """
    fk = get_fk(filename, params)
    result = bytearray(len(ciphertext))
    ls = fk
    for i in range(len(ciphertext) - 1, -1, -1):
        _k = compute_k(ls, i, fk, params)
        result[i] = js_xor(ciphertext[i], _k) & 0xFF
        ls = result[i]  # plaintext feedback — matches what encryption used
    return bytes(result)


def encrypt(plaintext: bytes, filename: str, params: CryptoParams) -> bytes:
    """
    Encrypt data into RPG Maker MZ format.

    Mirror of decrypt(): runs backwards, key for position i depends on
    plaintext[i+1].  Because both functions use the same plaintext in the
    feedback chain, they produce and consume the same keystream.
    """
    fk = get_fk(filename, params)
    result = bytearray(len(plaintext))
    ls = fk
    for i in range(len(plaintext) - 1, -1, -1):
        _k = compute_k(ls, i, fk, params)
        result[i] = js_xor(plaintext[i], _k) & 0xFF
        ls = plaintext[i]  # plaintext feedback, not ciphertext
    return bytes(result)


# ── File-level operations ──────────────────────────────────────────────────

def decrypt_file(input_path: str, output_path: str, params: CryptoParams,
                 pretty: bool = False) -> None:
    """Decrypt a single .json data file."""
    with open(input_path, "r", encoding="utf-8") as f:
        wrapper = json.load(f)

    if "data" not in wrapper:
        raise ValueError(
            f"{input_path}: not an encrypted RPG Maker MZ data file "
            f"(missing 'data' field)"
        )

    ciphertext = base64.b64decode(wrapper["data"])
    plaintext_bytes = decrypt(ciphertext, os.path.basename(input_path), params)
    text = plaintext_bytes.decode("utf-8")

    if text.startswith("﻿"):
        text = text[1:]

    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)

    if pretty:
        parsed = json.loads(text)
        with open(output_path, "w", encoding="utf-8") as f:
            json.dump(parsed, f, ensure_ascii=False, indent=2)
    else:
        with open(output_path, "w", encoding="utf-8", newline="") as f:
            f.write(text)


def encrypt_file(input_path: str, output_path: str, params: CryptoParams) -> None:
    """Encrypt a single .json file into RPG Maker MZ format."""
    with open(input_path, "r", encoding="utf-8") as f:
        text = f.read()

    if text.startswith("﻿"):
        text = text[1:]

    plaintext = text.encode("utf-8")
    ciphertext = encrypt(plaintext, os.path.basename(output_path), params)
    data_b64 = base64.b64encode(ciphertext).decode("ascii")

    wrapper = {
        "uid": "",
        "bid": "1.9.0",
        "data": data_b64,
    }

    os.makedirs(os.path.dirname(output_path) or ".", exist_ok=True)
    with open(output_path, "w", encoding="utf-8") as f:
        json.dump(wrapper, f, ensure_ascii=False)


def process_directory(
    input_dir: str,
    output_dir: str,
    mode: str,
    params: CryptoParams,
    pretty: bool = False,
) -> list[str]:
    """Process all .json files in a directory."""
    input_path = Path(input_dir)
    output_path = Path(output_dir)
    output_path.mkdir(parents=True, exist_ok=True)

    json_files = sorted(input_path.glob("*.json"))
    processed = []

    for src in json_files:
        dst = output_path / src.name
        try:
            if mode == "decrypt":
                decrypt_file(str(src), str(dst), params, pretty=pretty)
            else:
                encrypt_file(str(src), str(dst), params)
            processed.append(src.name)
        except Exception as e:
            print(f"  ERROR processing {src.name}: {e}", file=sys.stderr)

    return processed


# ── JS engine patching ─────────────────────────────────────────────────────

# The rmmz_managers.js file to patch
MANAGERS_JS = "js/rmmz_managers.js"
MANAGERS_JS_BAK = "js/rmmz_managers.js.bak"
DATA_BAK = "data.encrypted"

# Two known engine variants for onXhrLoad.  The obfuscation tool produces
# slightly different code structures across engine builds.
#
# Variant A (v1.9.x): no early-return guard — decryption always runs.
#   var b = Buffer.from(c.data, 'base64');
#   ... decrypt ...
#   window[name] = JSON.parse(b.toString('utf8')...);
#   _t.onLoad(window[name]);
#   → Patch adds if(c.bid){...}else{...} wrapper.
#
# Variant B (v1.3.x): has early-return guard using !c.data.
#   if(!c.data) { window[name] = c; return _t.onLoad(window[name]); }
#   var b = Buffer.from(...), n = ..., t = 0;
#   ... decrypt ...
#   window[name] = JSON.parse(b.toString('utf8')...);
#   _t.onLoad(window[name]);
#   → Patch: flip !c.data → !c.bid (the early-return already handles plain).
#
# We try variant A first (more common); fall back to variant B.

PATCH_SET_A = [
    (
        "var b = Buffer.from(c.data, 'base64');",
        "if(c.bid){var b = Buffer.from(c.data, 'base64');",
    ),
    (
        "window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));   _t.onLoad(window[name]);",
        "window[name] = JSON.parse(b.toString('utf8').replace(/^\\uFEFF/, ''));}else{window[name] = c;}   _t.onLoad(window[name]);",
    ),
]

PATCH_SET_B = [
    (
        "if(!c.data) { window[name] = c; return _t.onLoad(window[name]); }",
        "if(!c.bid) { window[name] = c; return _t.onLoad(window[name]); }",
    ),
]


def patch_managers_js(game_dir: str) -> bool:
    """
    Patch DataManager.onXhrLoad to support both encrypted and plain JSON files.

    Returns True if patched, False if already patched.
    """
    js_path = os.path.join(game_dir, MANAGERS_JS)

    if not os.path.isfile(js_path):
        print(f"ERROR: {js_path} not found — is this an RPG Maker MZ game?", file=sys.stderr)
        sys.exit(1)

    with open(js_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Check if already patched (either variant)
    if "if(c.bid)" in content:
        print("  JS already patched (plain JSON support detected).")
        return False

    # Try variant A patterns first
    if all(old in content for old, _new in PATCH_SET_A):
        for old, new in PATCH_SET_A:
            content = content.replace(old, new)
    elif all(old in content for old, _new in PATCH_SET_B):
        for old, new in PATCH_SET_B:
            content = content.replace(old, new)
    else:
        print(f"  ERROR: unrecognized engine pattern in {MANAGERS_JS}", file=sys.stderr)
        print(f"         This game may use an unsupported engine version.", file=sys.stderr)
        sys.exit(1)

    # Write patched file
    with open(js_path, "w", encoding="utf-8") as f:
        f.write(content)

    return True


# ── High-level commands ────────────────────────────────────────────────────

def cmd_restore(game_dir: str) -> None:
    """
    One-click restore: decrypt all data files in place + patch JS engine.

    Creates backups (data.encrypted/, js/rmmz_managers.js.bak) so the
    operation can be undone with `revert`.
    """
    game = Path(game_dir)
    data_dir = game / "data"
    data_bak = game / DATA_BAK
    js_file = game / MANAGERS_JS
    js_bak = game / MANAGERS_JS_BAK

    # Validate
    if not data_dir.is_dir():
        print(f"ERROR: {data_dir} not found — is this an RPG Maker MZ game?", file=sys.stderr)
        sys.exit(1)
    if not js_file.is_file():
        print(f"ERROR: {js_file} not found — is this an RPG Maker MZ game?", file=sys.stderr)
        sys.exit(1)

    # Extract encryption parameters from the engine BEFORE moving data
    params = extract_params_from_js(str(js_file))

    # Check for existing backups (don't overwrite)
    if data_bak.exists():
        print(f"ERROR: backup already exists at {data_bak}/", file=sys.stderr)
        print("       Run 'revert' first if you want to undo a previous restore.", file=sys.stderr)
        sys.exit(1)

    print("=" * 60)
    print("RPG Maker MZ — One-Click Restore")
    print("=" * 60)
    print(f"Game directory: {game.resolve()}")
    print(f"Detected params: _K={params.k_value}, xor_c={params.xor_c}, "
          f"left_shift_p={params.left_shift_p}, right_shift_p={params.right_shift_p}, "
          f"xor_k={params.xor_k}, add_k={params.add_k}, "
          f"lowercase_filename={params.lowercase_filename}")
    print()

    # Step 1: Backup data directory
    print("[1/3] Backing up encrypted data/ ...")
    shutil.move(str(data_dir), str(data_bak))
    print(f"  → {data_bak.name}/ ({len(list(data_bak.glob('*.json')))} files)")

    # Step 2: Decrypt all data files in place
    print("[2/3] Decrypting data files ...")
    os.makedirs(str(data_dir))
    processed = process_directory(str(data_bak), str(data_dir), "decrypt", params)
    print(f"  → {len(processed)} files decrypted")

    # Step 3: Backup and patch JS
    print("[3/3] Patching JS engine ...")
    shutil.copy2(str(js_file), str(js_bak))
    print(f"  → backup: {js_bak.name}")

    patched = patch_managers_js(game_dir)
    if patched:
        print("  → rmmz_managers.js patched: plain JSON support enabled")

    print()
    print("Done! The game now runs with decrypted (editable) data files.")
    print(f"  Encrypted backup: {data_bak.name}/")
    print(f"  JS backup:        {js_bak.name}")
    print()
    print("To undo, run:  python rpgmz_crypt.py revert " + game_dir)


def cmd_revert(game_dir: str) -> None:
    """Undo a previous restore operation."""
    game = Path(game_dir)
    data_dir = game / "data"
    data_bak = game / DATA_BAK
    js_file = game / MANAGERS_JS
    js_bak = game / MANAGERS_JS_BAK

    if not data_bak.is_dir() and not js_bak.is_file():
        print("ERROR: no backups found. Nothing to revert.", file=sys.stderr)
        sys.exit(1)

    print("Reverting restore...")

    if data_bak.is_dir():
        if data_dir.exists():
            shutil.rmtree(str(data_dir))
        shutil.move(str(data_bak), str(data_dir))
        print(f"  → data/ restored ({len(list(data_dir.glob('*.json')))} files)")

    if js_bak.is_file():
        shutil.copy2(str(js_bak), str(js_file))
        os.remove(str(js_bak))
        print(f"  → {MANAGERS_JS} restored")

    print("Revert complete. Game is back to its original (encrypted) state.")


def cmd_patch_js(game_dir: str) -> None:
    """Patch only the JS engine, without touching data files."""
    game = Path(game_dir)
    js_file = game / MANAGERS_JS
    js_bak = game / MANAGERS_JS_BAK

    if not js_file.is_file():
        print(f"ERROR: {js_file} not found.", file=sys.stderr)
        sys.exit(1)

    if js_bak.exists():
        print(f"Note: backup already exists at {js_bak} (not overwriting)")

    print("Patching JS engine...")
    shutil.copy2(str(js_file), str(js_bak))
    print(f"  → backup: {js_bak.name}")

    patched = patch_managers_js(game_dir)
    if patched:
        print("  → rmmz_managers.js patched successfully")
    print()
    print("The engine now accepts both encrypted and plain JSON data files.")


# ── CLI ────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="RPG Maker MZ Data File Encrypt/Decrypt Tool"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # ── decrypt ──
    dec = subparsers.add_parser(
        "decrypt",
        help="Decrypt all .json data files in a directory",
        description="Decrypt encrypted RPG Maker MZ data files to plain JSON.",
    )
    dec.add_argument("input_dir", help="Directory containing encrypted .json files")
    dec.add_argument("output_dir", help="Directory to write decrypted .json files")
    dec.add_argument(
        "--pretty", action="store_true",
        help="Pretty-print JSON with indent=2 (default: preserve raw format)",
    )
    dec.add_argument(
        "--game", dest="game_dir", default=None,
        help="Game root directory for auto-detecting encryption params (default: use v1.9.x)",
    )

    # ── encrypt ──
    enc = subparsers.add_parser(
        "encrypt",
        help="Encrypt all .json files in a directory",
        description="Encrypt plain JSON files back to RPG Maker MZ format.",
    )
    enc.add_argument("input_dir", help="Directory containing plain .json files")
    enc.add_argument("output_dir", help="Directory to write encrypted .json files")
    enc.add_argument(
        "--game", dest="game_dir", default=None,
        help="Game root directory for auto-detecting encryption params (default: use v1.9.x)",
    )

    # ── decrypt-file ──
    dec_f = subparsers.add_parser(
        "decrypt-file",
        help="Decrypt a single file",
        description="Decrypt a single RPG Maker MZ data file.",
    )
    dec_f.add_argument("input", help="Encrypted .json file")
    dec_f.add_argument("output", help="Output path for decrypted .json")
    dec_f.add_argument(
        "--pretty", action="store_true",
        help="Pretty-print JSON with indent=2 (default: preserve raw format)",
    )
    dec_f.add_argument(
        "--game", dest="game_dir", default=None,
        help="Game root directory for auto-detecting encryption params (default: use v1.9.x)",
    )

    # ── encrypt-file ──
    enc_f = subparsers.add_parser(
        "encrypt-file",
        help="Encrypt a single file",
        description="Encrypt a single JSON file back to RPG Maker MZ format.",
    )
    enc_f.add_argument("input", help="Plain .json file")
    enc_f.add_argument("output", help="Output path for encrypted .json")
    enc_f.add_argument(
        "--game", dest="game_dir", default=None,
        help="Game root directory for auto-detecting encryption params (default: use v1.9.x)",
    )

    # ── restore ──
    restore = subparsers.add_parser(
        "restore",
        help="One-click: decrypt all data + patch JS so the game runs with plain JSON",
        description=(
            "Decrypt all data/*.json files in place and patch rmmz_managers.js "
            "so the engine can read plain JSON directly.  Creates backups "
            "(data.encrypted/, js/rmmz_managers.js.bak) for safe undo.\n\n"
            "After this, you can edit data files directly and the game will "
            "run without needing to re-encrypt."
        ),
    )
    restore.add_argument(
        "game_dir",
        help="Root directory of the RPG Maker MZ game (contains data/ and js/)",
    )

    # ── revert ──
    revert = subparsers.add_parser(
        "revert",
        help="Undo a previous 'restore' — re-encrypt data and restore original JS",
        description="Restore data/ and rmmz_managers.js from backups.",
    )
    revert.add_argument(
        "game_dir",
        help="Root directory of the RPG Maker MZ game (contains data/ and js/)",
    )

    # ── patch-js ──
    pjs = subparsers.add_parser(
        "patch-js",
        help="Patch only the JS engine to support plain JSON (without touching data)",
        description=(
            "Modify rmmz_managers.js so it accepts both encrypted and plain "
            "JSON data files.  The data files themselves are left unchanged."
        ),
    )
    pjs.add_argument(
        "game_dir",
        help="Root directory of the RPG Maker MZ game (contains data/ and js/)",
    )

    args = parser.parse_args()

    # Resolve encryption params: --game flag > auto-detect via game_dir arg > v1.9.x default
    def _resolve_params(game_dir: str | None) -> CryptoParams:
        if game_dir:
            js = os.path.join(game_dir, MANAGERS_JS)
            if os.path.isfile(js):
                p = extract_params_from_js(js)
                print(f"Using params from {js}: _K={p.k_value}, xor_c={p.xor_c}, "
                      f"left_shift_p={p.left_shift_p}, right_shift_p={p.right_shift_p}, "
                      f"xor_k={p.xor_k}, add_k={p.add_k}, "
                      f"lowercase_filename={p.lowercase_filename}")
                return p
            print(f"WARNING: {js} not found, using v1.9.x defaults", file=sys.stderr)
        return CryptoParams.default()

    if args.command in ("decrypt", "encrypt"):
        params = _resolve_params(getattr(args, "game_dir", None))
        pretty = getattr(args, "pretty", False)
        processed = process_directory(args.input_dir, args.output_dir,
                                      args.command, params, pretty=pretty)
        print(f"{args.command.capitalize()}ed {len(processed)} files:")
        for name in processed:
            print(f"  {name}")

    elif args.command == "decrypt-file":
        params = _resolve_params(getattr(args, "game_dir", None))
        pretty = getattr(args, "pretty", False)
        decrypt_file(args.input, args.output, params, pretty=pretty)
        print(f"Decrypted: {args.input} → {args.output}")

    elif args.command == "encrypt-file":
        params = _resolve_params(getattr(args, "game_dir", None))
        encrypt_file(args.input, args.output, params)
        print(f"Encrypted: {args.input} → {args.output}")

    elif args.command == "restore":
        cmd_restore(args.game_dir)

    elif args.command == "revert":
        cmd_revert(args.game_dir)

    elif args.command == "patch-js":
        cmd_patch_js(args.game_dir)


if __name__ == "__main__":
    main()
