//! `~/.config/kiko/config.json`: the hook server's port and the secret every hook request must carry.
//! Next to it, `auth-header` holds that secret as a curl header line for the hook commands, so the
//! token never appears in Claude Code's settings.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::platform;

pub const DEFAULT_PORT: u16 = 47821;
/// Next to config.json; the hook commands read it with `curl -H @<file>`.
pub const AUTH_HEADER_FILE: &str = "auth-header";
/// Kiko lets go of a held permission or question after this long, below the hook's own timeout.
pub const DEFAULT_DECISION_TIMEOUT_SECS: u64 = 110;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KikoConfig {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_timeout")]
    pub decision_timeout_secs: u64,
}

fn default_port() -> u16 {
    DEFAULT_PORT
}

fn default_timeout() -> u64 {
    DEFAULT_DECISION_TIMEOUT_SECS
}

impl KikoConfig {
    /// Reads the config, creating it (with a new token) on first run. The file is kept owner-only (0600).
    pub fn load_or_create(path: &Path) -> io::Result<Self> {
        let mut config = match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).map_err(io::Error::other)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => KikoConfig {
                port: DEFAULT_PORT,
                token: String::new(),
                decision_timeout_secs: DEFAULT_DECISION_TIMEOUT_SECS,
            },
            Err(error) => return Err(error),
        };
        let needs_token = config.token.len() < 32;
        if needs_token {
            config.token = new_token()?;
        }
        if needs_token || !path.exists() {
            config.save(path)?;
        } else {
            platform::restrict_to_owner(path)?;
        }
        config.write_auth_header(&auth_header_path(path))?;
        Ok(config)
    }

    /// `X-Kiko-Token: <token>`, rewritten whenever it no longer matches the token. Owner-only (0600).
    fn write_auth_header(&self, path: &Path) -> io::Result<()> {
        let line = format!("{}: {}\n", super::install::TOKEN_HEADER, self.token);
        if fs::read_to_string(path).is_ok_and(|current| current == line) {
            return platform::restrict_to_owner(path);
        }
        platform::write_private_file(path, line.as_bytes())
    }

    fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            platform::create_private_dir(dir)?;
        }
        let mut json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        json.push('\n');
        platform::write_private_file(path, json.as_bytes())
    }
}

/// The header file that goes with the config at `config_path`.
pub fn auth_header_path(config_path: &Path) -> PathBuf {
    config_path.with_file_name(AUTH_HEADER_FILE)
}

/// 32 random bytes as hex.
fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(io::Error::other)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_run_creates_a_private_config_and_keeps_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiko").join("config.json");
        let first = KikoConfig::load_or_create(&path).unwrap();
        assert_eq!(first.port, DEFAULT_PORT);
        assert_eq!(first.token.len(), 64);
        assert_eq!(KikoConfig::load_or_create(&path).unwrap(), first);
        assert!(platform::is_owner_only(&path).unwrap());

        let header = auth_header_path(&path);
        assert_eq!(
            fs::read_to_string(&header).unwrap(),
            format!("X-Kiko-Token: {}\n", first.token)
        );
        assert!(platform::is_owner_only(&header).unwrap());
    }

    #[test]
    fn the_auth_header_follows_a_new_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let first = KikoConfig::load_or_create(&path).unwrap();
        // A lost token (here: an emptied one) is regenerated, and the header file with it.
        fs::write(&path, r#"{ "port": 47821, "token": "" }"#).unwrap();
        let second = KikoConfig::load_or_create(&path).unwrap();
        assert_ne!(second.token, first.token);
        assert_eq!(
            fs::read_to_string(auth_header_path(&path)).unwrap(),
            format!("X-Kiko-Token: {}\n", second.token)
        );
    }

    #[test]
    fn a_custom_port_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{ "port": 50000 }"#).unwrap();
        let config = KikoConfig::load_or_create(&path).unwrap();
        assert_eq!(config.port, 50000);
        assert_eq!(config.token.len(), 64);
    }
}
