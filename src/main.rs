mod app;
mod config;
mod ssh;
mod theme;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    io,
    process::ExitStatus,
    time::{Duration, Instant},
};

use app::{Action, App, Flash};
use config::Config;

const RED_B: &str = "\x1b[1;31m";
const MAGENTA_B: &str = "\x1b[1;35m";
const CYAN: &str = "\x1b[36m";
const YELLOW: &str = "\x1b[33m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

fn main() -> Result<()> {
    let config = Config::load_or_init()?;
    let mut app = App::new(config);

    loop {
        let mut terminal = setup_terminal()?;
        let action = run_app(&mut terminal, &mut app);
        restore_terminal(&mut terminal)?;
        match action? {
            Action::Quit => break,
            Action::Connect(gi, si) => {
                let group = app.config.groups[gi].clone();
                let server = group.servers[si].clone();
                let defaults = app.config.defaults.clone();
                let preview = ssh::command_preview(&group, &server, &defaults);

                println!();
                println!(
                    "  {MAGENTA_B}→{RESET} connecting to {MAGENTA_B}{}{RESET} ({CYAN}{}{RESET})",
                    server.name, server.host
                );
                println!();

                match ssh::connect(&group, &server, &defaults) {
                    Ok(status) if status.success() => {
                        println!();
                        println!("  {DIM}← back to shh{RESET}");
                    }
                    Ok(status) if status.code() == Some(255) => {
                        print_connection_failed(&server.host, &preview);
                        wait_for_keypress()?;
                        app.flash = Some(Flash::err(format!(
                            "✗ {} — connection failed",
                            server.name
                        )));
                    }
                    Ok(status) => {
                        print_session_ended(&server.host, &preview, status);
                        app.flash = Some(Flash::err(format!(
                            "✗ {} — exit {}",
                            server.name,
                            status
                                .code()
                                .map(|c| c.to_string())
                                .unwrap_or_else(|| "killed".into())
                        )));
                    }
                    Err(e) => {
                        print_spawn_error(&server.host, &preview, &e);
                        wait_for_keypress()?;
                        app.flash = Some(Flash::err(format!(
                            "✗ couldn't run ssh: {}",
                            short_error(&e)
                        )));
                    }
                }
            }
        }
    }
    Ok(())
}

fn setup_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    Ok(Terminal::new(CrosstermBackend::new(stdout))?)
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    Ok(())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<Action> {
    let tick = Duration::from_millis(120);
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = tick.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && matches!(key.code, KeyCode::Char('c'))
                {
                    return Ok(Action::Quit);
                }

                app.flash = None;

                if app.rename_group.is_some() {
                    match key.code {
                        KeyCode::Esc => app.cancel_rename_group(),
                        KeyCode::Enter => app.commit_rename_group()?,
                        KeyCode::Backspace => app.rename_backspace(),
                        KeyCode::Char(c) => {
                            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                                app.rename_input(c);
                            }
                        }
                        _ => {}
                    }
                } else if app.delete_confirm.is_some() {
                    match key.code {
                        KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                            app.cancel_delete()
                        }
                        KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                            app.commit_delete()?
                        }
                        _ => {}
                    }
                } else if app.wizard.is_some() {
                    match key.code {
                        KeyCode::Esc => app.cancel_wizard(),
                        KeyCode::Enter => app.wizard_advance()?,
                        KeyCode::Up => app.wizard_up(),
                        KeyCode::Down => app.wizard_pick_down(),
                        KeyCode::Left => app.wizard_back(),
                        KeyCode::Backspace => app.wizard_backspace(),
                        KeyCode::Char(c) => app.wizard_input(c),
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Esc => {
                            if app.query.is_empty() {
                                return Ok(Action::Quit);
                            }
                            app.query.clear();
                            app.clamp_selection();
                        }
                        KeyCode::Down => app.move_down(),
                        KeyCode::Up => app.move_up(),
                        KeyCode::Enter | KeyCode::Right => {
                            if let Some(action) = app.enter() {
                                return Ok(action);
                            }
                        }
                        KeyCode::Left => app.toggle_current(),
                        KeyCode::Backspace => {
                            app.query.pop();
                            app.clamp_selection();
                        }
                        KeyCode::Char(c) => {
                            if key.modifiers.contains(KeyModifiers::CONTROL) {
                                match c {
                                    'a' => app.start_wizard(),
                                    'e' => app.start_wizard_edit(),
                                    'd' => app.start_delete_confirm(),
                                    'r' => app.start_rename_group(),
                                    _ => {}
                                }
                            } else {
                                app.query.push(c);
                                app.clamp_selection();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick {
            last_tick = Instant::now();
        }
    }
}

fn print_connection_failed(host: &str, command: &str) {
    eprintln!();
    eprintln!("  {RED_B}✗ connection failed{RESET}");
    eprintln!();
    eprintln!("  {DIM}command{RESET}   {}", command);
    eprintln!("  {DIM}host{RESET}      {}", host);
    eprintln!("  {DIM}status{RESET}    exit 255  (ssh could not establish the connection)");
    eprintln!();
    eprintln!("  {DIM}common causes:{RESET}");
    for cause in [
        "host unreachable or DNS failure",
        "port closed or firewalled",
        "authentication rejected (wrong key or user)",
        "host key mismatch",
    ] {
        eprintln!("    {YELLOW}•{RESET} {}", cause);
    }
    eprintln!();
    eprintln!("  {DIM}press any key to return to shh{RESET}");
}

fn print_session_ended(host: &str, command: &str, status: ExitStatus) {
    let code = status
        .code()
        .map(|c| format!("exit {}", c))
        .unwrap_or_else(|| "killed by signal".into());
    eprintln!();
    eprintln!(
        "  {YELLOW}⚠ session ended{RESET}  {DIM}{}{RESET}  {DIM}·{RESET}  {}  {DIM}({}){RESET}",
        code, host, command
    );
}

fn print_spawn_error(host: &str, command: &str, err: &anyhow::Error) {
    eprintln!();
    eprintln!("  {RED_B}✗ couldn't start ssh{RESET}");
    eprintln!();
    eprintln!("  {DIM}command{RESET}   {}", command);
    eprintln!("  {DIM}host{RESET}      {}", host);
    eprintln!("  {DIM}error{RESET}     {}", err);
    let mut source = err.source();
    while let Some(s) = source {
        eprintln!("  {DIM}  └─{RESET}       {}", s);
        source = s.source();
    }
    eprintln!();
    eprintln!("  {DIM}is the ssh binary in your PATH?{RESET}");
    eprintln!();
    eprintln!("  {DIM}press any key to return to shh{RESET}");
}

fn short_error(err: &anyhow::Error) -> String {
    err.source()
        .map(|s| s.to_string())
        .unwrap_or_else(|| err.to_string())
}

fn wait_for_keypress() -> Result<()> {
    enable_raw_mode()?;
    loop {
        if let Event::Key(k) = event::read()? {
            if k.kind == KeyEventKind::Press {
                break;
            }
        }
    }
    disable_raw_mode()?;
    Ok(())
}
