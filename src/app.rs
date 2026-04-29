use anyhow::Result;

use crate::config::{Config, Group, Server};
use crate::ssh::{self, KeyInfo};

#[derive(Debug, Clone)]
pub struct KeyOption {
    pub path: String,
    pub info: Option<KeyInfo>,
}

#[derive(Debug, Clone, Copy)]
pub enum Row {
    Group(usize),
    Server(usize, usize),
}

pub enum Action {
    Quit,
    Connect(usize, usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardStep {
    PickGroup,
    NewGroupName,
    ServerName,
    Host,
    User,
    Key,
    Port,
    Flags,
    Tags,
    Description,
    Confirm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WizardMode {
    New,
    Edit { gi: usize, si: usize },
}

#[derive(Default, Debug, Clone)]
pub struct ServerDraft {
    pub name: String,
    pub host: String,
    pub user: String,
    pub key: String,
    pub port: String,
    pub flags: String,
    pub tags: String,
    pub description: String,
}

pub struct Wizard {
    pub mode: WizardMode,
    pub step: WizardStep,
    pub group_idx: Option<usize>,
    pub group_pick: usize,
    pub new_group_name: String,
    pub draft: ServerDraft,
    pub input: String,
    pub key_options: Vec<KeyOption>,
    pub key_pick: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashKind {
    Ok,
    Err,
}

#[derive(Debug, Clone)]
pub struct Flash {
    pub kind: FlashKind,
    pub message: String,
}

impl Flash {
    pub fn ok(s: impl Into<String>) -> Self {
        Self {
            kind: FlashKind::Ok,
            message: s.into(),
        }
    }
    pub fn err(s: impl Into<String>) -> Self {
        Self {
            kind: FlashKind::Err,
            message: s.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenameGroup {
    pub gi: usize,
    pub input: String,
    pub error: Option<String>,
}

pub struct App {
    pub config: Config,
    pub expanded: Vec<bool>,
    pub selected: usize,
    pub query: String,
    pub wizard: Option<Wizard>,
    pub delete_confirm: Option<(usize, usize)>,
    pub rename_group: Option<RenameGroup>,
    pub flash: Option<Flash>,
}

impl App {
    pub fn new(config: Config) -> Self {
        let n = config.groups.len();
        Self {
            config,
            expanded: vec![true; n],
            selected: 0,
            query: String::new(),
            wizard: None,
            delete_confirm: None,
            rename_group: None,
            flash: None,
        }
    }

    pub fn visible_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let q = self.query.to_lowercase();
        let filter = !q.is_empty();
        for (gi, group) in self.config.groups.iter().enumerate() {
            let group_match = filter && group.name.to_lowercase().contains(&q);
            let matching: Vec<usize> = group
                .servers
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    !filter
                        || group_match
                        || s.name.to_lowercase().contains(&q)
                        || s.host.to_lowercase().contains(&q)
                        || s.tags.iter().any(|t| t.to_lowercase().contains(&q))
                })
                .map(|(i, _)| i)
                .collect();

            if filter && matching.is_empty() && !group_match {
                continue;
            }
            rows.push(Row::Group(gi));
            let expanded = self.expanded.get(gi).copied().unwrap_or(true) || filter;
            if expanded {
                for si in matching {
                    rows.push(Row::Server(gi, si));
                }
            }
        }
        rows
    }

    pub fn move_down(&mut self) {
        let n = self.visible_rows().len();
        if n == 0 {
            return;
        }
        self.selected = (self.selected + 1) % n;
    }

    pub fn move_up(&mut self) {
        let n = self.visible_rows().len();
        if n == 0 {
            return;
        }
        self.selected = if self.selected == 0 {
            n - 1
        } else {
            self.selected - 1
        };
    }

    pub fn toggle_current(&mut self) {
        let rows = self.visible_rows();
        if let Some(Row::Group(gi)) = rows.get(self.selected).copied() {
            self.expanded[gi] = !self.expanded[gi];
            self.clamp_selection();
        }
    }

    pub fn enter(&mut self) -> Option<Action> {
        let rows = self.visible_rows();
        match rows.get(self.selected).copied()? {
            Row::Group(gi) => {
                self.expanded[gi] = !self.expanded[gi];
                self.clamp_selection();
                None
            }
            Row::Server(gi, si) => Some(Action::Connect(gi, si)),
        }
    }

    pub fn clamp_selection(&mut self) {
        let n = self.visible_rows().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    pub fn current_server(&self) -> Option<(usize, usize, &Server)> {
        let rows = self.visible_rows();
        match rows.get(self.selected).copied()? {
            Row::Server(gi, si) => Some((gi, si, &self.config.groups[gi].servers[si])),
            _ => None,
        }
    }

    pub fn start_wizard(&mut self) {
        let preselect = self
            .current_server()
            .map(|(gi, _, _)| gi)
            .or_else(|| {
                self.visible_rows().get(self.selected).and_then(|r| match r {
                    Row::Group(gi) => Some(*gi),
                    _ => None,
                })
            })
            .unwrap_or(0);
        let key_options = build_key_options("");
        self.wizard = Some(Wizard {
            mode: WizardMode::New,
            step: WizardStep::PickGroup,
            group_idx: None,
            group_pick: preselect.min(self.config.groups.len()),
            new_group_name: String::new(),
            draft: ServerDraft::default(),
            input: String::new(),
            key_options,
            key_pick: 0,
            error: None,
        });
    }

    pub fn start_wizard_edit(&mut self) {
        let Some((gi, si, server)) = self.current_server() else {
            return;
        };
        let existing_key = server.key.clone().unwrap_or_default();
        let key_options = build_key_options(&existing_key);
        let key_pick = key_options
            .iter()
            .position(|k| k.path == existing_key)
            .unwrap_or(0);
        let draft = ServerDraft {
            name: server.name.clone(),
            host: server.host.clone(),
            user: server.user.clone().unwrap_or_default(),
            key: existing_key,
            port: server.port.map(|p| p.to_string()).unwrap_or_default(),
            flags: server.flags.clone().unwrap_or_default(),
            tags: server.tags.join(", "),
            description: server.description.clone().unwrap_or_default(),
        };
        self.wizard = Some(Wizard {
            mode: WizardMode::Edit { gi, si },
            step: WizardStep::PickGroup,
            group_idx: None,
            group_pick: gi,
            new_group_name: String::new(),
            draft,
            input: String::new(),
            key_options,
            key_pick,
            error: None,
        });
    }

    pub fn cancel_wizard(&mut self) {
        self.wizard = None;
    }

    pub fn start_delete_confirm(&mut self) {
        if let Some((gi, si, _)) = self.current_server() {
            self.delete_confirm = Some((gi, si));
        }
    }

    pub fn cancel_delete(&mut self) {
        self.delete_confirm = None;
    }

    pub fn start_rename_group(&mut self) {
        let rows = self.visible_rows();
        let Some(row) = rows.get(self.selected).copied() else {
            return;
        };
        let gi = match row {
            Row::Group(gi) => gi,
            Row::Server(gi, _) => gi,
        };
        if gi >= self.config.groups.len() {
            return;
        }
        let name = self.config.groups[gi].name.clone();
        self.rename_group = Some(RenameGroup {
            gi,
            input: name,
            error: None,
        });
    }

    pub fn cancel_rename_group(&mut self) {
        self.rename_group = None;
    }

    pub fn rename_input(&mut self, c: char) {
        if let Some(r) = self.rename_group.as_mut() {
            if !c.is_control() {
                r.input.push(c);
                r.error = None;
            }
        }
    }

    pub fn rename_backspace(&mut self) {
        if let Some(r) = self.rename_group.as_mut() {
            r.input.pop();
            r.error = None;
        }
    }

    pub fn commit_rename_group(&mut self) -> Result<()> {
        let Some(mut r) = self.rename_group.take() else {
            return Ok(());
        };
        let new_name = r.input.trim().to_string();

        if new_name.is_empty() {
            r.error = Some("name cannot be empty".into());
            self.rename_group = Some(r);
            return Ok(());
        }
        if r.gi >= self.config.groups.len() {
            return Ok(());
        }
        let same_as_current = new_name == self.config.groups[r.gi].name;
        if !same_as_current
            && self
                .config
                .groups
                .iter()
                .enumerate()
                .any(|(i, g)| i != r.gi && g.name == new_name)
        {
            r.error = Some("a group with that name already exists".into());
            self.rename_group = Some(r);
            return Ok(());
        }
        if same_as_current {
            return Ok(());
        }

        let mut new_config = self.config.clone();
        let old_name = new_config.groups[r.gi].name.clone();
        new_config.groups[r.gi].name = new_name.clone();

        if let Err(e) = new_config.save() {
            r.error = Some(format!("save failed: {}", e));
            self.rename_group = Some(r);
            return Ok(());
        }

        self.config = new_config;
        self.flash = Some(Flash::ok(format!("✓ renamed {} → {}", old_name, new_name)));
        Ok(())
    }

    pub fn commit_delete(&mut self) -> Result<()> {
        let Some((gi, si)) = self.delete_confirm.take() else {
            return Ok(());
        };
        if gi >= self.config.groups.len() || si >= self.config.groups[gi].servers.len() {
            return Ok(());
        }
        let mut new_config = self.config.clone();
        let removed = new_config.groups[gi].servers.remove(si);
        prune_empty_groups(&mut new_config);
        if let Err(e) = new_config.save() {
            self.flash = Some(Flash::err(format!("✗ delete failed: {}", e)));
            return Ok(());
        }
        let new_expanded =
            reconcile_expanded(&self.config.groups, &self.expanded, &new_config.groups);
        self.config = new_config;
        self.expanded = new_expanded;
        self.clamp_selection();
        self.flash = Some(Flash::ok(format!("✓ deleted {}", removed.name)));
        Ok(())
    }

    pub fn wizard_input(&mut self, c: char) {
        if let Some(w) = self.wizard.as_mut() {
            if matches!(w.step, WizardStep::PickGroup | WizardStep::Key) {
                return;
            }
            if !c.is_control() {
                w.input.push(c);
                w.error = None;
            }
        }
    }

    pub fn wizard_backspace(&mut self) {
        if let Some(w) = self.wizard.as_mut() {
            if matches!(
                w.step,
                WizardStep::PickGroup | WizardStep::Key | WizardStep::Confirm
            ) {
                return;
            }
            w.input.pop();
            w.error = None;
        }
    }

    pub fn wizard_pick_down(&mut self) {
        let group_max = self.config.groups.len();
        if let Some(w) = self.wizard.as_mut() {
            match w.step {
                WizardStep::PickGroup => {
                    if w.group_pick < group_max {
                        w.group_pick += 1;
                    }
                }
                WizardStep::Key => {
                    if w.key_pick + 1 < w.key_options.len() {
                        w.key_pick += 1;
                    }
                }
                _ => {}
            }
        }
    }

    pub fn wizard_up(&mut self) {
        if let Some(w) = self.wizard.as_mut() {
            match w.step {
                WizardStep::PickGroup => {
                    if w.group_pick > 0 {
                        w.group_pick -= 1;
                    }
                    return;
                }
                WizardStep::Key => {
                    if w.key_pick > 0 {
                        w.key_pick -= 1;
                    }
                    return;
                }
                _ => {}
            }
        }
        self.wizard_back();
    }

    pub fn wizard_back(&mut self) {
        if let Some(w) = self.wizard.as_mut() {
            let new_step = match w.step {
                WizardStep::PickGroup => return,
                WizardStep::NewGroupName => WizardStep::PickGroup,
                WizardStep::ServerName => {
                    if w.group_idx.is_none() {
                        WizardStep::NewGroupName
                    } else {
                        WizardStep::PickGroup
                    }
                }
                WizardStep::Host => WizardStep::ServerName,
                WizardStep::User => WizardStep::Host,
                WizardStep::Key => WizardStep::User,
                WizardStep::Port => WizardStep::Key,
                WizardStep::Flags => WizardStep::Port,
                WizardStep::Tags => WizardStep::Flags,
                WizardStep::Description => WizardStep::Tags,
                WizardStep::Confirm => WizardStep::Description,
            };
            w.step = new_step;
            w.error = None;
            sync_input(w);
        }
    }

    pub fn wizard_advance(&mut self) -> Result<()> {
        let mut commit = false;
        let groups_len = self.config.groups.len();

        if let Some(w) = self.wizard.as_mut() {
            match w.step {
                WizardStep::PickGroup => {
                    if w.group_pick < groups_len {
                        w.group_idx = Some(w.group_pick);
                        w.step = WizardStep::ServerName;
                    } else {
                        w.group_idx = None;
                        w.step = WizardStep::NewGroupName;
                    }
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::NewGroupName => {
                    let v = w.input.trim().to_string();
                    if v.is_empty() {
                        w.error = Some("group name cannot be empty".into());
                        return Ok(());
                    }
                    w.new_group_name = v;
                    w.step = WizardStep::ServerName;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::ServerName => {
                    let v = w.input.trim().to_string();
                    if v.is_empty() {
                        w.error = Some("server name required".into());
                        return Ok(());
                    }
                    w.draft.name = v;
                    w.step = WizardStep::Host;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Host => {
                    let v = w.input.trim().to_string();
                    if v.is_empty() {
                        w.error = Some("host required".into());
                        return Ok(());
                    }
                    if let Some(reason) = validate_host(&v) {
                        w.error = Some(reason);
                        return Ok(());
                    }
                    w.draft.host = v;
                    w.step = WizardStep::User;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::User => {
                    w.draft.user = w.input.trim().to_string();
                    w.step = WizardStep::Key;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Key => {
                    w.draft.key = w
                        .key_options
                        .get(w.key_pick)
                        .map(|o| o.path.clone())
                        .unwrap_or_default();
                    w.step = WizardStep::Port;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Port => {
                    let v = w.input.trim().to_string();
                    if !v.is_empty() {
                        match v.parse::<u16>() {
                            Err(_) => {
                                w.error = Some(format!("invalid port: {}", v));
                                return Ok(());
                            }
                            Ok(0) => {
                                w.error = Some("port must be between 1 and 65535".into());
                                return Ok(());
                            }
                            Ok(_) => {}
                        }
                    }
                    w.draft.port = v;
                    w.step = WizardStep::Flags;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Flags => {
                    w.draft.flags = w.input.trim().to_string();
                    w.step = WizardStep::Tags;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Tags => {
                    w.draft.tags = w.input.trim().to_string();
                    w.step = WizardStep::Description;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Description => {
                    w.draft.description = w.input.trim().to_string();
                    w.step = WizardStep::Confirm;
                    w.error = None;
                    sync_input(w);
                }
                WizardStep::Confirm => {
                    commit = true;
                }
            }
        }

        if commit {
            self.commit_wizard()?;
        }
        Ok(())
    }

    fn commit_wizard(&mut self) -> Result<()> {
        let Some(mut w) = self.wizard.take() else {
            return Ok(());
        };

        let port = if w.draft.port.is_empty() {
            None
        } else {
            match w.draft.port.parse::<u16>() {
                Ok(p) => Some(p),
                Err(_) => {
                    w.error = Some(format!("invalid port: {}", w.draft.port));
                    w.step = WizardStep::Port;
                    sync_input(&mut w);
                    self.wizard = Some(w);
                    return Ok(());
                }
            }
        };

        let tags: Vec<String> = w
            .draft
            .tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let server = Server {
            name: w.draft.name.clone(),
            host: w.draft.host.clone(),
            user: opt_string(&w.draft.user),
            key: opt_string(&w.draft.key),
            port,
            flags: opt_string(&w.draft.flags),
            tags,
            description: opt_string(&w.draft.description),
        };

        // Mutate a clone so we only commit if persistence succeeds.
        let mut new_config = self.config.clone();

        let target_gi = match w.group_idx {
            Some(i) => i,
            None => {
                new_config.groups.push(Group {
                    name: w.new_group_name.clone(),
                    icon: None,
                    user: None,
                    key: None,
                    servers: Vec::new(),
                });
                new_config.groups.len() - 1
            }
        };

        let server_name = server.name.clone();
        let target_name = new_config.groups[target_gi].name.clone();
        let (new_si, action) = match w.mode {
            WizardMode::New => {
                new_config.groups[target_gi].servers.push(server);
                (new_config.groups[target_gi].servers.len() - 1, "added")
            }
            WizardMode::Edit {
                gi: orig_gi,
                si: orig_si,
            } => {
                if target_gi == orig_gi {
                    new_config.groups[target_gi].servers[orig_si] = server;
                    (orig_si, "updated")
                } else {
                    new_config.groups[orig_gi].servers.remove(orig_si);
                    new_config.groups[target_gi].servers.push(server);
                    (new_config.groups[target_gi].servers.len() - 1, "moved")
                }
            }
        };

        prune_empty_groups(&mut new_config);

        if let Err(e) = new_config.save() {
            w.error = Some(format!("save failed: {}", e));
            self.wizard = Some(w);
            return Ok(());
        }

        let new_target_gi = new_config
            .groups
            .iter()
            .position(|g| g.name == target_name);
        let new_expanded =
            reconcile_expanded(&self.config.groups, &self.expanded, &new_config.groups);
        self.config = new_config;
        self.expanded = new_expanded;
        self.query.clear();

        if let Some(gi) = new_target_gi {
            self.expanded[gi] = true;
            let rows = self.visible_rows();
            if let Some(idx) = rows
                .iter()
                .position(|r| matches!(r, Row::Server(g, s) if *g == gi && *s == new_si))
            {
                self.selected = idx;
            } else {
                self.clamp_selection();
            }
        } else {
            self.clamp_selection();
        }

        self.flash = Some(Flash::ok(format!("✓ {} {}", action, server_name)));
        Ok(())
    }
}

fn sync_input(w: &mut Wizard) {
    w.input = match w.step {
        WizardStep::NewGroupName => w.new_group_name.clone(),
        WizardStep::ServerName => w.draft.name.clone(),
        WizardStep::Host => w.draft.host.clone(),
        WizardStep::User => w.draft.user.clone(),
        WizardStep::Port => w.draft.port.clone(),
        WizardStep::Flags => w.draft.flags.clone(),
        WizardStep::Tags => w.draft.tags.clone(),
        WizardStep::Description => w.draft.description.clone(),
        _ => String::new(),
    };
    if matches!(w.step, WizardStep::Key) {
        w.key_pick = w
            .key_options
            .iter()
            .position(|k| k.path == w.draft.key)
            .unwrap_or(0);
    }
}

fn build_key_options(existing: &str) -> Vec<KeyOption> {
    let mut opts = vec![KeyOption {
        path: String::new(),
        info: None,
    }];
    let discovered = ssh::discover_ssh_keys();
    let trimmed = existing.trim();
    let in_discovered = !trimmed.is_empty() && discovered.iter().any(|k| k == trimmed);
    if !trimmed.is_empty() && !in_discovered {
        opts.push(KeyOption {
            info: ssh::key_fingerprint(trimmed),
            path: trimmed.to_string(),
        });
    }
    for path in discovered {
        let info = ssh::key_fingerprint(&path);
        opts.push(KeyOption { path, info });
    }
    opts
}

fn opt_string(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn validate_host(s: &str) -> Option<String> {
    if s.contains('@') {
        return Some("don't include user@ — set user in the next step".into());
    }
    if s.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Some("host cannot contain whitespace".into());
    }

    let ip_candidate = s
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .unwrap_or(s);
    if ip_candidate.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    if is_valid_hostname(s) {
        return None;
    }
    Some("host must be a hostname or IP address".into())
}

fn is_valid_hostname(s: &str) -> bool {
    if s.is_empty() || s.len() > 253 {
        return false;
    }
    let s = s.strip_suffix('.').unwrap_or(s);
    s.split('.').all(is_valid_label)
}

fn is_valid_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    let len = bytes.len();
    if len == 0 || len > 63 {
        return false;
    }
    if bytes[0] == b'-' || bytes[len - 1] == b'-' {
        return false;
    }
    label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

fn prune_empty_groups(config: &mut Config) {
    config.groups.retain(|g| !g.servers.is_empty());
}

fn reconcile_expanded(old_groups: &[Group], old_expanded: &[bool], new_groups: &[Group]) -> Vec<bool> {
    new_groups
        .iter()
        .map(|g| {
            old_groups
                .iter()
                .position(|og| og.name == g.name)
                .and_then(|i| old_expanded.get(i).copied())
                .unwrap_or(true)
        })
        .collect()
}
