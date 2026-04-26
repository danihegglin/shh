use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap,
    },
};

use crate::{
    app::{App, FlashKind, RenameGroup, Row, Wizard, WizardMode, WizardStep},
    ssh, theme,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let search_h = if !app.query.is_empty() { 3 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(search_h),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    draw_header(f, chunks[0], app);
    if search_h > 0 {
        draw_search(f, chunks[1], app);
    }
    draw_body(f, chunks[2], app);
    draw_footer(f, chunks[3], app);

    if let Some(w) = app.wizard.as_ref() {
        draw_wizard(f, area, app, w);
    } else if let Some((gi, si)) = app.delete_confirm {
        draw_delete_confirm(f, area, app, gi, si);
    } else if let Some(r) = app.rename_group.as_ref() {
        draw_rename_group(f, area, app, r);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let total_servers: usize = app.config.groups.iter().map(|g| g.servers.len()).sum();
    let total_groups = app.config.groups.len();

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("▍", Style::new().fg(theme::PRIMARY)),
            Span::styled("▍", Style::new().fg(theme::ACCENT)),
            Span::styled("▍", Style::new().fg(theme::SECONDARY)),
            Span::raw("  "),
            Span::styled(
                "S H H",
                Style::new()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("∙", Style::new().fg(theme::DIM)),
            Span::raw("  "),
            Span::styled("ssh connection manager", Style::new().fg(theme::TEXT).italic()),
            Span::raw("    "),
            Span::styled(format!("{} hosts", total_servers), Style::new().fg(theme::SECONDARY)),
            Span::styled("  ·  ", Style::new().fg(theme::DIM)),
            Span::styled(format!("{} groups", total_groups), Style::new().fg(theme::ACCENT)),
        ]),
        Line::from(""),
    ];

    let p = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::new().fg(theme::DIM))
            .border_type(BorderType::Plain),
    );
    f.render_widget(p, area);
}

fn draw_search(f: &mut Frame, area: Rect, app: &App) {
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled(app.query.clone(), Style::new().fg(theme::TEXT).bold()),
        Span::styled("▌", Style::new().fg(theme::PRIMARY)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::PRIMARY))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "search",
                Style::new()
                    .fg(theme::PRIMARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]));

    f.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_body(f: &mut Frame, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    draw_tree(f, chunks[0], app);
    draw_details(f, chunks[1], app);
}

fn draw_tree(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = app.visible_rows();
    let filtering = !app.query.is_empty();

    let items: Vec<ListItem> = rows
        .iter()
        .map(|r| match *r {
            Row::Group(gi) => {
                let g = &app.config.groups[gi];
                let expanded = app.expanded.get(gi).copied().unwrap_or(true) || filtering;
                let arrow = if expanded { "▾" } else { "▸" };
                let icon = g.icon.as_deref().unwrap_or("●");
                let count = g.servers.len();
                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", arrow), Style::new().fg(theme::ACCENT)),
                    Span::styled(icon.to_string(), Style::new().fg(theme::PRIMARY)),
                    Span::raw(" "),
                    Span::styled(g.name.clone(), Style::new().fg(theme::TEXT).bold()),
                    Span::styled(format!("  {}", count), Style::new().fg(theme::DIM)),
                ]))
            }
            Row::Server(gi, si) => {
                let s = &app.config.groups[gi].servers[si];
                ListItem::new(Line::from(vec![
                    Span::raw("    "),
                    Span::styled("◆ ", Style::new().fg(theme::SECONDARY)),
                    Span::styled(s.name.clone(), Style::new().fg(theme::TEXT)),
                    Span::raw("  "),
                    Span::styled(s.host.clone(), Style::new().fg(theme::MUTED).italic()),
                ]))
            }
        })
        .collect();

    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(app.selected.min(rows.len().saturating_sub(1))));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(theme::DIM))
                .title(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("⌁ ", Style::new().fg(theme::PRIMARY)),
                    Span::styled(
                        "HOSTS",
                        Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                ])),
        )
        .highlight_style(
            Style::new()
                .bg(theme::SELECT_BG)
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▍");

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_details(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::DIM))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled("⌬ ", Style::new().fg(theme::SECONDARY)),
            Span::styled(
                "DETAILS",
                Style::new()
                    .fg(theme::SECONDARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .padding(Padding::new(2, 2, 1, 1));

    match app.current_server() {
        Some((gi, _si, server)) => {
            let group = &app.config.groups[gi];
            let user = server.resolved_user(group, &app.config.defaults);
            let key = server.resolved_key(group, &app.config.defaults);
            let port = server.resolved_port(&app.config.defaults);

            let mut lines = vec![
                Line::from(vec![
                    Span::styled("◆ ", Style::new().fg(theme::PRIMARY)),
                    Span::styled(
                        server.name.clone(),
                        Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("  in ", Style::new().fg(theme::DIM)),
                    Span::styled(group.name.clone(), Style::new().fg(theme::ACCENT)),
                ]),
                Line::from(""),
                detail_row("host", &server.host, theme::SECONDARY),
                detail_row("user", user.as_deref().unwrap_or("(none)"), theme::PRIMARY),
                detail_row("port", &port.to_string(), theme::TEXT),
                detail_row("key", key.as_deref().unwrap_or("(none)"), theme::WARN),
            ];

            if let Some(flags) = server.flags.as_deref().filter(|s| !s.is_empty()) {
                lines.push(detail_row("flags", flags, theme::ACCENT));
            }

            if !server.tags.is_empty() {
                lines.push(Line::from(""));
                let mut tag_spans: Vec<Span> = vec![Span::styled(
                    "  tags  ",
                    Style::new().fg(theme::DIM),
                )];
                for t in &server.tags {
                    tag_spans.push(Span::styled(
                        format!(" {} ", t),
                        Style::new().fg(theme::TEXT).bg(theme::TAG_BG),
                    ));
                    tag_spans.push(Span::raw(" "));
                }
                lines.push(Line::from(tag_spans));
            }

            if let Some(desc) = &server.description {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("  {}", desc),
                    Style::new().fg(theme::MUTED).italic(),
                )));
            }

            lines.push(Line::from(""));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "  ┌─ command ─────────────────",
                Style::new().fg(theme::DIM),
            )]));
            lines.push(Line::from(vec![
                Span::styled("  │ ", Style::new().fg(theme::DIM)),
                Span::styled("$ ", Style::new().fg(theme::ACCENT)),
                Span::styled(
                    ssh::command_preview(group, server, &app.config.defaults),
                    Style::new().fg(theme::MUTED),
                ),
            ]));
            lines.push(Line::from(vec![Span::styled(
                "  └────────────────────────────",
                Style::new().fg(theme::DIM),
            )]));
            lines.push(Line::from(""));
            let chip_bg = Style::new().bg(theme::SECONDARY);
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "  CONNECT",
                    chip_bg.fg(Color::Black).add_modifier(Modifier::BOLD),
                ),
                Span::styled("  ⏎  ", chip_bg.fg(theme::DIM)),
            ]));

            f.render_widget(
                Paragraph::new(lines).block(block).wrap(Wrap { trim: false }),
                area,
            );
        }
        None => {
            let msg = if app.config.groups.is_empty() {
                "no groups defined — edit your config.toml"
            } else if app.visible_rows().is_empty() {
                "no matches"
            } else {
                "select a host to see details"
            };
            let lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    format!("  {}", msg),
                    Style::new().fg(theme::MUTED).italic(),
                )),
            ];
            f.render_widget(Paragraph::new(lines).block(block), area);
        }
    }
}

fn detail_row(label: &str, value: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<6}", label), Style::new().fg(theme::DIM)),
        Span::styled(value.to_string(), Style::new().fg(color)),
    ])
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    if let Some(flash) = &app.flash {
        let color = match flash.kind {
            FlashKind::Ok => theme::SUCCESS,
            FlashKind::Err => theme::WARN,
        };
        let line = Line::from(vec![
            Span::raw("  "),
            Span::styled(
                flash.message.clone(),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let hints: Vec<(&str, &str)> = {
        let mut h = vec![("↑↓", "nav"), ("⏎", "connect / toggle"), ("←", "fold")];
        if !app.query.is_empty() {
            h.push(("⌫", "delete"));
            h.push(("esc", "clear"));
        } else {
            h.push(("type", "to filter"));
            h.push(("esc", "quit"));
        }
        h.push(("ctrl-a", "add"));
        if app.current_server().is_some() {
            h.push(("ctrl-e", "edit"));
            h.push(("ctrl-d", "delete"));
        }
        if !app.config.groups.is_empty() {
            h.push(("ctrl-r", "rename group"));
        }
        h
    };
    let mut spans: Vec<Span> = vec![Span::raw("  ")];
    let last = hints.len().saturating_sub(1);
    for (i, (k, v)) in hints.iter().enumerate() {
        spans.push(Span::styled(
            (*k).to_string(),
            Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*v).to_string(), Style::new().fg(theme::MUTED)));
        if i != last {
            spans.push(Span::styled("  ·  ", Style::new().fg(theme::DIM)));
        }
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        area,
    );
}

fn draw_rename_group(f: &mut Frame, area: Rect, app: &App, r: &RenameGroup) {
    let modal = centered_rect(54, 12, area);
    f.render_widget(Clear, modal);

    let current = app
        .config
        .groups
        .get(r.gi)
        .map(|g| g.name.clone())
        .unwrap_or_default();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(theme::PRIMARY))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled("✎ ", Style::new().fg(theme::PRIMARY)),
            Span::styled(
                "RENAME GROUP",
                Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .padding(Padding::new(2, 2, 1, 1));

    let mut lines = vec![
        Line::from(vec![
            Span::styled("currently  ", Style::new().fg(theme::DIM)),
            Span::styled(current, Style::new().fg(theme::ACCENT)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  > ", Style::new().fg(theme::ACCENT).bold()),
            Span::styled(
                r.input.clone(),
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▌", Style::new().fg(theme::PRIMARY)),
        ]),
    ];

    if let Some(err) = &r.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ! {}", err),
            Style::new().fg(theme::WARN),
        )));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(
            "⏎",
            Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("save", Style::new().fg(theme::MUTED)),
        Span::styled("    ·    ", Style::new().fg(theme::DIM)),
        Span::styled(
            "esc",
            Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("cancel", Style::new().fg(theme::MUTED)),
    ]));

    f.render_widget(Paragraph::new(lines).block(block), modal);
}

fn draw_delete_confirm(f: &mut Frame, area: Rect, app: &App, gi: usize, si: usize) {
    let modal = centered_rect(54, 13, area);
    f.render_widget(Clear, modal);

    let server = &app.config.groups[gi].servers[si];
    let group = &app.config.groups[gi];

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(theme::PRIMARY))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled("⚠ ", Style::new().fg(theme::WARN)),
            Span::styled(
                "DELETE HOST",
                Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .padding(Padding::new(2, 2, 1, 1));

    let lines = vec![
        Line::from(vec![
            Span::styled("◆ ", Style::new().fg(theme::PRIMARY)),
            Span::styled(
                server.name.clone(),
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(server.host.clone(), Style::new().fg(theme::MUTED).italic()),
        ]),
        Line::from(vec![
            Span::styled("  in ", Style::new().fg(theme::DIM)),
            Span::styled(group.name.clone(), Style::new().fg(theme::ACCENT)),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "This cannot be undone.",
            Style::new().fg(theme::WARN),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "y / ⏎",
                Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("delete", Style::new().fg(theme::MUTED)),
            Span::styled("    ·    ", Style::new().fg(theme::DIM)),
            Span::styled(
                "n / esc",
                Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("cancel", Style::new().fg(theme::MUTED)),
        ]),
    ];

    f.render_widget(Paragraph::new(lines).block(block), modal);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let w = width.min(area.width.saturating_sub(2));
    let h = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

fn draw_wizard(f: &mut Frame, area: Rect, app: &App, w: &Wizard) {
    let modal = centered_rect(70, 24, area);
    f.render_widget(Clear, modal);

    let title_text = match w.mode {
        WizardMode::New => "NEW HOST",
        WizardMode::Edit { .. } => "EDIT HOST",
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::new().fg(theme::PRIMARY))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled("✦ ", Style::new().fg(theme::PRIMARY)),
            Span::styled(
                title_text,
                Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(step_label(w), Style::new().fg(theme::MUTED).italic()),
            Span::raw(" "),
        ]))
        .padding(Padding::new(2, 2, 1, 1));

    let inner = block.inner(modal);
    f.render_widget(block, modal);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0),
            Constraint::Length(2),
        ])
        .split(inner);

    draw_wizard_progress(f, chunks[0], w);
    draw_wizard_step(f, chunks[1], app, w);
    draw_wizard_hints(f, chunks[2], w);
}

fn step_label(w: &Wizard) -> &'static str {
    match w.step {
        WizardStep::PickGroup => "group",
        WizardStep::NewGroupName => "new group",
        WizardStep::ServerName => "name",
        WizardStep::Host => "host",
        WizardStep::User => "user",
        WizardStep::Key => "key",
        WizardStep::Port => "port",
        WizardStep::Flags => "flags",
        WizardStep::Tags => "tags",
        WizardStep::Description => "description",
        WizardStep::Confirm => "confirm",
    }
}

fn step_progress(w: &Wizard) -> (usize, usize) {
    let (idx, total) = if w.group_idx.is_none()
        && !matches!(w.step, WizardStep::PickGroup)
    {
        let i = match w.step {
            WizardStep::NewGroupName => 2,
            WizardStep::ServerName => 3,
            WizardStep::Host => 4,
            WizardStep::User => 5,
            WizardStep::Key => 6,
            WizardStep::Port => 7,
            WizardStep::Flags => 8,
            WizardStep::Tags => 9,
            WizardStep::Description => 10,
            WizardStep::Confirm => 11,
            _ => 1,
        };
        (i, 11)
    } else {
        let i = match w.step {
            WizardStep::PickGroup => 1,
            WizardStep::ServerName => 2,
            WizardStep::Host => 3,
            WizardStep::User => 4,
            WizardStep::Key => 5,
            WizardStep::Port => 6,
            WizardStep::Flags => 7,
            WizardStep::Tags => 8,
            WizardStep::Description => 9,
            WizardStep::Confirm => 10,
            _ => 1,
        };
        (i, 10)
    };
    (idx, total)
}

fn draw_wizard_progress(f: &mut Frame, area: Rect, w: &Wizard) {
    let (idx, total) = step_progress(w);
    let mut spans: Vec<Span> = vec![];
    for i in 1..=total {
        let filled = i <= idx;
        spans.push(Span::styled(
            if filled { "●" } else { "○" },
            Style::new().fg(if filled { theme::PRIMARY } else { theme::DIM }),
        ));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::raw("  "));
    spans.push(Span::styled(
        format!("{} / {}", idx, total),
        Style::new().fg(theme::DIM),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_wizard_step(f: &mut Frame, area: Rect, app: &App, w: &Wizard) {
    match w.step {
        WizardStep::PickGroup => draw_group_picker(f, area, app, w),
        WizardStep::NewGroupName => draw_text_step(f, area, w, "New group name", "", true),
        WizardStep::ServerName => draw_text_step(f, area, w, "Server name", "web-01", true),
        WizardStep::Host => draw_text_step(f, area, w, "Host", "host.example.com", true),
        WizardStep::User => draw_text_step(
            f,
            area,
            w,
            "User",
            app.config.defaults.user.as_deref().unwrap_or(""),
            false,
        ),
        WizardStep::Key => draw_key_picker(f, area, w),
        WizardStep::Port => draw_text_step(f, area, w, "Port", "22", false),
        WizardStep::Flags => draw_text_step(f, area, w, "Extra SSH flags", "", false),
        WizardStep::Tags => draw_text_step(f, area, w, "Tags  (comma-separated)", "", false),
        WizardStep::Description => draw_text_step(f, area, w, "Description", "", false),
        WizardStep::Confirm => draw_confirm(f, area, app, w),
    }
}

fn draw_text_step(
    f: &mut Frame,
    area: Rect,
    w: &Wizard,
    label: &str,
    placeholder: &str,
    required: bool,
) {
    let suffix = if required {
        Span::styled("  required", Style::new().fg(theme::WARN).italic())
    } else if !placeholder.is_empty() {
        Span::styled(
            format!("  default: {}", placeholder),
            Style::new().fg(theme::DIM).italic(),
        )
    } else {
        Span::styled(
            "  (optional — press ⏎ to skip)",
            Style::new().fg(theme::DIM).italic(),
        )
    };

    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                label.to_string(),
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
            suffix,
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  > ", Style::new().fg(theme::ACCENT).bold()),
            Span::styled(
                w.input.clone(),
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled("▌", Style::new().fg(theme::PRIMARY)),
        ]),
    ];

    if let Some(err) = &w.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ! {}", err),
            Style::new().fg(theme::WARN),
        )));
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn draw_key_picker(f: &mut Frame, area: Rect, w: &Wizard) {
    let hint = if w.key_options.len() > 1 {
        "from ~/.ssh/  (↑↓ to choose)"
    } else {
        "no private keys found in ~/.ssh/"
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled(
                "Identity file",
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(hint, Style::new().fg(theme::DIM).italic()),
        ]),
        Line::from(""),
    ];

    for (i, opt) in w.key_options.iter().enumerate() {
        let selected = w.key_pick == i;
        let arrow = if selected { "▍ " } else { "  " };
        let arrow_style = Style::new().fg(if selected { theme::PRIMARY } else { theme::DIM });
        let (label, base_color) = if opt.path.is_empty() {
            ("(none — fall through to default)".to_string(), theme::MUTED)
        } else {
            (opt.path.clone(), theme::WARN)
        };
        let style = if selected {
            Style::new()
                .fg(theme::PRIMARY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(base_color)
        };
        lines.push(Line::from(vec![
            Span::styled(arrow, arrow_style),
            Span::styled(label, style),
        ]));

        if selected && !opt.path.is_empty() {
            let sub = match &opt.info {
                Some(info) => format!("    {}  ·  {}", info.algorithm, info.fingerprint),
                None => "    (no .pub file — fingerprint unavailable)".to_string(),
            };
            lines.push(Line::from(Span::styled(
                sub,
                Style::new().fg(theme::MUTED).italic(),
            )));
        }
    }

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn draw_group_picker(f: &mut Frame, area: Rect, app: &App, w: &Wizard) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Pick a group",
            Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for (i, g) in app.config.groups.iter().enumerate() {
        let selected = w.group_pick == i;
        let icon = g.icon.as_deref().unwrap_or("●");
        let arrow = if selected { "▍ " } else { "  " };
        let arrow_style = Style::new().fg(if selected { theme::PRIMARY } else { theme::DIM });
        let name_style = if selected {
            Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(theme::TEXT)
        };
        lines.push(Line::from(vec![
            Span::styled(arrow, arrow_style),
            Span::styled(format!("{} ", icon), Style::new().fg(theme::ACCENT)),
            Span::styled(g.name.clone(), name_style),
            Span::styled(
                format!("   {} hosts", g.servers.len()),
                Style::new().fg(theme::DIM),
            ),
        ]));
    }

    let i_new = app.config.groups.len();
    let selected = w.group_pick == i_new;
    let arrow = if selected { "▍ " } else { "  " };
    let arrow_style = Style::new().fg(if selected { theme::PRIMARY } else { theme::DIM });
    let style = if selected {
        Style::new()
            .fg(theme::SUCCESS)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme::SUCCESS)
    };
    if !app.config.groups.is_empty() {
        lines.push(Line::from(""));
    }
    lines.push(Line::from(vec![
        Span::styled(arrow, arrow_style),
        Span::styled("[ + new group ]", style),
    ]));

    f.render_widget(Paragraph::new(lines), area);
}

fn draw_confirm(f: &mut Frame, area: Rect, app: &App, w: &Wizard) {
    let group_name = match w.group_idx {
        Some(i) => app.config.groups[i].name.clone(),
        None => format!("{}  (new)", w.new_group_name),
    };
    let port_display = if w.draft.port.is_empty() {
        "22  (default)".to_string()
    } else {
        w.draft.port.clone()
    };
    let user_display = if w.draft.user.is_empty() {
        app.config
            .defaults
            .user
            .clone()
            .map(|u| format!("{}  (default)", u))
            .unwrap_or_else(|| "—".into())
    } else {
        w.draft.user.clone()
    };
    let key_display = if w.draft.key.is_empty() {
        app.config
            .defaults
            .key
            .clone()
            .map(|k| format!("{}  (default)", k))
            .unwrap_or_else(|| "—".into())
    } else {
        w.draft.key.clone()
    };
    let flags_display = if w.draft.flags.is_empty() {
        "—".into()
    } else {
        w.draft.flags.clone()
    };
    let tags_display = if w.draft.tags.is_empty() {
        "—".into()
    } else {
        w.draft.tags.clone()
    };
    let desc_display = if w.draft.description.is_empty() {
        "—".into()
    } else {
        w.draft.description.clone()
    };

    let verb = match w.mode {
        WizardMode::New => "ready to add",
        WizardMode::Edit { .. } => "ready to save",
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("✓ ", Style::new().fg(theme::SUCCESS).bold()),
            Span::styled(
                verb,
                Style::new().fg(theme::TEXT).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        confirm_row("group", &group_name, theme::ACCENT),
        confirm_row("name", &w.draft.name, theme::PRIMARY),
        confirm_row("host", &w.draft.host, theme::SECONDARY),
        confirm_row("user", &user_display, theme::TEXT),
        confirm_row("key", &key_display, theme::WARN),
        confirm_row("port", &port_display, theme::TEXT),
        confirm_row("flags", &flags_display, theme::ACCENT),
        confirm_row("tags", &tags_display, theme::TEXT),
        confirm_row("desc", &desc_display, theme::MUTED),
    ];
    if let Some(err) = &w.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ! {}", err),
            Style::new().fg(theme::WARN),
        )));
    }
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

fn confirm_row(label: &str, value: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("  {:<7}", label), Style::new().fg(theme::DIM)),
        Span::styled(value.to_string(), Style::new().fg(color)),
    ])
}

fn draw_wizard_hints(f: &mut Frame, area: Rect, w: &Wizard) {
    let hints: Vec<(&str, &str)> = match w.step {
        WizardStep::PickGroup => vec![("↑↓", "choose"), ("⏎", "select"), ("esc", "cancel")],
        WizardStep::Key => vec![
            ("↑↓", "choose"),
            ("⏎", "select"),
            ("←", "back"),
            ("esc", "cancel"),
        ],
        WizardStep::Confirm => vec![("⏎", "save"), ("↑", "back"), ("esc", "cancel")],
        _ => vec![("⏎", "next"), ("↑", "back"), ("esc", "cancel")],
    };
    let mut spans: Vec<Span> = vec![];
    let last = hints.len().saturating_sub(1);
    for (i, (k, v)) in hints.iter().enumerate() {
        spans.push(Span::styled(
            (*k).to_string(),
            Style::new().fg(theme::PRIMARY).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*v).to_string(), Style::new().fg(theme::MUTED)));
        if i != last {
            spans.push(Span::styled("  ·  ", Style::new().fg(theme::DIM)));
        }
    }
    let p = Paragraph::new(Line::from(spans)).block(
        Block::default()
            .borders(Borders::TOP)
            .border_style(Style::new().fg(theme::DIM)),
    );
    f.render_widget(p, area);
}
