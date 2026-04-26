use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

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
                fs::create_dir_all(parent)
                    .with_context(|| format!("create {}", parent.display()))?;
            }
            fs::write(&path, DEFAULT_CONFIG)
                .with_context(|| format!("write {}", path.display()))?;
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))?;
        let config: Config = toml::from_str(&raw)
            .with_context(|| format!("parse {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
        let body = toml::to_string_pretty(self).context("serialize config")?;
        fs::write(&path, body).with_context(|| format!("write {}", path.display()))?;
        Ok(())
    }
}

pub fn config_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "shh")
        .context("could not determine config directory")?;
    Ok(dirs.config_dir().join("config.toml"))
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
