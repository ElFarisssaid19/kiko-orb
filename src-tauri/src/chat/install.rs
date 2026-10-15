//! Installs Kiko's chat for the sessions the user starts with it: copies kiko-channel into Kiko's data
//! directory and writes Kiko's own MCP config (mcp.json) pointing to it. Claude Code loads that file
//! only when started with `claude --mcp-config <mcp.json> --dangerously-load-development-channels
//! server:kiko`, so other sessions never start the channel. No command is run, and neither
//! `~/.claude.json` nor Claude Code's settings are touched. The install records what it created, so
//! uninstall removes exactly that.

use std::{
    collections::hash_map::DefaultHasher,
    env, fmt, fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use kiko_channel::protocol::{SERVER_NAME, launch_args};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::hooks::{Action, shell_path};

/// The first Claude Code version with channels.
pub const MIN_CLAUDE_VERSION: (u32, u32, u32) = (2, 1, 80);
/// The alias the Settings suggest; the user adds it to their shell themselves.
pub const ALIAS_NAME: &str = "kiko-claude";
/// `claude --version` answers at once; this only guards against a stuck process.
const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub enum ChatInstallError {
    Io(io::Error),
    /// kiko-channel isn't built next to Kiko's executable; nothing was written.
    MissingBinary(PathBuf),
    /// The files changed between the preview and the confirmation; nothing was written.
    ChangedSincePreview,
}

impl fmt::Display for ChatInstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChatInstallError::Io(error) => write!(f, "{error}"),
            ChatInstallError::MissingBinary(path) => write!(
                f,
                "kiko-channel was not found at {}. Build it with: cd src-tauri && cargo build -p kiko-channel",
                path.display()
            ),
            ChatInstallError::ChangedSincePreview => write!(
                f,
                "Kiko's chat files changed since the preview. Nothing was changed; review it again."
            ),
        }
    }
}

impl From<io::Error> for ChatInstallError {
    fn from(error: io::Error) -> Self {
        ChatInstallError::Io(error)
    }
}

/// What an install created, saved next to Kiko's config.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRecord {
    pub binary_path: String,
    pub mcp_config_path: String,
    /// Directories that didn't exist before, deepest last.
    pub created_dirs: Vec<String>,
}

/// One thing a change does, shown before it happens.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    /// "create-dir" | "copy" | "write" | "remove" | "remove-dir"
    pub kind: &'static str,
    /// Where, with `~` for the home directory.
    pub path: String,
    pub detail: String,
    /// The text written, for "write".
    pub content: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatPreview {
    pub action: Action,
    pub steps: Vec<Step>,
    pub nothing_to_do: bool,
    /// Identifies the files at preview time; `apply` refuses to write if they changed.
    pub fingerprint: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatState {
    /// The channel and Kiko's mcp.json are in place.
    pub installed: bool,
    /// Installed, and the same as this Kiko would install.
    pub up_to_date: bool,
    /// Something of Kiko's chat is on disk, even half an install: it can be removed.
    pub removable: bool,
}

/// What `apply` does, step by step. `steps` is what the user sees.
enum Op {
    CreateDir(PathBuf),
    Copy,
    Write(PathBuf, String),
    Remove(PathBuf),
    RemoveDir(PathBuf),
}

/// Installs and removes Kiko's chat channel.
pub struct ChatInstaller {
    /// kiko-channel as built next to Kiko's own executable.
    pub source: PathBuf,
    /// Where it is installed: `~/.local/share/kiko/bin/kiko-channel`.
    pub binary_path: PathBuf,
    /// Kiko's MCP config for chat sessions: `~/.config/kiko/mcp.json`.
    pub mcp_config_path: PathBuf,
    /// Kiko's config.json (port and token), given to the channel with `--config`.
    pub kiko_config_path: PathBuf,
    /// The `ChatRecord`: `~/.config/kiko/chat-install.json`.
    pub record_path: PathBuf,
    pub home: PathBuf,
}

fn read_bytes(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Writes through a temporary file and a rename: Claude Code never reads a half-written file, and a
/// running kiko-channel keeps its old copy instead of failing with "text file busy".
fn replace_file(path: &Path, write_temp: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = path.with_file_name(format!(".{name}.kiko-tmp"));
    if let Err(error) = write_temp(&temp).and_then(|()| fs::rename(&temp, path)) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    Ok(())
}

fn size_text(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

impl ChatInstaller {
    /// Kiko's mcp.json: one stdio server, `kiko`, which is kiko-channel with Kiko's config.
    pub fn mcp_config(&self) -> String {
        let config = json!({
            "mcpServers": {
                SERVER_NAME: {
                    "command": self.binary_path,
                    "args": ["--config", self.kiko_config_path]
                }
            }
        });
        let mut text = serde_json::to_string_pretty(&config).unwrap_or_default();
        text.push('\n');
        text
    }

    /// `path` for the user: `~/…` inside the home directory.
    pub fn display(&self, path: &Path) -> String {
        match path.strip_prefix(&self.home) {
            Ok(relative) => format!("~/{}", relative.display()),
            Err(_) => path.display().to_string(),
        }
    }

    /// The command that starts a chat session, ready to paste in a shell.
    pub fn launch_command(&self) -> String {
        let safe = |text: &str| {
            text.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
        };
        let config = match self.mcp_config_path.strip_prefix(&self.home) {
            Ok(relative) if safe(&relative.display().to_string()) => {
                format!("~/{}", relative.display())
            }
            _ => shell_path(&self.mcp_config_path, &self.home),
        };
        format!("claude {}", launch_args(&config))
    }

    /// `alias kiko-claude='…'`, for the user's shell configuration.
    pub fn alias(&self) -> String {
        let command = self.launch_command().replace('\'', r"'\''");
        format!("alias {ALIAS_NAME}='{command}'")
    }

    fn record(&self) -> Option<ChatRecord> {
        serde_json::from_str(&fs::read_to_string(&self.record_path).ok()?).ok()
    }

    pub fn state(&self) -> io::Result<ChatState> {
        let binary = read_bytes(&self.binary_path)?;
        let config = read_bytes(&self.mcp_config_path)?;
        let installed = binary.is_some() && config.is_some();
        let source = read_bytes(&self.source)?;
        let up_to_date = installed
            && config.as_deref() == Some(self.mcp_config().as_bytes())
            // Without a built kiko-channel to compare with, the installed one is all there is.
            && (source.is_none() || source == binary);
        Ok(ChatState {
            installed,
            up_to_date,
            removable: binary.is_some() || config.is_some() || self.record_path.exists(),
        })
    }

    /// Why Install can't work here: kiko-channel isn't built next to Kiko.
    pub fn source_error(&self) -> Option<String> {
        (!self.source.is_file())
            .then(|| ChatInstallError::MissingBinary(self.source.clone()).to_string())
    }

    fn fingerprint(&self) -> io::Result<String> {
        let mut hasher = DefaultHasher::new();
        for path in [
            &self.source,
            &self.binary_path,
            &self.mcp_config_path,
            &self.record_path,
        ] {
            read_bytes(path)?.hash(&mut hasher);
        }
        Ok(format!("{:016x}", hasher.finish()))
    }

    /// What `action` would do, in order: the steps shown to the user and the operations behind them.
    fn plan(&self, action: Action) -> Result<Vec<(Step, Op)>, ChatInstallError> {
        let mut plan = Vec::new();
        let step = |kind, path: &Path, detail: String, content: Option<String>| Step {
            kind,
            path: self.display(path),
            detail,
            content,
        };
        match action {
            Action::Install => {
                let source = read_bytes(&self.source)?
                    .ok_or_else(|| ChatInstallError::MissingBinary(self.source.clone()))?;
                let installed = read_bytes(&self.binary_path)?;
                let mut created = self.record().map(|r| r.created_dirs).unwrap_or_default();
                let mut missing = Vec::new();
                let mut dir = self.binary_path.parent();
                while let Some(d) = dir.filter(|d| !d.exists() && *d != self.home) {
                    missing.push(d.to_path_buf());
                    dir = d.parent();
                }
                for dir in missing.into_iter().rev() {
                    let name = dir.display().to_string();
                    if !created.contains(&name) {
                        created.push(name);
                    }
                    plan.push((
                        step(
                            "create-dir",
                            &dir,
                            "New folder for Kiko's programs".into(),
                            None,
                        ),
                        Op::CreateDir(dir),
                    ));
                }
                if installed.as_deref() != Some(source.as_slice()) {
                    let detail = format!(
                        "Copy kiko-channel ({}) from {}{}",
                        size_text(source.len()),
                        self.display(&self.source),
                        if installed.is_some() {
                            ", replacing the installed copy"
                        } else {
                            ""
                        }
                    );
                    plan.push((step("copy", &self.binary_path, detail, None), Op::Copy));
                }
                let config = self.mcp_config();
                if read_bytes(&self.mcp_config_path)?.as_deref() != Some(config.as_bytes()) {
                    let detail = "Kiko's MCP config for chat sessions: Claude Code reads it only when started with the launch command".into();
                    plan.push((
                        step("write", &self.mcp_config_path, detail, Some(config.clone())),
                        Op::Write(self.mcp_config_path.clone(), config),
                    ));
                }
                let record = ChatRecord {
                    binary_path: self.binary_path.display().to_string(),
                    mcp_config_path: self.mcp_config_path.display().to_string(),
                    created_dirs: created,
                };
                let mut record_text = serde_json::to_string_pretty(&record).unwrap_or_default();
                record_text.push('\n');
                if !plan.is_empty()
                    || read_bytes(&self.record_path)?.as_deref() != Some(record_text.as_bytes())
                {
                    let detail =
                        "Kiko's note of what it installed, so Remove takes out exactly that".into();
                    plan.push((
                        step(
                            "write",
                            &self.record_path,
                            detail,
                            Some(record_text.clone()),
                        ),
                        Op::Write(self.record_path.clone(), record_text),
                    ));
                }
            }
            Action::Uninstall => {
                let record = self.record();
                let (binary, config) = match &record {
                    Some(r) => (
                        PathBuf::from(&r.binary_path),
                        PathBuf::from(&r.mcp_config_path),
                    ),
                    None => (self.binary_path.clone(), self.mcp_config_path.clone()),
                };
                let mut removed = Vec::new();
                for (path, detail) in [
                    (&binary, "The chat channel program"),
                    (&config, "Kiko's MCP config for chat sessions"),
                    (&self.record_path, "Kiko's note of what it installed"),
                ] {
                    if path.exists() {
                        removed.push(path.clone());
                        plan.push((
                            step("remove", path, detail.into(), None),
                            Op::Remove(path.clone()),
                        ));
                    }
                }
                // Folders Kiko created, deepest first, if nothing else is left in them.
                for dir in record.iter().flat_map(|r| r.created_dirs.iter().rev()) {
                    let dir = PathBuf::from(dir);
                    let empty_after = fs::read_dir(&dir).is_ok_and(|entries| {
                        entries.flatten().all(|e| removed.contains(&e.path()))
                    });
                    if empty_after {
                        removed.push(dir.clone());
                        plan.push((
                            step(
                                "remove-dir",
                                &dir,
                                "Folder Kiko created, now empty".into(),
                                None,
                            ),
                            Op::RemoveDir(dir),
                        ));
                    }
                }
            }
        }
        Ok(plan)
    }

    pub fn preview(&self, action: Action) -> Result<ChatPreview, ChatInstallError> {
        let fingerprint = self.fingerprint()?;
        let steps: Vec<Step> = self
            .plan(action)?
            .into_iter()
            .map(|(step, _)| step)
            .collect();
        Ok(ChatPreview {
            action,
            nothing_to_do: steps.is_empty(),
            steps,
            fingerprint,
        })
    }

    /// Does what the preview showed, if nothing changed since.
    pub fn apply(
        &self,
        action: Action,
        expected_fingerprint: &str,
    ) -> Result<(), ChatInstallError> {
        if self.fingerprint()? != expected_fingerprint {
            return Err(ChatInstallError::ChangedSincePreview);
        }
        for (_, op) in self.plan(action)? {
            match op {
                Op::CreateDir(dir) => fs::create_dir(&dir)?,
                // `fs::copy` keeps the source's permissions, so the copy is executable too.
                Op::Copy => replace_file(&self.binary_path, |temp| {
                    fs::copy(&self.source, temp).map(drop)
                })?,
                Op::Write(path, text) => {
                    if let Some(dir) = path.parent() {
                        fs::create_dir_all(dir)?;
                    }
                    replace_file(&path, |temp| fs::write(temp, text))?
                }
                Op::Remove(path) => match fs::remove_file(&path) {
                    Err(error) if error.kind() != io::ErrorKind::NotFound => {
                        return Err(error.into());
                    }
                    _ => {}
                },
                // Something new in it since the preview: it stays.
                Op::RemoveDir(dir) => {
                    let _ = fs::remove_dir(&dir);
                }
            }
        }
        Ok(())
    }
}

/// The Claude Code CLI found on this machine.
#[derive(Clone, Debug, PartialEq)]
pub struct ClaudeCli {
    pub path: PathBuf,
    /// "2.1.296", from `claude --version`; `None` if it didn't answer.
    pub version: Option<String>,
}

impl ClaudeCli {
    /// Older than the first version with channels (an unreadable version counts as old enough).
    pub fn too_old(&self) -> bool {
        self.version
            .as_deref()
            .and_then(parse_version)
            .is_some_and(|version| version < MIN_CLAUDE_VERSION)
    }
}

/// `2.1.296` → (2, 1, 296).
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().split('.').map(|n| n.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

/// Finds `claude` on `path_var` (the PATH), then where its installers put it: a Kiko started from
/// the desktop may not have `~/.local/bin` on its PATH.
pub fn find_claude(home: &Path, path_var: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    let name = format!("claude{}", env::consts::EXE_SUFFIX);
    let mut dirs: Vec<PathBuf> = path_var
        .map(|p| env::split_paths(p).collect())
        .unwrap_or_default();
    dirs.extend([
        home.join(".local").join("bin"),
        home.join(".claude").join("local"),
        home.join(".npm-global").join("bin"),
    ]);
    dirs.into_iter()
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file())
}

/// Runs `claude --version` ("2.1.296 (Claude Code)") and returns the version number.
pub fn claude_version(claude: &Path) -> Option<String> {
    let mut child = Command::new(claude)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + VERSION_TIMEOUT;
    while child.try_wait().ok()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let output = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let version = stdout.split_whitespace().next()?;
    parse_version(version).map(|_| version.to_string())
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    /// `mcpServers.kiko.command` in an mcp.json.
    fn configured_command(text: &str) -> Option<String> {
        let value: Value = serde_json::from_str(text).ok()?;
        value["mcpServers"][SERVER_NAME]["command"]
            .as_str()
            .map(str::to_string)
    }

    struct Setup {
        _dir: tempfile::TempDir,
        installer: ChatInstaller,
    }

    /// A home directory with Kiko's config folder, and a "built" kiko-channel next to Kiko.
    fn setup() -> Setup {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let config = home.join(".config").join("kiko");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(home.join(".local").join("share")).unwrap();
        fs::write(config.join("config.json"), "{}").unwrap();
        let target = dir.path().join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("kiko-channel"), b"\x7fELF channel v1").unwrap();
        let installer = ChatInstaller {
            source: target.join("kiko-channel"),
            binary_path: home.join(".local/share/kiko/bin/kiko-channel"),
            mcp_config_path: config.join("mcp.json"),
            kiko_config_path: config.join("config.json"),
            record_path: config.join("chat-install.json"),
            home,
        };
        Setup {
            _dir: dir,
            installer,
        }
    }

    fn run(installer: &ChatInstaller, action: Action) -> ChatPreview {
        let preview = installer.preview(action).unwrap();
        installer.apply(action, &preview.fingerprint).unwrap();
        preview
    }

    /// Every file and folder under `dir`, relative, sorted.
    fn tree(dir: &Path) -> Vec<String> {
        fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
            for entry in fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                out.push(path.strip_prefix(root).unwrap().display().to_string());
                if path.is_dir() {
                    walk(&path, root, out);
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn install_shows_every_step_then_does_exactly_that() {
        let Setup { _dir, installer } = setup();
        let before = tree(&installer.home);
        let preview = installer.preview(Action::Install).unwrap();
        let steps: Vec<(&str, &str)> = preview
            .steps
            .iter()
            .map(|s| (s.kind, s.path.as_str()))
            .collect();
        assert_eq!(
            steps,
            [
                ("create-dir", "~/.local/share/kiko"),
                ("create-dir", "~/.local/share/kiko/bin"),
                ("copy", "~/.local/share/kiko/bin/kiko-channel"),
                ("write", "~/.config/kiko/mcp.json"),
                ("write", "~/.config/kiko/chat-install.json"),
            ]
        );
        assert_eq!(tree(&installer.home), before, "a preview writes nothing");

        installer
            .apply(Action::Install, &preview.fingerprint)
            .unwrap();
        assert_eq!(
            fs::read(&installer.binary_path).unwrap(),
            fs::read(&installer.source).unwrap()
        );
        let config = fs::read_to_string(&installer.mcp_config_path).unwrap();
        assert_eq!(Some(config.as_str()), preview.steps[3].content.as_deref());
        assert_eq!(
            configured_command(&config).unwrap(),
            installer.binary_path.display().to_string()
        );
        let value: Value = serde_json::from_str(&config).unwrap();
        assert_eq!(
            value["mcpServers"]["kiko"]["args"],
            json!(["--config", installer.kiko_config_path])
        );
        assert_eq!(
            installer.state().unwrap(),
            ChatState {
                installed: true,
                up_to_date: true,
                removable: true
            }
        );
        assert!(installer.preview(Action::Install).unwrap().nothing_to_do);
    }

    #[test]
    fn uninstall_removes_exactly_what_install_created() {
        let Setup { _dir, installer } = setup();
        let before = tree(&installer.home);
        run(&installer, Action::Install);
        let preview = run(&installer, Action::Uninstall);
        let kinds: Vec<&str> = preview.steps.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            ["remove", "remove", "remove", "remove-dir", "remove-dir"]
        );
        assert_eq!(tree(&installer.home), before);
        assert_eq!(
            installer.state().unwrap(),
            ChatState {
                installed: false,
                up_to_date: false,
                removable: false
            }
        );
    }

    #[test]
    fn folders_that_existed_or_hold_other_files_stay() {
        let Setup { _dir, installer } = setup();
        let kiko_data = installer.home.join(".local/share/kiko");
        fs::create_dir_all(&kiko_data).unwrap();
        run(&installer, Action::Install);
        fs::write(installer.binary_path.with_file_name("notes.txt"), "mine").unwrap();
        run(&installer, Action::Uninstall);
        assert!(!installer.binary_path.exists());
        assert!(!installer.mcp_config_path.exists());
        assert!(installer.binary_path.with_file_name("notes.txt").exists());
        assert!(kiko_data.exists());
    }

    #[test]
    fn a_new_build_needs_an_update_and_replaces_only_the_binary() {
        let Setup { _dir, installer } = setup();
        run(&installer, Action::Install);
        fs::write(&installer.source, b"\x7fELF channel v2").unwrap();
        assert_eq!(
            installer.state().unwrap(),
            ChatState {
                installed: true,
                up_to_date: false,
                removable: true
            }
        );
        let preview = run(&installer, Action::Install);
        let kinds: Vec<&str> = preview.steps.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, ["copy", "write"], "the binary, and the record of it");
        assert!(
            preview.steps[0]
                .detail
                .contains("replacing the installed copy")
        );
        assert_eq!(
            fs::read(&installer.binary_path).unwrap(),
            b"\x7fELF channel v2"
        );
        // The record still names the folders the first install created.
        let record: ChatRecord =
            serde_json::from_str(&fs::read_to_string(&installer.record_path).unwrap()).unwrap();
        assert_eq!(record.created_dirs.len(), 2);
        run(&installer, Action::Uninstall);
        assert!(!installer.home.join(".local/share/kiko").exists());
    }

    #[test]
    fn nothing_is_written_when_files_changed_since_the_preview() {
        let Setup { _dir, installer } = setup();
        let preview = installer.preview(Action::Install).unwrap();
        fs::write(&installer.mcp_config_path, "{}").unwrap();
        assert!(matches!(
            installer.apply(Action::Install, &preview.fingerprint),
            Err(ChatInstallError::ChangedSincePreview)
        ));
        assert!(!installer.binary_path.exists());
        assert_eq!(
            fs::read_to_string(&installer.mcp_config_path).unwrap(),
            "{}"
        );
    }

    #[test]
    fn without_a_built_channel_install_explains_how_to_build_it() {
        let Setup { _dir, installer } = setup();
        fs::remove_file(&installer.source).unwrap();
        let error = installer.preview(Action::Install).unwrap_err();
        assert!(
            error.to_string().contains("cargo build -p kiko-channel"),
            "{error}"
        );
        assert!(installer.source_error().is_some());
        // Removing never needs it.
        assert!(installer.preview(Action::Uninstall).unwrap().nothing_to_do);
    }

    #[test]
    fn launch_command_and_alias() {
        let Setup { _dir, installer } = setup();
        assert_eq!(
            installer.launch_command(),
            "claude --mcp-config ~/.config/kiko/mcp.json --dangerously-load-development-channels server:kiko"
        );
        assert_eq!(
            installer.alias(),
            "alias kiko-claude='claude --mcp-config ~/.config/kiko/mcp.json --dangerously-load-development-channels server:kiko'"
        );
        let odd = ChatInstaller {
            mcp_config_path: installer.home.join("my kiko/mcp.json"),
            ..installer
        };
        assert_eq!(
            odd.launch_command(),
            "claude --mcp-config \"$HOME/my kiko/mcp.json\" --dangerously-load-development-channels server:kiko"
        );
    }

    #[test]
    fn claude_versions() {
        assert_eq!(parse_version("2.1.296"), Some((2, 1, 296)));
        assert_eq!(parse_version("2.1"), None);
        let cli = |version: &str| ClaudeCli {
            path: PathBuf::from("claude"),
            version: Some(version.into()),
        };
        assert!(cli("2.1.79").too_old());
        assert!(!cli("2.1.80").too_old());
        assert!(!cli("2.10.0").too_old());
        assert!(
            !ClaudeCli {
                path: PathBuf::from("claude"),
                version: None
            }
            .too_old()
        );
    }

    #[test]
    fn claude_is_found_outside_the_path_too() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path();
        assert_eq!(find_claude(home, Some(std::ffi::OsStr::new(""))), None);
        let local = home.join(".local/bin");
        fs::create_dir_all(&local).unwrap();
        fs::write(local.join("claude"), "").unwrap();
        assert_eq!(find_claude(home, None), Some(local.join("claude")));
    }

    #[test]
    fn the_version_comes_from_claude_version() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("claude");
        fs::write(&fake, "#!/bin/sh\necho '2.1.296 (Claude Code)'\n").unwrap();
        let status = Command::new("chmod").arg("+x").arg(&fake).status().unwrap();
        assert!(status.success());
        assert_eq!(claude_version(&fake).as_deref(), Some("2.1.296"));
        assert_eq!(claude_version(&dir.path().join("missing")), None);
    }
}
