use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

pub const MZ_MANAGERS_JS: &str = "js/rmmz_managers.js";
pub const MV_MANAGERS_JS: &str = "js/rpg_managers.js";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    Mz,
    MvCustom,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameContext {
    pub root: PathBuf,
    pub engine: EngineKind,
    pub manager_js: PathBuf,
}

pub fn detect_game_context(root: &Path) -> Result<GameContext> {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let data_dir = root.join("data");
    if !data_dir.is_dir() {
        bail!("{} is not a game root: missing data/", root.display());
    }

    let mz_js = root.join(MZ_MANAGERS_JS);
    if mz_js.is_file() {
        return Ok(GameContext {
            root,
            engine: EngineKind::Mz,
            manager_js: mz_js,
        });
    }

    let mv_js = root.join(MV_MANAGERS_JS);
    if mv_js.is_file() {
        return Ok(GameContext {
            root,
            engine: EngineKind::MvCustom,
            manager_js: mv_js,
        });
    }

    bail!(
        "{} is not a supported game root: expected {} or {}",
        root.display(),
        MZ_MANAGERS_JS,
        MV_MANAGERS_JS
    )
}

fn path_search_start(path: &Path) -> PathBuf {
    if path.is_file() {
        return path.parent().unwrap_or(path).to_path_buf();
    }
    if !path.exists() && path.extension().is_some() {
        return path.parent().unwrap_or(path).to_path_buf();
    }
    path.to_path_buf()
}

pub fn auto_detect_game_context(paths: &[&Path]) -> Result<GameContext> {
    for path in paths {
        let current = path_search_start(path);
        for candidate in current.ancestors() {
            if let Ok(ctx) = detect_game_context(candidate) {
                return Ok(ctx);
            }
        }
    }

    bail!(
        "Could not auto-detect an RPG Maker game root from the provided paths. Pass --game /path/to/game."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_mz_game_kind() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("js")).unwrap();
        fs::write(root.join(MZ_MANAGERS_JS), "// mz").unwrap();

        let ctx = detect_game_context(root).unwrap();
        assert_eq!(ctx.engine, EngineKind::Mz);
        assert_eq!(ctx.manager_js, root.join(MZ_MANAGERS_JS));
    }

    #[test]
    fn detects_mv_game_kind() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("js")).unwrap();
        fs::write(root.join(MV_MANAGERS_JS), "// mv").unwrap();

        let ctx = detect_game_context(root).unwrap();
        assert_eq!(ctx.engine, EngineKind::MvCustom);
        assert_eq!(ctx.manager_js, root.join(MV_MANAGERS_JS));
    }

    #[test]
    fn auto_detects_from_nested_path() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let nested = root.join("www/data/System.json");
        fs::create_dir_all(root.join("data")).unwrap();
        fs::create_dir_all(root.join("js")).unwrap();
        fs::create_dir_all(root.join("www/data")).unwrap();
        fs::write(root.join(MZ_MANAGERS_JS), "// mz").unwrap();

        let ctx = auto_detect_game_context(&[nested.as_path()]).unwrap();
        assert_eq!(ctx.engine, EngineKind::Mz);
        assert_eq!(ctx.root, root.to_path_buf());
    }
}
