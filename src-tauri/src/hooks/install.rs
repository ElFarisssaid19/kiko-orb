//! Adds Kiko's Claude Code hooks to the user's settings and removes them again, leaving every other
//! setting and hook as it was. Each hook is a shell command that pipes the event to Kiko with curl and
//! stays silent when Kiko is closed. Kiko's entries are recognized by their endpoint,
//! http://127.0.0.1:<port>/kiko/hooks/…, which also finds the hooks of earlier versions (http hooks,
//! then commands without the process id) so a reinstall replaces them.

use std::{
    collections::hash_map::DefaultHasher,
    fmt, fs,
    hash::{Hash, Hasher},
    io,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use super::parse::{ASK_USER_QUESTION, Hook};
use crate::platform;

pub const HOOK_PATH_PREFIX: &str = "/kiko/hooks/";
pub const TOKEN_HEADER: &str = "X-Kiko-Token";
/// The Claude Code process that ran the hook. Shell-form hooks run as `sh -c` children of Claude Code,
/// so `$PPID` is that process: the same one that starts Kiko's chat channel.
pub const PID_HEADER: &str = "X-Kiko-Claude-Pid";

/// curl's limit (seconds) on an event Kiko answers at once. A closed Kiko refuses the connection at once.
const QUICK_CURL_SECS: u64 = 2;
/// curl's limit (seconds) on a held permission or question: Kiko lets go after 110 s.
const DECISION_CURL_SECS: u64 = 115;
/// Claude Code's own `timeout` for each hook, in seconds (per the docs), always above curl's.
const QUICK_TIMEOUT_SECS: u64 = 3;
const DECISION_TIMEOUT_SECS: u64 = 120;

/// The oldest curl that reads headers from a file (`-H @file`).
const MIN_CURL: (u32, u32) = (7, 55);

/// The settings.json Claude Code reads for the user: `$CLAUDE_CONFIG_DIR/settings.json`, else `~/.claude/settings.json`.
pub fn claude_settings_path(home: &Path, claude_config_dir: Option<&str>) -> PathBuf {
    match claude_config_dir.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir).join("settings.json"),
        None => home.join(".claude").join("settings.json"),
    }
}

/// `path` as one double-quoted word for `sh -c`. Inside the home directory it is written
/// `"$HOME/…"`, so settings.json holds neither the user name nor the token.
pub fn shell_path(path: &Path, home: &Path) -> String {
    let escape = |text: &str| {
        text.chars().fold(String::new(), |mut out, c| {
            if matches!(c, '"' | '\\' | '$' | '`') {
                out.push('\\');
            }
            out.push(c);
            out
        })
    };
    match path.strip_prefix(home) {
        Ok(relative) => format!("\"$HOME/{}\"", escape(&relative.display().to_string())),
        Err(_) => format!("\"{}\"", escape(&path.display().to_string())),
    }
}

/// The shell command one hook runs: the event JSON goes from stdin to Kiko, with the Claude Code
/// process id in a header, and Kiko's answer comes back on stdout. `-f` prints nothing on an HTTP
/// error, and `|| true` turns every failure (Kiko closed, timeout, refused token) into a silent exit 0:
/// Claude Code carries on as if there were no hook.
pub fn hook_command(port: u16, auth_header: &str, hook: Hook, max_time: u64) -> String {
    format!(
        "curl -sf --max-time {max_time} -H @{auth_header} -H \"Content-Type: application/json\" \
         -H \"{PID_HEADER}: $PPID\" --data-binary @- http://127.0.0.1:{port}{HOOK_PATH_PREFIX}{} || true",
        hook.path()
    )
}

fn handler(port: u16, auth_header: &str, hook: Hook, held: bool, status: Option<&str>) -> Value {
    let (max_time, timeout) = if held {
        (DECISION_CURL_SECS, DECISION_TIMEOUT_SECS)
    } else {
        (QUICK_CURL_SECS, QUICK_TIMEOUT_SECS)
    };
    let mut handler = json!({
        "type": "command",
        "command": hook_command(port, auth_header, hook, max_time),
        "timeout": timeout
    });
    if let Some(status) = status {
        handler["statusMessage"] = json!(status);
    }
    handler
}

/// Kiko's `hooks` block: event name → matcher groups, one endpoint per event.
/// `auth_header` is the shell word for the header file (see `shell_path`).
pub fn kiko_hooks(port: u16, auth_header: &str) -> Map<String, Value> {
    let mut hooks = Map::new();
    for hook in Hook::ALL {
        let groups = match hook {
            // AskUserQuestion waits for the answer in Kiko; every other tool call is only observed.
            Hook::PreToolUse => json!([
                {
                    "matcher": ASK_USER_QUESTION,
                    "hooks": [handler(port, auth_header, hook, true, Some("Waiting for your answer in Kiko"))]
                },
                {
                    "matcher": format!("^(?!{ASK_USER_QUESTION}$)"),
                    "hooks": [handler(port, auth_header, hook, false, None)]
                }
            ]),
            Hook::PermissionRequest => json!([{
                "hooks": [handler(port, auth_header, hook, true, Some("Waiting for your decision in Kiko"))]
            }]),
            _ => json!([{ "hooks": [handler(port, auth_header, hook, false, None)] }]),
        };
        hooks.insert(hook.event_name().into(), groups);
    }
    hooks
}

/// Kiko's entries: today's curl commands, and the http hooks of earlier versions (so a reinstall migrates them).
pub fn is_kiko_handler(handler: &Value) -> bool {
    let field = |key: &str| handler.get(key).and_then(Value::as_str);
    match field("type") {
        Some("command") => field("command").is_some_and(|command| {
            command.contains("http://127.0.0.1:") && command.contains(HOOK_PATH_PREFIX)
        }),
        Some("http") => field("url").is_some_and(|url| {
            url.starts_with("http://127.0.0.1:") && url.contains(HOOK_PATH_PREFIX)
        }),
        _ => false,
    }
}

fn is_http(handler: &Value) -> bool {
    handler.get("type").and_then(Value::as_str) == Some("http")
}

/// Why the hooks can't run: Kiko's hook commands need curl.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CurlProblem {
    Missing,
    TooOld(String),
}

impl fmt::Display for CurlProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let install = platform::curl_install_command();
        match self {
            CurlProblem::Missing => write!(
                f,
                "Kiko's hooks need curl, which is not installed. Install it with: {install}"
            ),
            CurlProblem::TooOld(version) => write!(
                f,
                "Kiko's hooks need curl 7.55 or newer, and this one is {version}. Update it with: {install}"
            ),
        }
    }
}

/// The version of `program` (normally `curl`), if it can run Kiko's hook commands.
pub fn check_curl(program: &str) -> Result<String, CurlProblem> {
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|_| CurlProblem::Missing)?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    // "curl 8.5.0 (x86_64-pc-linux-gnu) libcurl/8.5.0 …"
    let version = stdout.split_whitespace().nth(1).unwrap_or("").to_string();
    let mut numbers = version.split('.').map(|n| n.parse::<u32>().unwrap_or(0));
    let found = (numbers.next().unwrap_or(0), numbers.next().unwrap_or(0));
    if !output.status.success() || found < MIN_CURL {
        return Err(CurlProblem::TooOld(version));
    }
    Ok(version)
}

#[derive(Debug)]
pub enum InstallError {
    Io(io::Error),
    /// settings.json can't be read as Claude Code settings; nothing was written.
    Invalid(String),
    /// settings.json changed between the preview and the confirmation; nothing was written.
    ChangedSincePreview,
    /// The hooks would not run on this machine; nothing was written.
    Curl(CurlProblem),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Io(error) => write!(f, "{error}"),
            InstallError::Invalid(why) => {
                write!(f, "settings.json is not valid: {why}. Nothing was changed.")
            }
            InstallError::Curl(problem) => write!(f, "{problem}"),
            InstallError::ChangedSincePreview => {
                write!(
                    f,
                    "settings.json changed since the preview. Nothing was changed; review it again."
                )
            }
        }
    }
}

impl From<io::Error> for InstallError {
    fn from(error: io::Error) -> Self {
        InstallError::Io(error)
    }
}

/// settings.json as read: its text (`None` when there is no file yet) and its top-level object.
struct Settings {
    text: Option<String>,
    json: Map<String, Value>,
}

fn read_settings(path: &Path) -> Result<Settings, InstallError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(Settings {
                text: None,
                json: Map::new(),
            });
        }
        Err(error) => return Err(error.into()),
    };
    let json = if text.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str(&text) {
            Ok(Value::Object(json)) => json,
            Ok(_) => return Err(InstallError::Invalid("it is not a JSON object".into())),
            Err(error) => return Err(InstallError::Invalid(error.to_string())),
        }
    };
    match json.get("hooks") {
        None => {}
        Some(Value::Object(hooks)) => {
            if let Some((event, _)) = hooks.iter().find(|(_, groups)| !groups.is_array()) {
                return Err(InstallError::Invalid(format!(
                    "hooks.{event} is not a list"
                )));
            }
        }
        Some(_) => return Err(InstallError::Invalid("\"hooks\" is not an object".into())),
    }
    Ok(Settings {
        text: Some(text),
        json,
    })
}

/// What an install created in settings.json, so that uninstall removes exactly that and keeps
/// whatever the user had, even an empty list. Saved next to Kiko's config.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub settings_path: String,
    pub created_hooks_object: bool,
    pub created_events: Vec<String>,
}

/// `settings` without Kiko's handlers, and without the groups they leave empty. Event lists and
/// the `hooks` object go only if Kiko created them (`record`); with no record, only if Kiko's
/// removal is what emptied them.
pub fn without_kiko(
    settings: &Map<String, Value>,
    record: Option<&InstallRecord>,
) -> Map<String, Value> {
    let mut settings = settings.clone();
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return settings;
    };
    let events_before = hooks.len();
    hooks.retain(|event, groups| {
        let Value::Array(groups) = groups else {
            return true;
        };
        let groups_before = groups.len();
        groups.retain_mut(|group| {
            let Some(Value::Array(handlers)) = group.get_mut("hooks") else {
                return true;
            };
            let handlers_before = handlers.len();
            handlers.retain(|handler| !is_kiko_handler(handler));
            !(handlers.is_empty() && handlers_before > 0)
        });
        let drop = match record {
            Some(record) => groups.is_empty() && record.created_events.contains(event),
            None => groups.is_empty() && groups_before > 0,
        };
        !drop
    });
    let drop_hooks = hooks.is_empty()
        && match record {
            Some(record) => record.created_hooks_object,
            None => events_before > 0,
        };
    if drop_hooks {
        settings.remove("hooks");
    }
    settings
}

/// `settings` with Kiko's hooks, replacing any earlier Kiko entries, and what that created.
/// Kiko's groups come after the user's.
pub fn with_kiko(
    settings: &Map<String, Value>,
    kiko: &Map<String, Value>,
    previous: Option<&InstallRecord>,
) -> (Map<String, Value>, InstallRecord) {
    let mut settings = without_kiko(settings, previous);
    let mut record = InstallRecord {
        created_hooks_object: !settings.contains_key("hooks"),
        ..Default::default()
    };
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    if let Value::Object(hooks) = hooks {
        for (event, groups) in kiko {
            if !hooks.contains_key(event) {
                record.created_events.push(event.clone());
            }
            let list = hooks.entry(event.clone()).or_insert_with(|| json!([]));
            if let (Value::Array(list), Value::Array(groups)) = (list, groups) {
                list.extend(groups.iter().cloned());
            }
        }
    }
    (settings, record)
}

/// Kiko's groups currently in `settings`, keeping only Kiko's handlers in each.
fn kiko_entries(settings: &Map<String, Value>) -> Map<String, Value> {
    let mut found = Map::new();
    let Some(Value::Object(hooks)) = settings.get("hooks") else {
        return found;
    };
    for (event, groups) in hooks {
        let kiko_groups: Vec<Value> = groups
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|group| {
                let handlers: Vec<Value> = group
                    .get("hooks")?
                    .as_array()?
                    .iter()
                    .filter(|handler| is_kiko_handler(handler))
                    .cloned()
                    .collect();
                if handlers.is_empty() {
                    return None;
                }
                let mut group = group.clone();
                group["hooks"] = Value::Array(handlers);
                Some(group)
            })
            .collect();
        if !kiko_groups.is_empty() {
            found.insert(event.clone(), Value::Array(kiko_groups));
        }
    }
    found
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HooksState {
    pub installed: bool,
    /// Installed with the current port and token.
    pub up_to_date: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Install,
    Uninstall,
}

/// What a change will do, shown to the user before anything is written.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub action: Action,
    pub settings_path: String,
    pub file_exists: bool,
    /// The `hooks` entries that will be added (install) or removed (uninstall), as JSON.
    pub entries: String,
    /// Install over an earlier Kiko install: those entries are replaced, not duplicated.
    pub replaces_existing: bool,
    /// The earlier install used http hooks (before Kiko switched to curl commands).
    pub replaces_http: bool,
    /// The earlier install's commands don't send the Claude Code process id, which chat needs.
    pub adds_process_id: bool,
    pub nothing_to_do: bool,
    /// Identifies the file's content at preview time; `apply` refuses to write if it changed.
    pub fingerprint: String,
}

/// Result of a change: where the previous file was saved, if there was one to save.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub changed: bool,
    pub backup_path: Option<String>,
}

fn fingerprint(text: Option<&str>) -> String {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Installs and removes Kiko's hooks in one Claude Code settings file.
pub struct Installer {
    pub settings_path: PathBuf,
    /// Where the `InstallRecord` is kept (Kiko's config directory).
    pub record_path: PathBuf,
    pub port: u16,
    /// The token's header file as the hook commands refer to it (`shell_path`).
    pub auth_header: String,
    /// The curl the hooks run, normally `curl` from PATH.
    pub curl: String,
}

impl Installer {
    fn record(&self) -> Option<InstallRecord> {
        let record: InstallRecord =
            serde_json::from_str(&fs::read_to_string(&self.record_path).ok()?).ok()?;
        (record.settings_path == self.settings_path.display().to_string()).then_some(record)
    }

    /// The settings after `action`, and the record to keep afterwards (`None` = no Kiko install).
    fn target(
        &self,
        settings: &Map<String, Value>,
        action: Action,
    ) -> (Map<String, Value>, Option<InstallRecord>) {
        let previous = self.record();
        match action {
            Action::Install => {
                let (updated, mut record) = with_kiko(
                    settings,
                    &kiko_hooks(self.port, &self.auth_header),
                    previous.as_ref(),
                );
                record.settings_path = self.settings_path.display().to_string();
                (updated, Some(record))
            }
            Action::Uninstall => (without_kiko(settings, previous.as_ref()), None),
        }
    }

    /// The hooks run curl: installing without a usable one would only produce hooks that do nothing.
    pub fn curl(&self) -> Result<String, InstallError> {
        check_curl(&self.curl).map_err(InstallError::Curl)
    }

    pub fn state(&self) -> Result<HooksState, InstallError> {
        let found = kiko_entries(&read_settings(&self.settings_path)?.json);
        Ok(HooksState {
            installed: !found.is_empty(),
            up_to_date: found == kiko_hooks(self.port, &self.auth_header),
        })
    }

    pub fn preview(&self, action: Action) -> Result<Preview, InstallError> {
        if action == Action::Install {
            self.curl()?;
        }
        let settings = read_settings(&self.settings_path)?;
        let existing = kiko_entries(&settings.json);
        let existing_handlers: Vec<&Value> = existing
            .values()
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|group| group.get("hooks")?.as_array())
            .flatten()
            .collect();
        let replaces_http =
            action == Action::Install && existing_handlers.iter().any(|handler| is_http(handler));
        let adds_process_id = action == Action::Install
            && existing_handlers.iter().any(|handler| {
                !is_http(handler)
                    && !handler
                        .get("command")
                        .and_then(Value::as_str)
                        .is_some_and(|command| command.contains(PID_HEADER))
            });
        let entries = match action {
            Action::Install => kiko_hooks(self.port, &self.auth_header),
            Action::Uninstall => existing.clone(),
        };
        Ok(Preview {
            action,
            settings_path: self.settings_path.display().to_string(),
            file_exists: settings.text.is_some(),
            entries: serde_json::to_string_pretty(&json!({ "hooks": entries })).unwrap_or_default(),
            replaces_existing: action == Action::Install && !existing.is_empty(),
            replaces_http,
            adds_process_id,
            nothing_to_do: self.target(&settings.json, action).0 == settings.json,
            fingerprint: fingerprint(settings.text.as_deref()),
        })
    }

    /// Backs up settings.json, then writes it with Kiko's hooks added or removed.
    pub fn apply(
        &self,
        action: Action,
        expected_fingerprint: &str,
        now: SystemTime,
    ) -> Result<Applied, InstallError> {
        if action == Action::Install {
            self.curl()?;
        }
        let settings = read_settings(&self.settings_path)?;
        if fingerprint(settings.text.as_deref()) != expected_fingerprint {
            return Err(InstallError::ChangedSincePreview);
        }
        let (updated, record) = self.target(&settings.json, action);
        let changed = updated != settings.json || settings.text.is_none();
        let backup_path = match (&settings.text, changed) {
            (Some(_), true) => Some(backup(&self.settings_path, now)?),
            _ => None,
        };
        if changed {
            write_atomically(
                &self.settings_path,
                &render(&updated, settings.text.as_deref()),
            )?;
        }
        match record {
            Some(record) => {
                if let Some(dir) = self.record_path.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(
                    &self.record_path,
                    serde_json::to_string_pretty(&record).unwrap_or_default(),
                )?;
            }
            None => match fs::remove_file(&self.record_path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error.into()),
                _ => {}
            },
        }
        Ok(Applied {
            changed,
            backup_path: backup_path.map(|p| p.display().to_string()),
        })
    }
}

/// Pretty JSON with 2-space indentation, as Claude Code writes it, keeping the key order and the
/// original's final newline.
fn render(settings: &Map<String, Value>, original: Option<&str>) -> String {
    let mut text =
        serde_json::to_string_pretty(&Value::Object(settings.clone())).unwrap_or_default();
    if original.is_none_or(|original| original.ends_with('\n')) {
        text.push('\n');
    }
    text
}

/// Copies settings.json to `settings.json.kiko-backup-<UTC timestamp>` next to it.
fn backup(path: &Path, now: SystemTime) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json");
    let base = format!("{name}.kiko-backup-{}", utc_stamp(now));
    let mut backup = path.with_file_name(&base);
    let mut n = 1;
    while backup.exists() {
        backup = path.with_file_name(format!("{base}-{n}"));
        n += 1;
    }
    fs::copy(path, &backup)?;
    Ok(backup)
}

/// Writes through a temporary file and a rename, so Claude Code never reads a half-written file.
fn write_atomically(path: &Path, text: &str) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json");
    let temp = path.with_file_name(format!("{name}.kiko-tmp"));
    fs::write(&temp, text)?;
    if let Ok(metadata) = fs::metadata(path) {
        fs::set_permissions(&temp, metadata.permissions())?;
    }
    fs::rename(&temp, path)
}

/// `20261012T143005Z`.
fn utc_stamp(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let rest = secs % 86_400;
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

/// Days since 1970-01-01 → (year, month, day), Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const PORT: u16 = 47821;
    const AUTH: &str = "\"$HOME/.config/kiko/auth-header\"";

    /// A user's settings with their own hook, in Claude Code's own format (2-space JSON, final newline).
    const USER_SETTINGS: &str = r#"{
  "model": "opus",
  "permissions": {
    "allow": [
      "Bash(npm run lint)"
    ]
  },
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PROJECT_DIR}/.claude/hooks/block-rm.sh"
          }
        ]
      }
    ],
    "Stop": []
  },
  "statusLine": {
    "type": "command",
    "command": "~/.claude/statusline.sh"
  }
}
"#;

    fn settings_in(dir: &Path, text: Option<&str>) -> PathBuf {
        let path = dir.join(".claude").join("settings.json");
        if let Some(text) = text {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
        }
        path
    }

    fn installer(path: &Path, port: u16) -> Installer {
        Installer {
            settings_path: path.to_path_buf(),
            record_path: path
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("kiko")
                .join("hooks-install.json"),
            port,
            auth_header: AUTH.into(),
            curl: "curl".into(),
        }
    }

    fn run_with(installer: &Installer, action: Action, at: u64) -> Applied {
        let preview = installer.preview(action).unwrap();
        installer
            .apply(
                action,
                &preview.fingerprint,
                UNIX_EPOCH + Duration::from_secs(at),
            )
            .unwrap()
    }

    fn run(path: &Path, action: Action, at: u64) -> Applied {
        run_with(&installer(path, PORT), action, at)
    }

    fn read_json(path: &Path) -> Value {
        serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
    }

    fn backups(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir.join(".claude"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.contains(".kiko-backup-"))
            .collect();
        names.sort();
        names
    }

    fn all_handlers(settings: &Value) -> Vec<Value> {
        settings["hooks"]
            .as_object()
            .into_iter()
            .flat_map(|hooks| hooks.values())
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|group| group["hooks"].as_array())
            .flatten()
            .cloned()
            .collect()
    }

    /// What the http-hooks version of Kiko wrote: its entries (no SessionStart), and its install record.
    fn legacy_http_install(path: &Path, token: &str) {
        let mut settings: Map<String, Value> =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        let http = |hook: Hook, timeout: Option<u64>, status: Option<&str>| {
            let mut handler = json!({
                "type": "http",
                "url": format!("http://127.0.0.1:{PORT}/kiko/hooks/{}", hook.path()),
                "headers": { "X-Kiko-Token": token }
            });
            if let Some(timeout) = timeout {
                handler["timeout"] = json!(timeout);
            }
            if let Some(status) = status {
                handler["statusMessage"] = json!(status);
            }
            handler
        };
        let hooks = settings.get_mut("hooks").unwrap().as_object_mut().unwrap();
        let mut created = Vec::new();
        for hook in Hook::ALL.into_iter().filter(|h| *h != Hook::SessionStart) {
            let groups = match hook {
                Hook::PreToolUse => vec![
                    json!({ "matcher": "AskUserQuestion", "hooks": [http(hook, Some(120), Some("Waiting for your answer in Kiko"))] }),
                    json!({ "matcher": "^(?!AskUserQuestion$)", "hooks": [http(hook, Some(5), None)] }),
                ],
                Hook::PermissionRequest => {
                    vec![
                        json!({ "hooks": [http(hook, Some(120), Some("Waiting for your decision in Kiko"))] }),
                    ]
                }
                Hook::SessionEnd => vec![json!({ "hooks": [http(hook, None, None)] })],
                _ => vec![json!({ "hooks": [http(hook, Some(5), None)] })],
            };
            if !hooks.contains_key(hook.event_name()) {
                created.push(hook.event_name().to_string());
            }
            hooks
                .entry(hook.event_name())
                .or_insert(json!([]))
                .as_array_mut()
                .unwrap()
                .extend(groups);
        }
        fs::write(path, render(&settings, Some("\n"))).unwrap();
        let record = InstallRecord {
            settings_path: path.display().to_string(),
            created_hooks_object: false,
            created_events: created,
        };
        let installer = installer(path, PORT);
        fs::create_dir_all(installer.record_path.parent().unwrap()).unwrap();
        fs::write(
            &installer.record_path,
            serde_json::to_string(&record).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn hooks_are_silent_curl_commands_with_no_token() {
        let hooks = kiko_hooks(PORT, AUTH);
        assert_eq!(
            hooks.len(),
            Hook::ALL.len(),
            "every event, SessionStart included"
        );
        assert_eq!(
            hooks["PermissionRequest"][0]["hooks"][0],
            json!({
                "type": "command",
                "command": "curl -sf --max-time 115 -H @\"$HOME/.config/kiko/auth-header\" -H \"Content-Type: application/json\" -H \"X-Kiko-Claude-Pid: $PPID\" --data-binary @- http://127.0.0.1:47821/kiko/hooks/permission-request || true",
                "timeout": 120,
                "statusMessage": "Waiting for your decision in Kiko"
            })
        );
        assert_eq!(hooks["PreToolUse"][0]["matcher"], "AskUserQuestion");
        assert!(
            hooks["PreToolUse"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains("--max-time 115 ")
        );
        assert_eq!(hooks["PreToolUse"][1]["matcher"], "^(?!AskUserQuestion$)");
        assert_eq!(
            hooks["SessionStart"][0]["hooks"][0],
            json!({
                "type": "command",
                "command": "curl -sf --max-time 2 -H @\"$HOME/.config/kiko/auth-header\" -H \"Content-Type: application/json\" -H \"X-Kiko-Claude-Pid: $PPID\" --data-binary @- http://127.0.0.1:47821/kiko/hooks/session-start || true",
                "timeout": 3
            })
        );
        let text = serde_json::to_string(&hooks).unwrap();
        assert!(
            !text.contains("X-Kiko-Token"),
            "the token stays in the header file"
        );
        // Each hook's timeout is above its curl limit.
        for handler in all_handlers(&json!({ "hooks": hooks })) {
            let command = handler["command"].as_str().unwrap();
            let curl: u64 = command
                .split("--max-time ")
                .nth(1)
                .unwrap()
                .split(' ')
                .next()
                .unwrap()
                .parse()
                .unwrap();
            assert!(handler["timeout"].as_u64().unwrap() > curl, "{command}");
            assert!(command.ends_with(" || true"));
            assert!(is_kiko_handler(&handler));
        }
    }

    #[test]
    fn shell_paths() {
        let home = Path::new("/home/me");
        assert_eq!(
            shell_path(Path::new("/home/me/.config/kiko/auth-header"), home),
            AUTH
        );
        assert_eq!(
            shell_path(Path::new("/srv/kiko/auth-header"), home),
            "\"/srv/kiko/auth-header\""
        );
        assert_eq!(
            shell_path(Path::new("/srv/a \"b\" $c/h"), home),
            r#""/srv/a \"b\" \$c/h""#
        );
    }

    #[test]
    fn install_keeps_the_user_settings_and_hooks_and_makes_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));

        let applied = run(&path, Action::Install, 1_791_800_000);
        assert!(applied.changed);
        assert_eq!(
            backups(dir.path()),
            ["settings.json.kiko-backup-20261012T101320Z"]
        );
        assert_eq!(
            fs::read_to_string(applied.backup_path.unwrap()).unwrap(),
            USER_SETTINGS
        );

        let after = read_json(&path);
        let before: Value = serde_json::from_str(USER_SETTINGS).unwrap();
        for key in ["model", "permissions", "statusLine"] {
            assert_eq!(after[key], before[key], "{key} changed");
        }
        // The user's PreToolUse group is first and untouched; Kiko's two groups follow.
        let pre = after["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0], before["hooks"]["PreToolUse"][0]);
        assert_eq!(pre.len(), 3);
        assert!(
            pre[1..]
                .iter()
                .all(|g| g["hooks"].as_array().unwrap().iter().all(is_kiko_handler))
        );
        assert_eq!(after["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(
            after["hooks"]["SessionStart"],
            kiko_hooks(PORT, AUTH)["SessionStart"]
        );
        assert_eq!(
            installer(&path, PORT).state().unwrap(),
            HooksState {
                installed: true,
                up_to_date: true
            }
        );
    }

    #[test]
    fn uninstall_leaves_the_file_as_it_was() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        run(&path, Action::Install, 1_791_800_000);
        let applied = run(&path, Action::Uninstall, 1_791_800_060);
        assert!(applied.changed);
        assert_eq!(fs::read_to_string(&path).unwrap(), USER_SETTINGS);
        assert_eq!(backups(dir.path()).len(), 2);
        assert_eq!(
            installer(&path, PORT).state().unwrap(),
            HooksState {
                installed: false,
                up_to_date: false
            }
        );
    }

    #[test]
    fn reinstall_migrates_the_old_http_hooks_and_uninstall_still_restores_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        legacy_http_install(&path, &"a".repeat(64));
        let installer = installer(&path, PORT);
        assert_eq!(
            installer.state().unwrap(),
            HooksState {
                installed: true,
                up_to_date: false
            }
        );

        let preview = installer.preview(Action::Install).unwrap();
        assert!(preview.replaces_existing && preview.replaces_http);
        let legacy = fs::read_to_string(&path).unwrap();
        let applied = installer
            .apply(
                Action::Install,
                &preview.fingerprint,
                UNIX_EPOCH + Duration::from_secs(1_791_800_000),
            )
            .unwrap();
        assert_eq!(
            fs::read_to_string(applied.backup_path.unwrap()).unwrap(),
            legacy
        );

        let after = read_json(&path);
        let handlers = all_handlers(&after);
        assert!(
            handlers.iter().all(|h| h["type"] != "http"),
            "no http hook left"
        );
        assert!(
            !fs::read_to_string(&path).unwrap().contains("X-Kiko-Token"),
            "no token left"
        );
        assert_eq!(
            handlers.iter().filter(|h| is_kiko_handler(h)).count(),
            Hook::ALL.len() + 1
        );
        assert_eq!(
            after["hooks"]["PreToolUse"][0],
            serde_json::from_str::<Value>(USER_SETTINGS).unwrap()["hooks"]["PreToolUse"][0]
        );
        assert_eq!(
            installer.state().unwrap(),
            HooksState {
                installed: true,
                up_to_date: true
            }
        );

        // The user's empty "Stop" list, and the absence of every other event, come back exactly.
        run_with(&installer, Action::Uninstall, 1_791_800_060);
        assert_eq!(fs::read_to_string(&path).unwrap(), USER_SETTINGS);
    }

    #[test]
    fn reinstall_adds_the_process_id_to_the_commands_of_the_previous_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        let installer = installer(&path, PORT);
        // What the previous version wrote: the same curl commands without the pid header.
        run_with(&installer, Action::Install, 1_791_800_000);
        let previous = fs::read_to_string(&path)
            .unwrap()
            .replace(" -H \\\"X-Kiko-Claude-Pid: $PPID\\\"", "");
        assert!(!previous.contains("PPID"));
        fs::write(&path, &previous).unwrap();
        assert_eq!(
            installer.state().unwrap(),
            HooksState {
                installed: true,
                up_to_date: false
            }
        );

        let preview = installer.preview(Action::Install).unwrap();
        assert!(preview.replaces_existing && preview.adds_process_id && !preview.replaces_http);
        installer
            .apply(
                Action::Install,
                &preview.fingerprint,
                UNIX_EPOCH + Duration::from_secs(1_791_800_060),
            )
            .unwrap();
        let handlers = all_handlers(&read_json(&path));
        let kiko: Vec<&Value> = handlers.iter().filter(|h| is_kiko_handler(h)).collect();
        assert_eq!(kiko.len(), Hook::ALL.len() + 1, "replaced, not duplicated");
        assert!(
            kiko.iter()
                .all(|h| h["command"].as_str().unwrap().contains("$PPID"))
        );
        assert!(installer.state().unwrap().up_to_date);
        assert!(!installer.preview(Action::Install).unwrap().adds_process_id);

        run_with(&installer, Action::Uninstall, 1_791_800_120);
        assert_eq!(fs::read_to_string(&path).unwrap(), USER_SETTINGS);
    }

    #[test]
    fn old_http_hooks_are_found_even_without_the_install_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        legacy_http_install(&path, &"a".repeat(64));
        fs::remove_file(installer(&path, PORT).record_path).unwrap();
        run(&path, Action::Uninstall, 1_791_800_000);
        let after = read_json(&path);
        let before: Value = serde_json::from_str(USER_SETTINGS).unwrap();
        assert_eq!(after["hooks"]["PreToolUse"], before["hooks"]["PreToolUse"]);
        assert!(all_handlers(&after).iter().all(|h| !is_kiko_handler(h)));
    }

    #[test]
    fn installing_twice_replaces_instead_of_duplicating() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        run(&path, Action::Install, 1_791_800_000);
        let once = fs::read_to_string(&path).unwrap();
        assert!(
            installer(&path, PORT)
                .preview(Action::Install)
                .unwrap()
                .replaces_existing
        );
        let again = run(&path, Action::Install, 1_791_800_001);
        assert!(!again.changed, "an identical reinstall writes nothing");
        assert_eq!(fs::read_to_string(&path).unwrap(), once);

        // A new port updates Kiko's entries in place.
        let moved = installer(&path, 50000);
        run_with(&moved, Action::Install, 1_791_800_002);
        let after = read_json(&path);
        assert_eq!(after["hooks"]["PreToolUse"].as_array().unwrap().len(), 3);
        assert!(
            after["hooks"]["Stop"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .contains(":50000/")
        );
        assert_eq!(
            installer(&path, PORT).state().unwrap(),
            HooksState {
                installed: true,
                up_to_date: false
            }
        );
        run_with(&moved, Action::Uninstall, 1_791_800_003);
        assert_eq!(fs::read_to_string(&path).unwrap(), USER_SETTINGS);
    }

    #[test]
    fn without_the_record_uninstall_still_removes_only_kiko() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        run(&path, Action::Install, 1_791_800_000);
        fs::remove_file(installer(&path, PORT).record_path).unwrap();
        run(&path, Action::Uninstall, 1_791_800_001);
        let after = read_json(&path);
        let before: Value = serde_json::from_str(USER_SETTINGS).unwrap();
        assert_eq!(after["hooks"]["PreToolUse"], before["hooks"]["PreToolUse"]);
        assert_eq!(after["statusLine"], before["statusLine"]);
        assert!(
            installer(&path, PORT).state().unwrap()
                == HooksState {
                    installed: false,
                    up_to_date: false
                }
        );
    }

    #[test]
    fn a_missing_file_is_created_and_emptied_without_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), None);
        let applied = run(&path, Action::Install, 1_791_800_000);
        assert_eq!(applied.backup_path, None);
        assert_eq!(
            read_json(&path)["hooks"].as_object().unwrap().len(),
            Hook::ALL.len()
        );
        run(&path, Action::Uninstall, 1_791_800_001);
        assert_eq!(read_json(&path), json!({}));
    }

    #[test]
    fn without_curl_nothing_is_installed() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        let mut no_curl = installer(&path, PORT);
        no_curl.curl = "kiko-test-no-such-curl".into();
        let error = no_curl.preview(Action::Install).unwrap_err();
        assert!(matches!(error, InstallError::Curl(CurlProblem::Missing)));
        assert!(
            error.to_string().contains("curl, which is not installed"),
            "{error}"
        );
        assert!(matches!(
            no_curl.apply(
                Action::Install,
                &fingerprint(Some(USER_SETTINGS)),
                SystemTime::now()
            ),
            Err(InstallError::Curl(_))
        ));
        assert_eq!(fs::read_to_string(&path).unwrap(), USER_SETTINGS);
        // Removing the hooks never needs curl.
        assert!(no_curl.preview(Action::Uninstall).is_ok());
    }

    #[test]
    fn the_installed_curl_is_recent_enough() {
        let version = check_curl("curl").expect("curl is needed to run these tests");
        assert!(!version.is_empty());
    }

    #[test]
    fn nothing_is_written_when_the_file_changed_since_the_preview() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some(USER_SETTINGS));
        let installer = installer(&path, PORT);
        let preview = installer.preview(Action::Install).unwrap();
        fs::write(&path, "{\n  \"model\": \"sonnet\"\n}\n").unwrap();
        let result = installer.apply(Action::Install, &preview.fingerprint, SystemTime::now());
        assert!(matches!(result, Err(InstallError::ChangedSincePreview)));
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "{\n  \"model\": \"sonnet\"\n}\n"
        );
        assert!(backups(dir.path()).is_empty());
    }

    #[test]
    fn invalid_settings_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let path = settings_in(dir.path(), Some("{ \"model\": "));
        let installer = installer(&path, PORT);
        assert!(matches!(
            installer.preview(Action::Install),
            Err(InstallError::Invalid(_))
        ));
        fs::write(&path, r#"{ "hooks": { "Stop": {} } }"#).unwrap();
        assert!(matches!(
            installer.preview(Action::Install),
            Err(InstallError::Invalid(_))
        ));
    }

    #[test]
    fn settings_path_follows_claude_config_dir() {
        let home = Path::new("/home/me");
        assert_eq!(
            claude_settings_path(home, None),
            Path::new("/home/me/.claude/settings.json")
        );
        assert_eq!(
            claude_settings_path(home, Some("")),
            Path::new("/home/me/.claude/settings.json")
        );
        assert_eq!(
            claude_settings_path(home, Some("/tmp/cc")),
            Path::new("/tmp/cc/settings.json")
        );
    }

    #[test]
    fn utc_timestamps() {
        assert_eq!(utc_stamp(UNIX_EPOCH), "19700101T000000Z");
        assert_eq!(
            utc_stamp(UNIX_EPOCH + Duration::from_secs(951_868_800)),
            "20000301T000000Z"
        );
        assert_eq!(
            utc_stamp(UNIX_EPOCH + Duration::from_secs(1_791_800_000)),
            "20261012T101320Z"
        );
    }
}
