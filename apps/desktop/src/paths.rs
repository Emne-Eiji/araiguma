//! XDG configuration and state paths (OD-08, document 05 §1).
//!
//! The desktop process resolves its configuration and state locations
//! through the XDG Base Directory rules and uses nothing else:
//!
//! | Content | Path |
//! |---|---|
//! | Configuration | `$XDG_CONFIG_HOME/araiguma/config.toml` |
//! | State | `$XDG_STATE_HOME/araiguma/` |
//!
//! `$XDG_CONFIG_HOME` and `$XDG_STATE_HOME` are honored when they are set
//! to an ABSOLUTE path (the XDG specification requires relative values to
//! be ignored); otherwise the defaults `~/.config` and `~/.local/state`
//! apply.
//!
//! Stage 1 (WO-05) resolves the locations and ensures the state directory
//! exists. No configuration file is read or written yet: no configuration
//! schema is defined, and inventing one would be inventing behavior.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// The resolved configuration and state locations of the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgPaths {
    /// `$XDG_CONFIG_HOME/araiguma` — holds `config.toml`.
    pub config_dir: PathBuf,
    /// `$XDG_STATE_HOME/araiguma` — holds logs and session state.
    pub state_dir: PathBuf,
}

impl XdgPaths {
    /// Resolve the locations from the environment.
    pub fn resolve() -> XdgPaths {
        let config_home = xdg_dir("XDG_CONFIG_HOME", ".config");
        let state_home = xdg_dir("XDG_STATE_HOME", ".local/state");
        XdgPaths {
            config_dir: config_home.join("araiguma"),
            state_dir: state_home.join("araiguma"),
        }
    }

    /// The configuration file path (`config.toml` inside the config dir).
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Ensure the state directory exists. A failure is reported, not
    /// hidden: the caller decides whether the app can continue without it.
    pub fn ensure_state_dir(&self) -> Result<(), String> {
        create_dir(&self.state_dir)
    }
}

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    xdg_dir_from(std::env::var_os(variable).as_deref(), fallback, &home_dir())
}

fn xdg_dir_from(value: Option<&OsStr>, fallback: &str, home: &Path) -> PathBuf {
    match value {
        // The XDG specification: a relative path in the environment
        // variable is invalid and must be ignored.
        Some(value) if !value.is_empty() => {
            let path = PathBuf::from(value);
            if path.is_absolute() {
                path
            } else {
                home.join(fallback)
            }
        }
        _ => home.join(fallback),
    }
}

fn home_dir() -> PathBuf {
    match std::env::var_os("HOME") {
        Some(home) if !home.is_empty() => PathBuf::from(home),
        _ => PathBuf::from("/"),
    }
}

fn create_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path)
        .map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    restrict_permissions(path)
}

#[cfg(unix)]
fn restrict_permissions(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .map_err(|error| format!("cannot restrict permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_directories_are_named_after_the_project() {
        let paths = XdgPaths {
            config_dir: PathBuf::from("/tmp/config/araiguma"),
            state_dir: PathBuf::from("/tmp/state/araiguma"),
        };
        assert!(paths.config_dir.ends_with("araiguma"));
        assert!(paths.state_dir.ends_with("araiguma"));
        assert_eq!(
            paths.config_file(),
            PathBuf::from("/tmp/config/araiguma/config.toml")
        );
    }

    #[test]
    fn relative_xdg_values_are_ignored() {
        let home = Path::new("/home/example");
        assert_eq!(
            xdg_dir_from(Some(OsStr::new("relative/path")), ".config", home),
            PathBuf::from("/home/example/.config")
        );
        assert_eq!(
            xdg_dir_from(Some(OsStr::new("/absolute/path")), ".config", home),
            PathBuf::from("/absolute/path")
        );
    }

    #[test]
    fn state_directory_is_created_when_missing() {
        let root = std::env::temp_dir().join(format!("araiguma-xdg-test-{}", std::process::id()));
        let state_dir = root.join("araiguma");
        let paths = XdgPaths {
            config_dir: root.join("config/araiguma"),
            state_dir: state_dir.clone(),
        };
        assert!(!state_dir.exists());
        paths.ensure_state_dir().expect("state dir created");
        assert!(state_dir.is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }
}
