# RPG Maker MZ / MV-Custom Data JSON Encrypt/Decrypt Tool

[简体中文说明 / Simplified Chinese](./README.zh-CN.md)

This tool targets the RPG Maker game family where `data/*.json` files are wrapped as `{"uid","bid","data"}` and decrypted inside the manager script at load time.

## Supported game families

Supported and tested families:

- RPG Maker MZ games that load encrypted JSON data through `js/rmmz_managers.js`
- Custom RPG Maker MV games that were modified to load the same `uid/bid/data` JSON wrapper through `js/rpg_managers.js`

This is different from stock RPG Maker MV. Stock MV uses `Decrypter` for images and audio, while `data/*.json` is usually plain JSON and does not use this MZ-style data wrapper. In other words, support depends on the JSON data loader logic in `js/*managers.js`, not just on whether the game is called MZ or MV.

## What `/path/to/game` must contain

A usable game root must contain `data/` plus the matching manager script for the engine family:

- MZ root: `data/` and `js/rmmz_managers.js`
- MV-custom root: `data/` and `js/rpg_managers.js`

Examples:

```text
/path/to/game/
├── data/
└── js/
    └── rmmz_managers.js
```

```text
/path/to/game/
├── data/
└── js/
    └── rpg_managers.js
```

## Auto-detection and `--game`

Conversion commands try to auto-detect the game root first.

- If your input and output paths are inside the game tree, auto-detection usually works.
- If your input or output paths are outside the game tree, pass `--game /path/to/game`.
- You can also pass `--game` when auto-detection works but you want to override the detected root explicitly.

Current behavior is: auto-detect first, and only require `--game` when detection fails or when you want to override detection.

## Tool names and compatibility

- Primary Python entry point: `rpgdata_crypt.py`
- Backward-compatible Python alias: `rpgmz_crypt.py`
- Rust binary name: `rpgdata_crypt`
- Rust source directory: `rpgmz_crypt_rs/`

The repository directory still keeps the historical `rpgmz_crypt_rs/` folder name, but the user-facing tool name is now `rpgdata_crypt`.

## How the supported JSON encryption family works

### 1. Wrapped JSON file format

Supported games store each encrypted data file as a JSON wrapper that looks like this:

```json
{
  "uid": "...",
  "bid": "...",
  "data": "base64 ciphertext"
}
```

- `uid` is an identifier carried by the game but not used for key derivation
- `bid` is useful as an encrypted/plain marker once the manager script is patched
- `data` is the actual base64-encoded ciphertext payload

### 2. Parameters come from the game engine, not from the file itself

The tool reads the decryption parameters from the manager script in the game root:

- MZ: `js/rmmz_managers.js`
- MV-custom: `js/rpg_managers.js`

This is why auto-detection and `--game` matter. The encrypted JSON file alone does not tell the tool which `_K`, XOR constants, shift counts, or filename normalization rule the game expects.

### 3. Filename binding is part of the key derivation

The filename stem (for example `Map002` from `Map002.json`) participates in the key schedule. In some games the stem is lowercased before hashing; in others it is not. Renaming files before encrypting them back changes the derived key and produces unusable output.

### 4. Decryption runs backwards with plaintext feedback

The supported family uses a reverse XOR stream:

- iterate from the end of the file to the beginning
- derive a position-aware byte key from `_K`, filename hash, shift/XOR constants, and the previous plaintext byte
- XOR the ciphertext byte with that key

This is why the tool must replicate the game’s exact JS-style integer behavior instead of using a simplified ad-hoc transform.

### 5. What `restore` actually changes

`restore` does two things in addition to decrypting `data/*.json`:

1. backs up the original encrypted `data/` into `data.encrypted/`
2. patches the manager script so the loader behaves like:

```javascript
if (c.bid) {
    // wrapped encrypted JSON -> decrypt
} else {
    // plain JSON -> use directly
}
```

That patch is what allows you to edit plain JSON in-place after `restore` without re-encrypting after every change.

## Quick start

### MZ restore/revert flow

Use this when you want the game to run directly on plain JSON data:

```bash
python3 rpgdata_crypt.py restore /path/to/game
# edit data/*.json directly
python3 rpgdata_crypt.py revert /path/to/game
```

`restore` for MZ will:

1. Back up encrypted data into `data.encrypted/`
2. Decrypt `data/*.json` into plain JSON
3. Patch `js/rmmz_managers.js` so the game accepts both wrapped encrypted JSON and plain JSON

`revert` restores the original encrypted data and JS backup.

### MV-custom restore/revert flow

MV-custom games that use `js/rpg_managers.js` are supported by the same convenience flow:

```bash
python3 rpgdata_crypt.py restore /path/to/game
# edit data/*.json directly
python3 rpgdata_crypt.py revert /path/to/game
```

For MV-custom games, `restore` will:

1. Back up encrypted data into `data.encrypted/`
2. Decrypt `data/*.json` into plain JSON
3. Patch `js/rpg_managers.js` so the game accepts both wrapped encrypted JSON and plain JSON

`revert` restores the original encrypted data and JS backup.

The conversion workflow below is still available when you want a directory-level decrypt/encrypt round trip, especially if you prefer to keep working outside the game tree.

## Conversion workflow

```bash
# decrypt a directory
python3 rpgdata_crypt.py decrypt /path/to/game/data ./data_plain --pretty

# encrypt it back
python3 rpgdata_crypt.py encrypt ./data_plain /path/to/game/data

# explicit root when paths are outside the game tree
python3 rpgdata_crypt.py decrypt /encrypted/data ./data_plain --pretty --game /path/to/game
python3 rpgdata_crypt.py encrypt ./data_plain /encrypted/data --game /path/to/game
```

Single-file commands follow the same rule:

```bash
python3 rpgdata_crypt.py decrypt-file /path/to/game/data/Map002.json ./Map002.json --pretty
python3 rpgdata_crypt.py encrypt-file ./Map002.json /path/to/game/data/Map002.json
```

## Filename binding requirement

The crypto parameters are derived in part from the original filename. Output filenames must stay the original game filenames.

Examples:

- `Map002.json` must stay `Map002.json`
- `Actors.json` must stay `Actors.json`

Do not rename decrypted files to names like `Map002_edit.json` before encrypting them back. The filename binding is part of the key derivation, so renaming will produce unusable output.

## Wrong-parameter symptom

If you encrypt with the wrong parameters or point `--game` at the wrong root, a common runtime symptom is:

- `fail load MapXXX.json`

That usually means the game tried to load a file that was encrypted with mismatched parameters or loaded as plain JSON under the wrong flow.

## Command summary

```bash
python3 rpgdata_crypt.py decrypt <encrypted_dir> <output_dir> [--pretty] [--game /path/to/game]
python3 rpgdata_crypt.py encrypt <plain_dir> <output_dir> [--game /path/to/game]
python3 rpgdata_crypt.py decrypt-file <input.json> <output.json> [--pretty] [--game /path/to/game]
python3 rpgdata_crypt.py encrypt-file <input.json> <output.json> [--game /path/to/game]
python3 rpgdata_crypt.py restore /path/to/game
python3 rpgdata_crypt.py revert /path/to/game
python3 rpgdata_crypt.py patch-js /path/to/game
```

## Rust build artifacts

The Rust CLI crate is in `rpgmz_crypt_rs/` and builds the `rpgdata_crypt` binary.

Rolling release artifact names:

- Linux: `rpgdata_crypt-linux-x86_64`
- Windows: `rpgdata_crypt-windows-x86_64.exe`

Local default Cargo outputs remain:

- Linux local release binary: `target/release/rpgdata_crypt`
- Windows local release binary: `target/release/rpgdata_crypt.exe`

## Notes

- If you keep your work inside the game tree, auto-detection usually removes the need to pass `--game`.
- If you work outside the game tree, pass `--game /path/to/game`.
- Stock MV image/audio decryption is not the same thing as this JSON data workflow.
- The important distinction is whether the game’s manager script decrypts wrapped JSON data at load time.
