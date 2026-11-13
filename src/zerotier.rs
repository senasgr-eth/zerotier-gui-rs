// ZeroTier-GUI — a cross-platform front-end for ZeroTier.
//
// Rust + GPUI rewrite Copyright (C) 2026 Senasgr.
// Based on the original ZeroTier-GUI by Tomás Ralph (GPL-3.0-or-later).
// See NOTICE and LICENSE for attribution and licensing details.

//! Thin, typed wrapper around the ZeroTier command line tools.
//!
//! Everything in this module shells out to `zerotier-cli`, `systemctl` and
//! `pkexec`.  Commands are expected to be run from a background thread; see
//! `app::ZeroTierGui::spawn_blocking`.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output};

use serde::Deserialize;

pub const CLI: &str = "zerotier-cli";
#[cfg(target_os = "linux")]
pub const SERVICE: &str = "zerotier-one";
pub const CENTRAL_URL: &str = "https://my.zerotier.com";
pub const REPOSITORY_URL: &str = "https://github.com/senasgr-eth/zerotier-gui-rs";
pub const ORIGINAL_URL: &str = "https://github.com/tralph3/ZeroTier-GUI";

/// Why a command failed.  The variants mirror the exit codes `zerotier-cli`
/// uses for the situations the GUI knows how to explain to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliErrorKind {
    /// `zerotier-cli` is not on PATH.
    NotInstalled,
    /// The service is not running.
    ServiceDown,
    /// The user cannot read the ZeroTier authentication token.
    AccessDenied,
    /// A command failed for any other reason.
    Other,
    /// The command succeeded but produced output we could not parse.
    Parse,
}

#[derive(Debug, Clone)]
pub struct CliError {
    pub kind: CliErrorKind,
    pub message: String,
}

impl CliError {
    fn new(kind: CliErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// An action that only makes sense on one platform (for example `pkexec`
    /// interface management on Linux).
    #[allow(dead_code)]
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(CliErrorKind::Other, message)
    }

    fn from_output(program: &str, output: &Output, cli_exit_codes: bool) -> Self {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let message = if stderr.is_empty() { stdout } else { stderr };

        let kind = if cli_exit_codes {
            match output.status.code() {
                Some(1) => CliErrorKind::ServiceDown,
                Some(2) => CliErrorKind::AccessDenied,
                Some(127) => CliErrorKind::NotInstalled,
                _ => CliErrorKind::Other,
            }
        } else {
            CliErrorKind::Other
        };

        let message = if message.is_empty() {
            format!("{program} exited with status {}", output.status)
        } else {
            message
        };

        Self::new(kind, message)
    }

    /// A short, user-facing explanation followed by whatever the command
    /// printed.
    pub fn detail(&self) -> String {
        let summary = match self.kind {
            CliErrorKind::NotInstalled => "ZeroTier isn't installed",
            CliErrorKind::ServiceDown => "The 'zerotier-one' service isn't running",
            CliErrorKind::AccessDenied => "This user doesn't have access to ZeroTier",
            CliErrorKind::Other => "The command failed",
            CliErrorKind::Parse => "Couldn't understand ZeroTier's output",
        };

        if self.message.is_empty() {
            summary.to_string()
        } else {
            format!("{summary}.\n\n{}", self.message)
        }
    }
}

fn run_with<S: AsRef<OsStr>>(
    program: &str,
    args: &[S],
    cli_exit_codes: bool,
) -> Result<String, CliError> {
    let output = Command::new(program).args(args).output().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            CliError::new(
                CliErrorKind::NotInstalled,
                format!("'{program}' was not found on PATH"),
            )
        } else {
            CliError::new(
                CliErrorKind::Other,
                format!("could not run '{program}': {error}"),
            )
        }
    })?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(CliError::from_output(program, &output, cli_exit_codes))
    }
}

#[cfg(target_os = "linux")]
fn run<S: AsRef<OsStr>>(program: &str, args: &[S]) -> Result<String, CliError> {
    run_with(program, args, false)
}

fn run_cli<S: AsRef<OsStr>>(args: &[S]) -> Result<String, CliError> {
    run_with(CLI, args, true)
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub nwid: String,
    #[serde(default, deserialize_with = "null_to_empty_string")]
    pub name: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub port_device_name: String,
    #[serde(default, rename = "type")]
    pub network_type: String,
    #[serde(default)]
    pub mac: String,
    #[serde(default)]
    pub mtu: u64,
    #[serde(default)]
    pub dhcp: bool,
    #[serde(default)]
    pub bridge: bool,
    #[serde(default)]
    pub allow_default: bool,
    #[serde(default)]
    pub allow_global: bool,
    #[serde(default)]
    pub allow_managed: bool,
    #[serde(default, rename = "allowDNS")]
    pub allow_dns: bool,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub assigned_addresses: Vec<String>,
}

impl Network {
    /// The identifier used by `zerotier-cli`.
    pub fn key(&self) -> &str {
        if self.id.is_empty() {
            &self.nwid
        } else {
            &self.id
        }
    }

    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            "Unknown Name".to_string()
        } else {
            self.name.clone()
        }
    }
}

fn null_to_empty_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}

fn null_to_empty_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerPath {
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub expired: bool,
    #[serde(default)]
    pub last_receive: i64,
    #[serde(default)]
    pub last_send: i64,
    #[serde(default)]
    pub preferred: bool,
    #[serde(default)]
    pub trusted_path_id: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub latency: i64,
    // Kept for API completeness even though the UI does not show it yet.
    #[allow(dead_code)]
    #[serde(default, rename = "isBonded")]
    pub is_bonded: bool,
    #[serde(default, deserialize_with = "null_to_empty_vec")]
    pub paths: Vec<PeerPath>,
}

impl Peer {
    pub fn display_version(&self) -> String {
        if self.version == "-1.-1.-1" || self.version.is_empty() {
            "-".to_string()
        } else {
            self.version.clone()
        }
    }

    pub fn display_latency(&self) -> String {
        if self.latency <= 0 {
            "-".to_string()
        } else {
            format!("{} ms", self.latency)
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ClientStatus {
    pub address: String,
    pub version: String,
    pub state: String,
    pub online: bool,
}

impl ClientStatus {
    pub fn display_address(&self) -> String {
        if self.address.is_empty() {
            "-".to_string()
        } else {
            self.address.clone()
        }
    }

    pub fn display_version(&self) -> String {
        if self.version.is_empty() {
            "-".to_string()
        } else {
            self.version.clone()
        }
    }

    pub fn display_state(&self) -> String {
        if self.state.is_empty() {
            "UNKNOWN".to_string()
        } else {
            self.state.clone()
        }
    }
}

/// Parse the single line printed by `zerotier-cli status`, for example:
/// `200 info 12ac4a1b2c 1.14.0 ONLINE`.
pub fn parse_status(output: &str) -> ClientStatus {
    let mut words = output.split_whitespace();
    let _code = words.next();
    let _info = words.next();
    let address = words.next().unwrap_or("-").to_string();
    let version = words.next().unwrap_or("-").to_string();
    let state = words.next().unwrap_or("UNKNOWN").to_string();

    ClientStatus {
        online: state.eq_ignore_ascii_case("ONLINE"),
        address,
        version,
        state,
    }
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub networks: Vec<Network>,
    pub interface_states: BTreeMap<String, String>,
    pub peers: Vec<Peer>,
    pub status: ClientStatus,
    pub service_state: String,
}

pub fn list_networks() -> Result<Vec<Network>, CliError> {
    let output = run_cli(&["-j", "listnetworks"])?;
    serde_json::from_str(&output).map_err(|error| {
        CliError::new(
            CliErrorKind::Parse,
            format!("could not parse 'zerotier-cli -j listnetworks': {error}"),
        )
    })
}

pub fn list_peers() -> Result<Vec<Peer>, CliError> {
    let output = run_cli(&["-j", "peers"])?;
    serde_json::from_str(&output).map_err(|error| {
        CliError::new(
            CliErrorKind::Parse,
            format!("could not parse 'zerotier-cli -j peers': {error}"),
        )
    })
}

pub fn client_status() -> Result<ClientStatus, CliError> {
    run_cli(&["status"]).map(|output| parse_status(&output))
}

/// Read every piece of data the main window needs, so a refresh is a single
/// background round-trip.
pub fn snapshot() -> Result<Snapshot, CliError> {
    let networks = list_networks()?;
    let interface_states = networks
        .iter()
        .filter(|network| !network.port_device_name.is_empty())
        .map(|network| {
            (
                network.key().to_string(),
                interface_state(&network.port_device_name),
            )
        })
        .collect();

    Ok(Snapshot {
        networks,
        interface_states,
        peers: list_peers().unwrap_or_default(),
        status: client_status().unwrap_or_default(),
        service_state: service_state(),
    })
}

/// `probe` is the cheap startup check: it only asks whether the CLI can talk
/// to a running service.
pub fn probe() -> Result<Vec<Network>, CliError> {
    list_networks()
}

pub fn join(network_id: &str) -> Result<(), CliError> {
    run_cli(&["join", network_id]).map(|_| ())
}

pub fn leave(network_id: &str) -> Result<(), CliError> {
    run_cli(&["leave", network_id]).map(|_| ())
}

pub fn set_network_option(network_id: &str, option: &str, value: bool) -> Result<(), CliError> {
    run_cli(&["set", network_id, &format!("{option}={}", u8::from(value))]).map(|_| ())
}

/// The `ActiveState` reported by systemd, or `"unknown"` when systemd (or the
/// unit) is unavailable.
#[cfg(target_os = "linux")]
pub fn service_state() -> String {
    match run(
        "systemctl",
        &["show", "--no-page", "--property=ActiveState", SERVICE],
    ) {
        Ok(output) => output
            .lines()
            .find_map(|line| line.strip_prefix("ActiveState="))
            .map(|state| state.trim().to_string())
            .filter(|state| !state.is_empty())
            .unwrap_or_else(|| "unknown".to_string()),
        Err(_) => "unknown".to_string(),
    }
}

/// On macOS and Windows the CLI only answers while the service is running, so
/// use that as a best-effort status indicator.
#[cfg(not(target_os = "linux"))]
pub fn service_state() -> String {
    if run_cli(&["status"]).is_ok() {
        "active".to_string()
    } else {
        "unknown".to_string()
    }
}

#[cfg(target_os = "linux")]
pub fn set_service_running(running: bool) -> Result<(), CliError> {
    run(
        "systemctl",
        &[if running { "start" } else { "stop" }, SERVICE],
    )
    .map(|_| ())
}

#[cfg(not(target_os = "linux"))]
pub fn set_service_running(_running: bool) -> Result<(), CliError> {
    Err(CliError::unsupported(
        "Starting and stopping the ZeroTier service is only supported on Linux",
    ))
}

/// The kernel's view of an interface, read straight from sysfs so a refresh
/// does not need another process.
///
/// TUN/TAP devices frequently report their `operstate` as `unknown` even when
/// the interface is administratively up, so fall back to the interface flags
/// in that case.
#[cfg(target_os = "linux")]
pub fn interface_state(interface: &str) -> String {
    if interface.is_empty() {
        return "UNKNOWN".to_string();
    }

    let base = Path::new("/sys/class/net").join(interface);
    let operstate = std::fs::read_to_string(base.join("operstate"))
        .map(|state| state.trim().to_string())
        .unwrap_or_default();

    if operstate.eq_ignore_ascii_case("up") || operstate.eq_ignore_ascii_case("down") {
        return operstate;
    }

    let Ok(flags) = std::fs::read_to_string(base.join("flags")) else {
        return "UNKNOWN".to_string();
    };

    // IFF_UP is 0x1.  A TUN device that is administratively up but has no
    // carrier still reports `unknown`, which is the state we care about.
    match u64::from_str_radix(flags.trim().trim_start_matches("0x"), 16) {
        Ok(flags) if flags & 0x1 != 0 => "up".to_string(),
        Ok(_) => "down".to_string(),
        Err(_) => "UNKNOWN".to_string(),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn interface_state(_interface: &str) -> String {
    "UNKNOWN".to_string()
}

#[cfg(target_os = "linux")]
pub fn set_interface_running(interface: &str, up: bool) -> Result<(), CliError> {
    run(
        "pkexec",
        &[
            "ip",
            "link",
            "set",
            interface,
            if up { "up" } else { "down" },
        ],
    )
    .map(|_| ())
}

#[cfg(not(target_os = "linux"))]
pub fn set_interface_running(_interface: &str, _up: bool) -> Result<(), CliError> {
    Err(CliError::unsupported(
        "Bringing ZeroTier interfaces up or down is only supported on Linux",
    ))
}

pub fn open_central() {
    let _ = open::that(CENTRAL_URL);
}

pub fn open_repository() {
    let _ = open::that(REPOSITORY_URL);
}

pub fn open_original_project() {
    let _ = open::that(ORIGINAL_URL);
}

pub fn authtoken_exists() -> bool {
    Path::new("/var/lib/zerotier-one/authtoken.secret").exists()
}

#[cfg(unix)]
pub fn is_root() -> bool {
    // SAFETY: geteuid has no preconditions and cannot fail.
    unsafe { libc::geteuid() == 0 }
}

#[cfg(not(unix))]
pub fn is_root() -> bool {
    false
}

pub fn username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .ok()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| "user".to_string())
}

/// Add the current user to the `zerotier-one` group and grant the group
/// access to the service secrets.  `pkexec` prompts for authentication.
#[cfg(target_os = "linux")]
pub fn grant_access(user: &str) -> Result<(), CliError> {
    let script = "usermod -aG zerotier-one \"$1\" \
                  && chmod 660 /var/lib/zerotier-one/authtoken.secret \
                  && chmod 660 /var/lib/zerotier-one/identity.secret";
    run("pkexec", &["sh", "-c", script, "zerotier-gui", user]).map(|_| ())
}

#[cfg(not(target_os = "linux"))]
pub fn grant_access(_user: &str) -> Result<(), CliError> {
    Err(CliError::unsupported(
        "Automatic permission setup is only supported on Linux",
    ))
}

/// Format a path timestamp (milliseconds since the Unix epoch) as a short
/// relative age.  Zero means the path has never been used.
pub fn format_age(timestamp_ms: i64) -> String {
    if timestamp_ms <= 0 {
        return "never".to_string();
    }

    let now_ms = chrono::Utc::now().timestamp_millis();
    let delta = now_ms.saturating_sub(timestamp_ms).max(0) / 1000;

    match delta {
        0..=4 => "just now".to_string(),
        5..=59 => format!("{delta}s ago"),
        60..=3599 => format!("{}m ago", delta / 60),
        3600..=86_399 => format!("{}h ago", delta / 3600),
        _ => format!("{}d ago", delta / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_online_status() {
        let status = parse_status("200 info 12ac4a1b2c 1.14.0 ONLINE");
        assert_eq!(status.address, "12ac4a1b2c");
        assert_eq!(status.version, "1.14.0");
        assert!(status.online);
        assert_eq!(status.state, "ONLINE");
    }

    #[test]
    fn parses_offline_status() {
        let status = parse_status("200 info deadbeef42 1.12.2 OFFLINE");
        assert!(!status.online);
        assert_eq!(status.display_state(), "OFFLINE");
    }

    #[test]
    fn unknown_names_are_friendly() {
        let mut network = Network::default();
        assert_eq!(network.display_name(), "Unknown Name");
        network.name = "  ".to_string();
        assert_eq!(network.display_name(), "Unknown Name");
        network.name = "Home".to_string();
        assert_eq!(network.display_name(), "Home");
    }

    #[test]
    fn peer_version_fallback() {
        let peer = Peer {
            version: "-1.-1.-1".to_string(),
            ..Default::default()
        };
        assert_eq!(peer.display_version(), "-");
    }

    #[test]
    fn parses_network_json() {
        let json = r#"[{
            "allowDefault": false,
            "allowDNS": true,
            "allowGlobal": false,
            "allowManaged": true,
            "assignedAddresses": ["10.42.0.7/24"],
            "bridge": false,
            "dhcp": true,
            "id": "8056c2e21c000001",
            "mac": "8e:aa:bb:cc:dd:ee",
            "mtu": 2800,
            "name": "Home",
            "nwid": "8056c2e21c000001",
            "portDeviceName": "ztabcdefgh",
            "status": "OK",
            "type": "PRIVATE"
        }]"#;

        let networks: Vec<Network> = serde_json::from_str(json).unwrap();
        let network = &networks[0];
        assert_eq!(network.key(), "8056c2e21c000001");
        assert_eq!(network.display_name(), "Home");
        assert_eq!(network.assigned_addresses, ["10.42.0.7/24"]);
        assert!(network.allow_dns);
        assert!(network.allow_managed);
        assert_eq!(network.port_device_name, "ztabcdefgh");
    }

    #[test]
    fn tolerates_null_fields() {
        let json = r#"[{"nwid":"abc","assignedAddresses":null,"name":null}]"#;
        let networks: Vec<Network> = serde_json::from_str(json).unwrap();
        assert!(networks[0].assigned_addresses.is_empty());
        assert_eq!(networks[0].name, "");
    }

    #[test]
    fn formats_relative_ages() {
        assert_eq!(format_age(0), "never");
        assert_eq!(
            format_age(chrono::Utc::now().timestamp_millis() - 2_000),
            "just now"
        );
    }
}
