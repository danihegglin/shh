<p align="left">
  <img src="docs/title.svg" alt="shh" width="260">
</p>

> A brief `shh` 🏎️💨 sound passing you by before you enter `ssh` 🏁.

**shh** is a fast, terminal-native TUI for managing SSH connections.  
Browse, filter, connect — just pure speed between you and your servers.

![demo](docs/shh.gif)

---
## Install

```sh
git clone <repo> shh
cd shh
cargo install --path .
```

Or download a binary from the release page.

## Keys

| key            | action                          |
| -------------- | ------------------------------- |
| `↑` `↓`        | navigate                        |
| `enter` / `→`  | connect, or fold/expand a group |
| `←`            | fold/unfold                     |
| *type*         | filter the list                 |
| `backspace`    | delete from filter              |
| `esc`          | clear filter, then quit         |
| `ctrl-a`       | add new host                    |
| `ctrl-e`       | edit selected host              |
| `ctrl-d`       | delete selected host            |
| `ctrl-r`       | rename group                    |
| `ctrl-c`       | quit                            |

Inside the add/edit wizard: `↑↓` choose, `←` previous step, `enter` next, `esc` cancel.

## Config

`~/.config/shh/config.toml`:

```toml
[defaults]
user = "dani"
key  = "~/.ssh/id_ed25519"
port = 22

[[group]]
name = "Production"
icon = "🔥"

[[group.server]]
name = "web-01"
host = "web1.example.com"
user = "deploy"
flags = "-A"
tags = ["frontend"]

[[group.server]]
name = "db-01"
host = "10.0.1.42"
key   = "~/.ssh/db_key"
flags = "-J jump.example.com -o ServerAliveInterval=60"
description = "primary postgres"
```

Resolution order for `user` / `key` / `port`: **server → group → defaults**. Most-specific wins.

## SSH flags per host

Every server entry can carry an optional `flags` string. It's split on whitespace and passed to `ssh` as separate args (no shell, so no metacharacter risk).

> ⚠ **The `flags` field can execute local code.** SSH options like `-o ProxyCommand=…` and `-o LocalCommand=…` run shell commands when you connect. Treat configs you didn't author yourself as untrusted code — the same way you'd treat a `~/.ssh/config` someone handed you.

| flag                          | use                       |
| ----------------------------- | ------------------------- |
| `-A`                          | forward auth agent        |
| `-C`                          | compression               |
| `-J jump.example.com`         | jump through a host       |
| `-L 8080:localhost:8080`      | local port forward        |
| `-R 9000:localhost:9000`      | remote port forward       |
| `-o ServerAliveInterval=60`   | keepalive                 |
| `-o StrictHostKeyChecking=no` | trust on first connect    |
| `-q` / `-v`                   | quiet / verbose           |
| `-t`                          | force pseudo-tty          |

Set them in the wizard (`ctrl-a` → walk to the **flags** step), via `ctrl-e` on a selected host, or directly in TOML:

```toml
flags = "-A -L 8080:localhost:8080"
```

> Flags with quoted internal spaces (e.g. `-o "ProxyCommand=ssh foo nc %h %p"`) need direct TOML editing — the in-app input splits naively on whitespace.

The exact command that will run is shown live in the details pane:

```
$ ssh deploy@web1.example.com -i ~/.ssh/id_ed25519 -A
```

## License

MIT
