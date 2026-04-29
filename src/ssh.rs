use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};

use crate::config::{Defaults, Group, Server};

#[derive(Debug, Clone)]
pub struct KeyInfo {
    pub algorithm: String,
    pub fingerprint: String,
}

pub fn connect(group: &Group, server: &Server, defaults: &Defaults) -> Result<(ExitStatus, String)> {
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

    let mut child = cmd
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn ssh")?;
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let captured = Arc::new(Mutex::new(String::new()));
    let captured_clone = captured.clone();
    let tee = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stderr_pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let _ = std::io::stderr().write_all(&buf[..n]);
                    let _ = std::io::stderr().flush();
                    if let Ok(mut s) = captured_clone.lock() {
                        s.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
            }
        }
    });
    let status = child.wait().context("failed to wait for ssh")?;
    let _ = tee.join();
    let stderr_text = captured.lock().map(|s| s.clone()).unwrap_or_default();
    Ok((status, stderr_text))
}

pub fn classify_failure(stderr: &str) -> String {
    let lower = stderr.to_lowercase();

    if lower.contains("could not resolve hostname")
        || lower.contains("name or service not known")
        || lower.contains("nodename nor servname")
    {
        return "DNS lookup failed — host name could not be resolved.".into();
    }
    if lower.contains("connection refused") {
        return "Connection refused — nothing is listening on the SSH port.".into();
    }
    if lower.contains("no route to host") {
        return "No route to host.".into();
    }
    if lower.contains("network is unreachable") {
        return "Network is unreachable.".into();
    }
    if lower.contains("operation timed out") || lower.contains("connection timed out") {
        return "Connection timed out — host did not respond.".into();
    }
    if lower.contains("remote host identification has changed") {
        return "Host key changed — server presented a different key than known_hosts.\n\
                If this was expected (e.g. server rebuild): ssh-keygen -R <host>"
            .into();
    }
    if lower.contains("host key verification failed") {
        return "Host key verification failed — known_hosts entry doesn't match.\n\
                Remove the stale entry: ssh-keygen -R <host>"
            .into();
    }
    if lower.contains("too many authentication failures") {
        return "Too many authentication failures — server rejected all offered keys.".into();
    }
    if lower.contains("permission denied") {
        return "Authentication denied.".into();
    }
    if lower.contains("kex_exchange_identification") {
        return "SSH handshake failed — peer did not respond as an SSH server.".into();
    }
    if lower.contains("bad configuration option") || lower.contains("bad option") {
        return "Invalid SSH option in the `flags` field.".into();
    }
    if lower.contains("identity file") && lower.contains("not accessible") {
        return "Identity file not accessible — key path is wrong or unreadable.".into();
    }
    if lower.contains("unprotected private key file") {
        return "Private key file has too-permissive permissions.\n\
                Fix with: chmod 600 <key-file>"
            .into();
    }

    let last = stderr.lines().rev().find(|l| !l.trim().is_empty());
    match last {
        Some(l) => format!("ssh exited 255. Last message:\n  {}", l.trim()),
        None => "ssh exited 255 with no diagnostic output.".into(),
    }
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
