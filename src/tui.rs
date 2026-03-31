use crate::system::{self, BlockDevice};
use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{CrosstermBackend, Backend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
    prelude::Stylize,
};
use serde::{Deserialize, Serialize};
use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserInfo {
    pub hostname: String,
    pub username: String,
    pub password: String,
    pub keymap: String,
    pub timezone: String,
    pub git_name: String,
    pub git_email: String,
    pub shell_ui: Option<String>,
}

impl Default for UserInfo {
    fn default() -> Self {
        Self {
            hostname: String::new(),
            username: String::new(),
            password: String::new(),
            keymap: "us".to_string(),
            timezone: "UTC".to_string(),
            git_name: String::new(),
            git_email: String::new(),
            shell_ui: None,
        }
    }
}

pub enum InstallMsg {
    Log(String),
    Progress(u16),
    Finished,
    Error(String),
}

#[derive(PartialEq)]
pub enum AppState {
    Welcome,
    UserSetupKeymap,
    SelectingDisk,
    UserSetupTimezone,
    UserSetupHostname,
    UserSetupUsername,
    UserSetupPassword,
    UserSetupGitName,
    UserSetupGitEmail,
    UserSetupShellUi,
    Confirmation,
    Installing,
    Finished,
    Error(String),
}

pub struct App {
    pub state: AppState,
    pub devices: Vec<BlockDevice>,
    pub selected_disk: Option<usize>,
    pub list_state: ListState,
    pub user_info: UserInfo,
    pub input: String,
    pub keymaps: Vec<String>,
    pub timezones: Vec<String>,
    pub shell_uis: Vec<String>,
    pub filtered_items: Vec<String>,
    pub logs: Vec<String>,
    pub log_scroll: usize,
    pub progress: u16,
    pub rx: Option<Receiver<InstallMsg>>,
}

impl App {
    pub fn new(devices: Vec<BlockDevice>, keymaps: Vec<String>, timezones: Vec<String>) -> Self {
        let mut list_state = ListState::default();
        if !devices.is_empty() {
            list_state.select(Some(0));
        }

        let shell_uis = vec![
            "None (Slate Default)".to_string(),
            "Ambxst (Modular Wayland Shell)".to_string(),
            "Caelestia (Aesthetic Quickshell)".to_string(),
            "Dank Material (Material You Shell)".to_string(),
        ];

        Self {
            state: AppState::Welcome,
            devices,
            selected_disk: None,
            list_state,
            user_info: UserInfo::default(),
            input: String::new(),
            keymaps,
            timezones,
            shell_uis,
            filtered_items: Vec::new(),
            logs: vec!["[System] App Initialized".to_string()],
            log_scroll: 0,
            progress: 0,
            rx: None,
        }
    }

    pub fn update_filter(&mut self) {
        let query = self.input.to_lowercase();
        let source = match self.state {
            AppState::UserSetupKeymap => &self.keymaps,
            AppState::UserSetupTimezone => &self.timezones,
            _ => return,
        };

        self.filtered_items = source
            .iter()
            .filter(|item| item.to_lowercase().contains(&query))
            .cloned()
            .collect();
        
        if self.filtered_items.is_empty() {
            self.list_state.select(None);
        } else {
            self.list_state.select(Some(0));
        }
    }

    pub fn next_item(&mut self) {
        let len = match self.state {
            AppState::SelectingDisk => self.devices.len(),
            AppState::UserSetupKeymap | AppState::UserSetupTimezone => self.filtered_items.len(),
            AppState::UserSetupShellUi => self.shell_uis.len(),
            _ => 0,
        };

        if len == 0 { return; }

        let i = match self.list_state.selected() {
            Some(i) => if i >= len - 1 { 0 } else { i + 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    pub fn previous_item(&mut self) {
        let len = match self.state {
            AppState::SelectingDisk => self.devices.len(),
            AppState::UserSetupKeymap | AppState::UserSetupTimezone => self.filtered_items.len(),
            AppState::UserSetupShellUi => self.shell_uis.len(),
            _ => 0,
        };

        if len == 0 { return; }

        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { len - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    fn max_log_scroll(&self) -> usize {
        self.logs.len().saturating_sub(1)
    }

    fn scroll_logs_up(&mut self, amount: usize) {
        self.log_scroll = (self.log_scroll + amount).min(self.max_log_scroll());
    }

    fn scroll_logs_down(&mut self, amount: usize) {
        self.log_scroll = self.log_scroll.saturating_sub(amount);
    }

    fn scroll_logs_home(&mut self) {
        self.log_scroll = self.max_log_scroll();
    }

    fn scroll_logs_end(&mut self) {
        self.log_scroll = 0;
    }
}

pub fn run_installer<F>(devices: Vec<BlockDevice>, forge_fn: F) -> Result<()>
where
    F: FnOnce(BlockDevice, UserInfo, Sender<InstallMsg>) + Send + 'static,
{
    let keymaps = system::list_keymaps().unwrap_or_else(|_| vec!["us".to_string()]);
    let timezones = system::list_timezones().unwrap_or_else(|_| vec!["UTC".to_string()]);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(devices, keymaps, timezones);
    let (tx, rx) = mpsc::channel();
    app.rx = Some(rx);

    let res = run_loop(&mut terminal, app, tx, Some(forge_fn));

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

fn run_loop<B: Backend, F>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tx: Sender<InstallMsg>,
    mut forge_fn: Option<F>,
) -> Result<()>
where
    F: FnOnce(BlockDevice, UserInfo, Sender<InstallMsg>) + Send + 'static,
    B::Error: std::fmt::Display + Send + Sync + 'static,
{
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Some(ref rx) = app.rx {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    InstallMsg::Log(l) => app.logs.push(l),
                    InstallMsg::Progress(p) => app.progress = p,
                    InstallMsg::Finished => app.state = AppState::Finished,
                    InstallMsg::Error(e) => app.state = AppState::Error(e),
                }
            }
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                match app.state {
                    AppState::Welcome => {
                        if key.code == KeyCode::Enter {
                            app.state = AppState::UserSetupKeymap;
                            app.input = String::new();
                            app.update_filter();
                        } else if key.code == KeyCode::Char('q') {
                            return Ok(());
                        }
                    }
                    AppState::UserSetupKeymap => match key.code {
                        KeyCode::Enter => {
                            if let Some(i) = app.list_state.selected() {
                                if let Some(selection) = app.filtered_items.get(i) {
                                    app.user_info.keymap = selection.clone();
                                    let _ = std::process::Command::new("loadkeys").arg(&app.user_info.keymap).status();
                                    app.state = AppState::SelectingDisk;
                                    app.list_state.select(Some(0));
                                }
                            }
                        }
                        KeyCode::Down => app.next_item(),
                        KeyCode::Up => app.previous_item(),
                        KeyCode::Char(c) => { app.input.push(c); app.update_filter(); }
                        KeyCode::Backspace => { app.input.pop(); app.update_filter(); }
                        KeyCode::Esc => app.state = AppState::Welcome,
                        _ => {}
                    },
                    AppState::SelectingDisk => match key.code {
                        KeyCode::Down => app.next_item(),
                        KeyCode::Up => app.previous_item(),
                        KeyCode::Enter => {
                            if let Some(i) = app.list_state.selected() {
                                app.selected_disk = Some(i);
                                app.state = AppState::UserSetupTimezone;
                                app.input = String::new();
                                app.update_filter();
                            }
                        }
                        _ => {}
                    },
                    AppState::UserSetupTimezone => match key.code {
                        KeyCode::Enter => {
                            if let Some(i) = app.list_state.selected() {
                                if let Some(selection) = app.filtered_items.get(i) {
                                    app.user_info.timezone = selection.clone();
                                    app.state = AppState::UserSetupHostname;
                                    app.input = String::new();
                                }
                            }
                        }
                        KeyCode::Down => app.next_item(),
                        KeyCode::Up => app.previous_item(),
                        KeyCode::Char(c) => { app.input.push(c); app.update_filter(); }
                        KeyCode::Backspace => { app.input.pop(); app.update_filter(); }
                        KeyCode::Esc => app.state = AppState::SelectingDisk,
                        _ => {}
                    },
                    AppState::UserSetupHostname => match key.code {
                        KeyCode::Enter => {
                            let hostname = app.input.trim().to_string();
                            if !is_valid_hostname(&hostname) {
                                app.logs.push("[Err] Invalid hostname. Use letters, digits, and '-' (not at start/end).".to_string());
                            } else {
                                app.user_info.hostname = hostname;
                                app.input.clear();
                                app.state = AppState::UserSetupUsername;
                            }
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupTimezone,
                        _ => {}
                    },
                    AppState::UserSetupUsername => match key.code {
                        KeyCode::Enter => {
                            let username = app.input.trim().to_string();
                            if !is_valid_username(&username) {
                                app.logs.push("[Err] Invalid username. Use 1-32 chars: lowercase letters, digits, '_' or '-' (must start with lowercase/_).".to_string());
                            } else {
                                app.user_info.username = username;
                                app.input.clear();
                                app.state = AppState::UserSetupPassword;
                            }
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupHostname,
                        _ => {}
                    },
                    AppState::UserSetupPassword => match key.code {
                        KeyCode::Enter => {
                            let password = app.input.trim().to_string();
                            if password.is_empty() {
                                app.logs.push("[Err] Password cannot be empty.".to_string());
                            } else {
                                app.user_info.password = password;
                                app.input.clear();
                                app.state = AppState::UserSetupGitName;
                            }
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupUsername,
                        _ => {}
                    },
                    AppState::UserSetupGitName => match key.code {
                        KeyCode::Enter => {
                            app.user_info.git_name = app.input.drain(..).collect();
                            app.state = AppState::UserSetupGitEmail;
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupPassword,
                        _ => {}
                    },
                    AppState::UserSetupGitEmail => match key.code {
                        KeyCode::Enter => {
                            app.user_info.git_email = app.input.drain(..).collect();
                            app.state = AppState::UserSetupShellUi;
                            app.list_state.select(Some(0));
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupGitName,
                        _ => {}
                    },
                    AppState::UserSetupShellUi => match key.code {
                        KeyCode::Enter => {
                            if let Some(i) = app.list_state.selected() {
                                let ui_choice = match i {
                                    1 => Some("ambxst".to_string()),
                                    2 => Some("caelestia".to_string()),
                                    3 => Some("dank-material".to_string()),
                                    _ => None,
                                };
                                app.user_info.shell_ui = ui_choice;
                                app.state = AppState::Confirmation;
                            }
                        }
                        KeyCode::Down => app.next_item(),
                        KeyCode::Up => app.previous_item(),
                        KeyCode::Esc => app.state = AppState::UserSetupGitEmail,
                        _ => {}
                    },
                    AppState::Confirmation => match key.code {
                        KeyCode::Enter => {
                            let device = app.devices[app.selected_disk.unwrap()].clone();
                            let info = app.user_info.clone();
                            app.state = AppState::Installing;
                            let tx_clone = tx.clone();
                            if let Some(f) = forge_fn.take() {
                                std::thread::spawn(move || {
                                    f(device, info, tx_clone);
                                });
                            }
                        }
                        KeyCode::Char('n') | KeyCode::Esc => app.state = AppState::SelectingDisk,
                        _ => {}
                    },
                    AppState::Finished | AppState::Error(_) => {
                        if key.code == KeyCode::Enter || key.code == KeyCode::Char('q') {
                            return Ok(());
                        }
                    }
                    AppState::Installing => match key.code {
                        KeyCode::Up => app.scroll_logs_up(1),
                        KeyCode::Down => app.scroll_logs_down(1),
                        KeyCode::PageUp => app.scroll_logs_up(10),
                        KeyCode::PageDown => app.scroll_logs_down(10),
                        KeyCode::Home => app.scroll_logs_home(),
                        KeyCode::End => app.scroll_logs_end(),
                        _ => {}
                    }
                }
                
                if key.code == KeyCode::Char('q') && !is_input_state(&app.state) {
                     return Ok(());
                }
            }
        }
    }
}

fn is_input_state(state: &AppState) -> bool {
    matches!(state, AppState::UserSetupKeymap | AppState::UserSetupTimezone | AppState::UserSetupHostname | AppState::UserSetupUsername | AppState::UserSetupPassword | AppState::UserSetupGitName | AppState::UserSetupGitEmail)
}

fn is_valid_hostname(value: &str) -> bool {
    if value.is_empty() || value.len() > 63 {
        return false;
    }

    if value.starts_with('-') || value.ends_with('-') {
        return false;
    }

    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_valid_username(value: &str) -> bool {
    if value.is_empty() || value.len() > 32 {
        return false;
    }

    let mut chars = value.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };

    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }

    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

#[derive(Clone, Copy)]
struct UiTheme {
    accent: Color,
    accent_soft: Color,
    border: Color,
    muted: Color,
    success: Color,
    error: Color,
    selection_bg: Color,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            accent: Color::Cyan,
            accent_soft: Color::LightBlue,
            border: Color::DarkGray,
            muted: Color::Gray,
            success: Color::Green,
            error: Color::Red,
            selection_bg: Color::DarkGray,
        }
    }
}

fn current_step(state: &AppState) -> (&'static str, u16, u16) {
    let total = 9;
    match state {
        AppState::Welcome | AppState::UserSetupKeymap => ("Keymap", 1, total),
        AppState::SelectingDisk => ("Disk", 2, total),
        AppState::UserSetupTimezone => ("Timezone", 3, total),
        AppState::UserSetupHostname => ("Hostname", 4, total),
        AppState::UserSetupUsername => ("Username", 5, total),
        AppState::UserSetupPassword | AppState::UserSetupGitName | AppState::UserSetupGitEmail => ("Credentials", 6, total),
        AppState::UserSetupShellUi => ("Shell UI", 7, total),
        AppState::Confirmation => ("Confirm", 8, total),
        AppState::Installing | AppState::Finished | AppState::Error(_) => ("Install", 9, total),
    }
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    if max_chars <= 1 {
        return "...".chars().take(max_chars).collect();
    }
    let keep = max_chars.saturating_sub(1);
    let mut out: String = input.chars().take(keep).collect();
    out.push('…');
    out
}

fn help_text_for_state(state: &AppState) -> &'static str {
    match state {
        AppState::Welcome => "Enter: Begin | q: Quit",
        AppState::SelectingDisk | AppState::UserSetupKeymap | AppState::UserSetupTimezone | AppState::UserSetupShellUi => {
            "Up/Down: Move | Enter: Select | Esc: Back | q: Quit"
        }
        AppState::UserSetupHostname
        | AppState::UserSetupUsername
        | AppState::UserSetupPassword
        | AppState::UserSetupGitName
        | AppState::UserSetupGitEmail => "Type: Input | Backspace: Delete | Enter: Continue | Esc: Back",
        AppState::Confirmation => "Enter: Install | Esc/N: Back | q: Quit",
        AppState::Installing => "Up/Down: Scroll logs | PgUp/PgDn: Faster | Home/End: Oldest/Latest",
        AppState::Finished | AppState::Error(_) => "Enter/q: Exit",
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let theme = UiTheme::default();
    let size = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(size);

    let header_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(chunks[0]);

    let title = Paragraph::new(Line::from(vec![
        Span::styled("SLATE", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::styled("  ARCH LINUX INSTALLER", Style::default().fg(theme.accent_soft)),
    ]))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.border)))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(title, header_chunks[0]);

    let (step_name, step_idx, step_total) = current_step(&app.state);
    let step_percent = (step_idx * 100 / step_total) as u16;
    let step = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.border)).title("Setup Progress".fg(theme.accent).bold()))
        .percent(step_percent)
        .label(format!("STEP {}/{} {}", step_idx, step_total, step_name))
        .gauge_style(Style::default().fg(theme.accent));
    f.render_widget(step, header_chunks[1]);

    // Clear body area every frame to avoid stale glyph artifacts when switching
    // between states with very different layouts.
    f.render_widget(Clear, chunks[1]);

    match &app.state {
        AppState::Welcome => {
            let p = Paragraph::new("Welcome to Slate!\n\nThis will install an opinionated Arch/Hyprland Desktop.\n\nPress Enter to begin.")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title("Welcome".fg(theme.accent).bold())
                );
            f.render_widget(p, chunks[1]);
        }
        AppState::SelectingDisk => {
            let available = chunks[1].width.saturating_sub(10) as usize;
            let model_max = available.saturating_sub(26).max(10);
            let items: Vec<ListItem> = app
                .devices
                .iter()
                .map(|d| {
                    let model = truncate_with_ellipsis(&d.model, model_max);
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("{:<14}", d.path),
                            Style::default().fg(theme.accent).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::styled(format!("{:>8}", d.size), Style::default().fg(theme.accent_soft)),
                        Span::raw("  "),
                        Span::styled(model, Style::default().fg(theme.muted)),
                    ]))
                })
                .collect();
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title("Select Disk".fg(theme.accent).bold())
                )
                .highlight_symbol("▶ ")
                .highlight_style(
                    Style::default()
                        .fg(theme.accent)
                        .bg(theme.selection_bg)
                        .add_modifier(Modifier::BOLD),
                );
            f.render_stateful_widget(list, chunks[1], &mut app.list_state);
        }
        AppState::UserSetupKeymap | AppState::UserSetupTimezone => {
            let prompt = if app.state == AppState::UserSetupKeymap { "Search Keymap:" } else { "Search Timezone:" };
            let sub_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(5)]).split(chunks[1]);
            
            let search_p = Paragraph::new(app.input.clone()).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(prompt.fg(theme.accent).bold())
            );
            f.render_widget(search_p, sub_chunks[0]);
            
            let items: Vec<ListItem> = app.filtered_items.iter().map(|i| ListItem::new(i.as_str())).collect();
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title("Results".fg(theme.accent).bold())
                )
                .highlight_symbol("▶ ")
                .highlight_style(Style::default().fg(theme.accent).bg(theme.selection_bg).add_modifier(Modifier::BOLD));
            f.render_stateful_widget(list, sub_chunks[1], &mut app.list_state);
        }
        AppState::UserSetupHostname | AppState::UserSetupUsername | AppState::UserSetupPassword | AppState::UserSetupGitName | AppState::UserSetupGitEmail => {
            let prompt = match app.state {
                AppState::UserSetupHostname => "Hostname:",
                AppState::UserSetupUsername => "Username:",
                AppState::UserSetupPassword => "Password:",
                AppState::UserSetupGitName => "Git Username (Optional):",
                AppState::UserSetupGitEmail => "Git Email (Optional):",
                _ => "",
            };
            let text = if app.state == AppState::UserSetupPassword { "*".repeat(app.input.len()) } else { app.input.clone() };
            let sub_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Length(3), Constraint::Min(1)])
                .split(chunks[1]);

            let p = Paragraph::new(text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title(prompt.fg(theme.accent).bold())
            );
            f.render_widget(p, sub_chunks[0]);

            let trimmed = app.input.trim();
            let (status, status_style) = match app.state {
                AppState::UserSetupHostname => {
                    if trimmed.is_empty() {
                        ("Required: letters, digits, '-' (not at start/end)", Style::default().fg(theme.muted))
                    } else if is_valid_hostname(trimmed) {
                        ("Valid hostname", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
                    } else {
                        ("Invalid hostname format", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))
                    }
                }
                AppState::UserSetupUsername => {
                    if trimmed.is_empty() {
                        ("Required: starts with lowercase/_; then lowercase, digits, '_' or '-'", Style::default().fg(theme.muted))
                    } else if is_valid_username(trimmed) {
                        ("Valid username", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
                    } else {
                        ("Invalid username format", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))
                    }
                }
                AppState::UserSetupPassword => {
                    if trimmed.is_empty() {
                        ("Password is required", Style::default().fg(theme.error).add_modifier(Modifier::BOLD))
                    } else {
                        ("Password set", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
                    }
                }
                AppState::UserSetupGitName | AppState::UserSetupGitEmail => {
                    if trimmed.is_empty() {
                        ("Optional: press Enter to skip", Style::default().fg(theme.muted))
                    } else {
                        ("Value captured", Style::default().fg(theme.success).add_modifier(Modifier::BOLD))
                    }
                }
                _ => ("", Style::default().fg(theme.muted)),
            };

            let status_panel = Paragraph::new(Line::from(vec![
                Span::styled("Status: ", Style::default().fg(theme.muted)),
                Span::styled(status, status_style),
            ]))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border))
                    .title("Validation".fg(theme.accent).bold())
            )
            .wrap(Wrap { trim: true });
            f.render_widget(status_panel, sub_chunks[1]);
        }
        AppState::UserSetupShellUi => {
            let items: Vec<ListItem> = app.shell_uis.iter().map(|i| ListItem::new(i.as_str())).collect();
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title("Select Shell UI (Quickshell-based)".fg(theme.accent).bold())
                )
                .highlight_symbol("▶ ")
                .highlight_style(Style::default().fg(theme.accent).bg(theme.selection_bg).add_modifier(Modifier::BOLD));
            f.render_stateful_widget(list, chunks[1], &mut app.list_state);
        }
        AppState::Confirmation => {
             let panel = Block::default()
                 .borders(Borders::ALL)
                 .border_style(Style::default().fg(theme.border))
                 .title("Final Confirmation".fg(theme.accent).bold());
             let inner = panel.inner(chunks[1]);
             f.render_widget(panel, chunks[1]);

             let cols = Layout::default()
                 .direction(Direction::Horizontal)
                 .constraints([Constraint::Length(22), Constraint::Min(20)])
                 .split(inner);

             let labels = Paragraph::new(vec![
                 Line::from(Span::styled("Disk", Style::default().fg(theme.muted))),
                 Line::from(Span::styled("Keymap", Style::default().fg(theme.muted))),
                 Line::from(Span::styled("Timezone", Style::default().fg(theme.muted))),
                 Line::from(Span::styled("Hostname", Style::default().fg(theme.muted))),
                 Line::from(Span::styled("User", Style::default().fg(theme.muted))),
                 Line::from(""),
                 Line::from(Span::styled("Desktop", Style::default().fg(theme.muted))),
                 Line::from(Span::styled("Shell UI", Style::default().fg(theme.muted))),
                 Line::from(Span::styled("Git Config", Style::default().fg(theme.muted))),
                 Line::from(""),
                 Line::from(Span::styled("Action", Style::default().fg(theme.muted))),
             ])
             .wrap(Wrap { trim: true });

             let disk_value = app
                 .selected_disk
                 .and_then(|i| app.devices.get(i))
                 .map(|d| d.path.clone())
                 .unwrap_or_else(|| "(none)".to_string());
             let git_value = if app.user_info.git_name.is_empty() || app.user_info.git_email.is_empty() {
                 "Skipped".to_string()
             } else {
                 format!("{} <{}>", app.user_info.git_name, app.user_info.git_email)
             };

             let shell_ui_value = app.user_info.shell_ui.as_deref().unwrap_or("None (Slate Default)");
             let values = Paragraph::new(vec![
                 Line::from(Span::styled(disk_value, Style::default().fg(theme.accent).add_modifier(Modifier::BOLD))),
                 Line::from(Span::raw(app.user_info.keymap.clone())),
                 Line::from(Span::raw(app.user_info.timezone.clone())),
                 Line::from(Span::raw(app.user_info.hostname.clone())),
                 Line::from(Span::raw(app.user_info.username.clone())),
                 Line::from(""),
                 Line::from(Span::raw("Hyprland + Zsh + Modern CLI Extras")),
                 Line::from(Span::raw(shell_ui_value)),
                 Line::from(Span::raw(git_value)),
                 Line::from(""),
                 Line::from(Span::styled("Press Enter to CONFIRM and INSTALL", Style::default().fg(theme.accent_soft).add_modifier(Modifier::BOLD))),
             ])
             .wrap(Wrap { trim: true });

             f.render_widget(labels, cols[0]);
             f.render_widget(values, cols[1]);
        }
        AppState::Installing | AppState::Finished => {
            let install_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(5)]).split(chunks[1]);
            
            let status_title = if app.state == AppState::Finished { "INSTALLATION COMPLETE!" } else { "Installing..." };
            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title(status_title.fg(theme.accent).bold())
                )
                .percent(app.progress)
                .gauge_style(if app.state == AppState::Finished {
                    Style::default().fg(theme.success)
                } else {
                    Style::default().fg(theme.accent)
                });
            f.render_widget(gauge, install_chunks[0]);
            
            let view_height = install_chunks[1].height.saturating_sub(2) as usize;
            let log_window = view_height.max(1);
            let end = app.logs.len().saturating_sub(app.log_scroll);
            let start = end.saturating_sub(log_window);
            let visible_logs = &app.logs[start..end];

            let log_lines: Vec<Line> = visible_logs
                .iter()
                .map(|l| {
                    let lower = l.to_ascii_lowercase();
                    let style = if l.starts_with("[Err]") || lower.starts_with("error") {
                        Style::default().fg(theme.error).add_modifier(Modifier::BOLD)
                    } else if lower.contains("warn") {
                        Style::default().fg(Color::Yellow)
                    } else if l.starts_with("$ ") {
                        Style::default().fg(theme.accent_soft)
                    } else {
                        Style::default().fg(theme.muted)
                    };
                    Line::from(Span::styled(l.clone(), style))
                })
                .collect();
            let logs_title = if app.log_scroll == 0 {
                "Logs (latest)".to_string()
            } else {
                format!("Logs (scroll +{})", app.log_scroll)
            };
            let p = Paragraph::new(log_lines)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title(logs_title.fg(theme.accent).bold())
                )
                .wrap(Wrap { trim: false });
            f.render_widget(p, install_chunks[1]);
            
            if app.state == AppState::Finished {
                let overlay = Rect::new(size.width / 4, size.height / 3, size.width / 2, size.height / 3);
                f.render_widget(Clear, overlay);
                let p = Paragraph::new("\nSUCCESS\n\nInstallation Finished.\nYou can now reboot.\n\nPress Enter or 'q' to exit.")
                    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.success)).title("Done".fg(theme.success).bold()))
                    .alignment(ratatui::layout::Alignment::Center);
                f.render_widget(p, overlay);
            }
        }
        AppState::Error(e) => {
            let err_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(4), Constraint::Min(5)])
                .split(chunks[1]);

            let summary = Paragraph::new("Installer failed. See details below and press Enter to exit.")
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.error))
                        .title("Installer Error".fg(theme.error).bold())
                )
                .wrap(Wrap { trim: true })
                .style(Style::default().fg(theme.error).add_modifier(Modifier::BOLD));
            f.render_widget(summary, err_chunks[0]);

            let detail = Paragraph::new(e.as_str())
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(theme.border))
                        .title("Details".fg(theme.accent).bold())
                )
                .wrap(Wrap { trim: false })
                .style(Style::default().fg(theme.muted));
            f.render_widget(detail, err_chunks[1]);
        }
    }

    let help = Paragraph::new(help_text_for_state(&app.state))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.border)).title("Keys".fg(theme.accent).bold()))
        .style(Style::default().fg(theme.muted))
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(help, chunks[2]);
}
