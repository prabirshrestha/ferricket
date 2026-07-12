use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub async fn workspace_key(tickets_dir: &Path) -> String {
    tokio::fs::canonicalize(tickets_dir)
        .await
        .unwrap_or_else(|_| tickets_dir.to_owned())
        .to_string_lossy()
        .into_owned()
}

pub async fn load(workspace_key: &str, section: &str) -> Result<Option<Value>> {
    let path = path(workspace_key, section)?;
    match tokio::fs::read_to_string(&path).await {
        Ok(contents) => Ok(Some(
            serde_json::from_str(&contents)
                .with_context(|| format!("reading preferences from {}", path.display()))?,
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            Err(error).with_context(|| format!("reading preferences from {}", path.display()))
        }
    }
}

pub async fn save(workspace_key: &str, section: &str, value: &Value) -> Result<()> {
    if !value.is_object() {
        bail!("preferences must be a JSON object");
    }
    let path = path(workspace_key, section)?;
    let parent = path
        .parent()
        .context("cannot locate preferences directory")?;
    tokio::fs::create_dir_all(parent).await?;
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        rand::random::<u64>()
    ));
    if let Err(error) = tokio::fs::write(&temporary, serde_json::to_vec_pretty(value)?).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error).with_context(|| format!("writing preferences to {}", path.display()));
    }
    if cfg!(windows)
        && tokio::fs::metadata(&path).await.is_ok()
        && let Err(error) = tokio::fs::remove_file(&path).await
    {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error).with_context(|| format!("replacing preferences at {}", path.display()));
    }
    match tokio::fs::rename(&temporary, &path).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = tokio::fs::remove_file(&temporary).await;
            Err(error).with_context(|| format!("replacing preferences at {}", path.display()))
        }
    }
}

fn path(workspace_key: &str, section: &str) -> Result<PathBuf> {
    if !matches!(section, "workspace" | "display" | "theme" | "tui") {
        bail!("unknown preference section '{section}'");
    }
    let root = if cfg!(target_os = "macos") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support/ferricket"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|path| path.join(".config"))
            })
            .map(|path| path.join("ferricket"))
    }
    .context("cannot locate user config directory")?;
    let mut hasher = Sha256::new();
    hasher.update(workspace_key.as_bytes());
    hasher.update([0]);
    hasher.update(section.as_bytes());
    Ok(root
        .join("preferences")
        .join(format!("{:x}.json", hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_paths_are_scoped_by_workspace_and_section() {
        let one = path("/tmp/one/.tickets", "workspace").unwrap();
        let two = path("/tmp/two/.tickets", "workspace").unwrap();
        let display = path("/tmp/one/.tickets", "display").unwrap();
        assert_ne!(one, two);
        assert_ne!(one, display);
        assert!(path("/tmp/one/.tickets", "../tickets").is_err());
    }
}
