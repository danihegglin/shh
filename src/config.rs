use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub defaults: Defaults,
    #[serde(default, rename = "group")]
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Defaults {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Group {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, rename = "server")]
    pub servers: Vec<Server>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub name: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Server {
    pub fn resolved_user(&self, group: &Group, defaults: &Defaults) -> Option<String> {
        self.user
            .clone()
            .or_else(|| group.user.clone())
            .or_else(|| defaults.user.clone())
    }

    pub fn resolved_key(&self, group: &Group, defaults: &Defaults) -> Option<String> {
        self.key
            .clone()
            .or_else(|| group.key.clone())
            .or_else(|| defaults.key.clone())
    }

    pub fn resolved_port(&self, defaults: &Defaults) -> u16 {
        self.port.or(defaults.port).unwrap_or(22)
    }
}

impl Config {
    pub fn load_or_init() -> Result<Self> {
        let path = config_path()?;
        if !path.exists() {
            if let Some(parent) = path.parent() {
                ensure_parent(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            write_secure(&path, DEFAULT_CONFIG.as_bytes())
                .with_context(|| format!("write {}", path.display()))?;
        }
        tighten_permissions(&path);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let mut config: Config = toml::from_str(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        sanitize_config(&mut config);
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            ensure_parent(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let body = toml::to_string_pretty(self).context("serialize config")?;
        write_secure(&path, body.as_bytes())
            .with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

fn ensure_parent(parent: &Path) -> std::io::Result<()> {
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
    Ok(())
}

#[cfg(unix)]
fn write_secure(path: &Path, body: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(body)?;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    Ok(())
}

#[cfg(not(unix))]
fn write_secure(path: &Path, body: &[u8]) -> std::io::Result<()> {
    fs::write(path, body)
}

#[cfg(unix)]
fn tighten_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o600));
    if let Some(parent) = path.parent() {
        let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
    }
}

#[cfg(not(unix))]
fn tighten_permissions(_path: &Path) {}

fn sanitize_config(c: &mut Config) {
    sanitize_opt(&mut c.defaults.user);
    sanitize_opt(&mut c.defaults.key);
    for g in &mut c.groups {
        sanitize_str(&mut g.name);
        sanitize_opt(&mut g.icon);
        sanitize_opt(&mut g.user);
        sanitize_opt(&mut g.key);
        for s in &mut g.servers {
            sanitize_str(&mut s.name);
            sanitize_str(&mut s.host);
            sanitize_opt(&mut s.user);
            sanitize_opt(&mut s.key);
            sanitize_opt(&mut s.flags);
            sanitize_opt(&mut s.description);
            for t in &mut s.tags {
                sanitize_str(t);
            }
        }
    }
}

fn sanitize_str(s: &mut String) {
    if s.chars().any(is_unsafe_control) {
        *s = s
            .chars()
            .map(|c| if is_unsafe_control(c) { '?' } else { c })
            .collect();
    }
}

fn sanitize_opt(s: &mut Option<String>) {
    if let Some(v) = s {
        sanitize_str(v);
    }
}

fn is_unsafe_control(c: char) -> bool {
    c.is_control() && c != '\t'
}

pub fn config_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .or_else(|| std::env::var_os("USERPROFILE").map(|h| PathBuf::from(h).join(".config")))
        .context("could not determine config directory")?;
    Ok(base.join("shh").join("config.toml"))
}

const DEFAULT_CONFIG: &str = r#"# shh — ssh connection manager
# Edit this file to manage your hosts. Reload by restarting shh.

[defaults]
# user = "your-username"
# key  = "~/.ssh/id_ed25519"
# port = 22

[[group]]
name = "Examples"
icon = "✦"
# user = "deploy"   # group-level default, overridden by [defaults]
# key  = "~/.ssh/group_key"

[[group.server]]
name = "github"
host = "github.com"
user = "git"
description = "test your ssh key against github"
tags = ["test"]

[[group.server]]
name = "localhost"
host = "127.0.0.1"
description = "loop back home"
"#;
