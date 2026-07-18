use crate::model::AppSnapshot;
use std::{env, fs, path::PathBuf};

pub fn load() -> Result<AppSnapshot, String> {
    let path = cache_path();
    if !path.exists() {
        return Ok(AppSnapshot::default());
    }
    let bytes =
        fs::read(&path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

pub fn save(snapshot: &AppSnapshot) -> Result<(), String> {
    let path = cache_path();
    ensure_data_dir()?;
    let bytes = serde_json::to_vec(snapshot)
        .map_err(|error| format!("could not serialize usage cache: {error}"))?;
    fs::write(&path, bytes)
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
    restrict_permissions(&path)?;
    Ok(())
}

fn cache_path() -> PathBuf {
    data_dir().join("state.json")
}

pub(crate) fn data_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(root).join("SeeUsage");
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("SeeUsage");
    }

    env::temp_dir().join("SeeUsage")
}

pub(crate) fn ensure_data_dir() -> Result<PathBuf, String> {
    let path = data_dir();
    fs::create_dir_all(&path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("could not secure {}: {error}", path.display()))?;
    }

    Ok(path)
}

fn restrict_permissions(path: &std::path::Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(|error| format!("could not secure {}: {error}", path.display()))?;
    }

    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
