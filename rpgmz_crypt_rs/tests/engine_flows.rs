use rpgdata_crypt::commands::{cmd_restore, cmd_revert, decrypt_file};
use rpgdata_crypt::detect::EngineKind;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

#[test]
fn auto_detects_mv_game_on_decrypt_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("game");
    let input = match serde_json::from_str::<Value>(&fs::read_to_string(root.join("data/Actors.json")).unwrap()) {
        Ok(Value::Object(map)) if map.contains_key("data") => root.join("data/Actors.json"),
        _ => root.join("data.encrypted/Actors.json"),
    };
    let temp = tempdir().unwrap();
    let output = temp.path().join("Actors.json");

    decrypt_file(&input, &output, false, None).unwrap();

    let text = fs::read_to_string(&output).unwrap();
    let parsed: Value = serde_json::from_str(&text).unwrap();
    assert!(parsed.is_array(), "expected decrypted Actors.json array");
}

#[test]
fn restore_and_revert_mz_game() {
    let temp = tempdir().unwrap();
    let game = SyntheticGame::new(temp.path(), EngineKind::Mz);

    cmd_restore(game.root()).unwrap();

    assert!(game.root().join("data.encrypted").is_dir());
    let decrypted = fs::read_to_string(game.root().join("data/Actors.json")).unwrap();
    assert!(!decrypted.contains("\"bid\""));
    assert!(game.manager_backup().is_file());
    assert!(fs::read_to_string(game.manager_path()).unwrap().contains("else{window[name] = c;}"));

    cmd_revert(game.root()).unwrap();

    assert!(!game.root().join("data.encrypted").exists());
    assert!(game.root().join("data/Actors.json").is_file());
    assert!(!game.manager_backup().exists());
    assert_eq!(fs::read_to_string(game.manager_path()).unwrap(), game.original_manager_js);
}

#[test]
fn restore_and_revert_mv_game() {
    let temp = tempdir().unwrap();
    let game = SyntheticGame::new(temp.path(), EngineKind::MvCustom);

    cmd_restore(game.root()).unwrap();

    assert!(game.root().join("data.encrypted").is_dir());
    let decrypted = fs::read_to_string(game.root().join("data/Actors.json")).unwrap();
    assert!(decrypted.contains("\"name\":\"Harold\""));
    assert!(game.manager_backup().is_file());
    let patched = fs::read_to_string(game.manager_path()).unwrap();
    assert!(patched.contains("else{window[name]=c;}"));
    assert!(patched.contains("if(c.bid){var b=Buffer.from(c.data,'base64');"));

    cmd_revert(game.root()).unwrap();

    assert!(!game.root().join("data.encrypted").exists());
    assert!(!game.manager_backup().exists());
    assert_eq!(fs::read_to_string(game.manager_path()).unwrap(), game.original_manager_js);
}

struct SyntheticGame {
    root: PathBuf,
    original_manager_js: String,
}

impl SyntheticGame {
    fn new(parent: &Path, engine: EngineKind) -> Self {
        let root = parent.join(match engine {
            EngineKind::Mz => "mz_game",
            EngineKind::MvCustom => "mv_game",
        });
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("js")).unwrap();

        let (manager_rel, manager_js) = manager_fixture(engine);
        fs::write(root.join(manager_rel), manager_js).unwrap();

        let plain = r#"[{"id":1,"name":"Harold"}]"#;
        let plain_path = root.join("Actors_plain.json");
        fs::write(&plain_path, plain).unwrap();
        let encrypted_path = root.join("data/Actors.json");
        decrypt_or_encrypt_fixture(engine, &plain_path, &encrypted_path);
        fs::remove_file(plain_path).unwrap();

        Self {
            root,
            original_manager_js: manager_js.to_string(),
        }
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn manager_path(&self) -> PathBuf {
        if self.root.join("js/rmmz_managers.js").exists() {
            self.root.join("js/rmmz_managers.js")
        } else {
            self.root.join("js/rpg_managers.js")
        }
    }

    fn manager_backup(&self) -> PathBuf {
        if self.root.join("js/rmmz_managers.js").exists() {
            self.root.join("js/rmmz_managers.js.bak")
        } else {
            self.root.join("js/rpg_managers.js.bak")
        }
    }
}

fn manager_fixture(engine: EngineKind) -> (&'static str, &'static str) {
    match engine {
        EngineKind::Mz => (
            "js/rmmz_managers.js",
            include_str!("../../tests/fixtures/mz_rmmz_managers.js"),
        ),
        EngineKind::MvCustom => (
            "js/rpg_managers.js",
            include_str!("../../tests/fixtures/mv_rpg_managers.js"),
        ),
    }
}

fn decrypt_or_encrypt_fixture(engine: EngineKind, input: &Path, output: &Path) {
    match engine {
        EngineKind::Mz => rpgdata_crypt::commands::encrypt_file(input, output, None).unwrap(),
        EngineKind::MvCustom => {
            let game_root = output.parent().unwrap().parent().unwrap();
            rpgdata_crypt::commands::encrypt_file(input, output, Some(game_root)).unwrap()
        }
    }
}
