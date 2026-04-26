use anyhow::{Context, Result};
use std::process::{Command, ExitStatus, Stdio};

use crate::config::{Defaults, Group, Server};

#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub algorithm: String,
    pub fingerprint: String,
}

pub fn connect(group: &Group, server: &Server, defaults: &Defaults) -> Result<ExitStatus> {
    let user = server.resolved_user(group, defaults);
    let key = server.resolved_key(group, defaults);
    let port = server.resolved_port(defaults);

    let target = match user {
        Some(u) => format!("{}@{}", u, server.host),
        None => server.host.clone(),
    };

    let mut cmd = Command::new("ssh");
    cmd.arg(&target);
    if port != 22 {
        cmd.arg("-p").arg(port.to_string());
    }
    if let Some(k) = key {
        cmd.arg("-i").arg(expand_tilde(&k));
    }
    if let Some(flags) = server.flags.as_deref() {
        for arg in flags.split_whitespace() {
            cmd.arg(arg);
        }
    }
    cmd.status().context("failed to spawn ssh")
}

pub fn command_preview(group: &Group, server: &Server, defaults: &Defaults) -> String {
    let user = server.resolved_user(group, defaults);
    let key = server.resolved_key(group, defaults);
    let port = server.resolved_port(defaults);
    let mut out = String::from("ssh ");
    if let Some(u) = user {
        out.push_str(&u);
        out.push('@');
    }
    out.push_str(&server.host);
    if port != 22 {
        out.push_str(&format!(" -p {}", port));
    }
    if let Some(k) = key {
        out.push_str(&format!(" -i {}", k));
    }
    if let Some(flags) = server.flags.as_deref() {
        if !flags.is_empty() {
            out.push(' ');
            out.push_str(flags);
        }
    }
    out
}

pub fn expand_tilde(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{}/{}", home, stripped);
        }
    }
    path.to_string()
}

pub fn discover_ssh_keys() -> Vec<String> {
    let Ok(home) = std::env::var("HOME") else {
        return Vec::new();
    };
    let dir = std::path::Path::new(&home).join(".ssh");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };

    let mut keys = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        if name.starts_with('.') || name.ends_with(".pub") {
            continue;
        }
        if name.chars().any(|c| c.is_control()) {
            continue;
        }
        if !looks_like_private_key(&path) {
            continue;
        }
        keys.push(format!("~/.ssh/{}", name));
    }
    keys.sort();
    keys
}

pub fn key_fingerprint(path: &str) -> Option<KeyInfo> {
    let expanded = expand_tilde(path);
    let pub_path = format!("{}.pub", expanded);
    if !std::path::Path::new(&pub_path).exists() {
        return None;
    }
    let output = Command::new("ssh-keygen")
        .args(["-l", "-f", &pub_path])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_keygen_line(&String::from_utf8_lossy(&output.stdout))
}

fn parse_keygen_line(out: &str) -> Option<KeyInfo> {
    let line = out.lines().next()?.trim();
    let mut tokens = line.splitn(3, ' ');
    let _bits = tokens.next()?;
    let fingerprint = tokens.next()?.to_string();
    let rest = tokens.next()?;
    let algorithm = rest
        .rfind('(')
        .filter(|_| rest.ends_with(')'))
        .map(|start| rest[start + 1..rest.len() - 1].to_string())
        .unwrap_or_default();
    Some(KeyInfo {
        algorithm,
        fingerprint,
    })
}

fn looks_like_private_key(path: &std::path::Path) -> bool {
    use std::io::BufRead;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut reader = std::io::BufReader::new(file);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line).is_err() {
        return false;
    }
    first_line.starts_with("-----BEGIN")
}
