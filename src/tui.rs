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
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Gauge, Wrap},
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
    pub filtered_items: Vec<String>,
    pub logs: Vec<String>,
    pub progress: u16,
    pub rx: Option<Receiver<InstallMsg>>,
}

impl App {
    pub fn new(devices: Vec<BlockDevice>, keymaps: Vec<String>, timezones: Vec<String>) -> Self {
        let mut list_state = ListState::default();
        if !devices.is_empty() {
            list_state.select(Some(0));
        }

        Self {
            state: AppState::Welcome,
            devices,
            selected_disk: None,
            list_state,
            user_info: UserInfo::default(),
            input: String::new(),
            keymaps,
            timezones,
            filtered_items: Vec::new(),
            logs: vec!["[System] App Initialized".to_string()],
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
            _ => 0,
        };

        if len == 0 { return; }

        let i = match self.list_state.selected() {
            Some(i) => if i == 0 { len - 1 } else { i - 1 },
            None => 0,
        };
        self.list_state.select(Some(i));
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
                            app.user_info.hostname = app.input.drain(..).collect();
                            app.state = AppState::UserSetupUsername;
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupTimezone,
                        _ => {}
                    },
                    AppState::UserSetupUsername => match key.code {
                        KeyCode::Enter => {
                            app.user_info.username = app.input.drain(..).collect();
                            app.state = AppState::UserSetupPassword;
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupHostname,
                        _ => {}
                    },
                    AppState::UserSetupPassword => match key.code {
                        KeyCode::Enter => {
                            app.user_info.password = app.input.drain(..).collect();
                            app.state = AppState::UserSetupGitName;
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
                            app.state = AppState::Confirmation;
                        }
                        KeyCode::Char(c) => app.input.push(c),
                        KeyCode::Backspace => { app.input.pop(); }
                        KeyCode::Esc => app.state = AppState::UserSetupGitName,
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
                    AppState::Installing => {}
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

fn ui(f: &mut Frame, app: &mut App) {
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

    let title = Paragraph::new("SLATE ARCH LINUX INSTALLER")
        .block(Block::default().borders(Borders::ALL))
        .alignment(ratatui::layout::Alignment::Center);
    f.render_widget(title, chunks[0]);

    match &app.state {
        AppState::Welcome => {
            let p = Paragraph::new("Welcome to Slate!\n\nThis will install an opinionated Arch/Hyprland Desktop.\n\nPress Enter to begin.")
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(p, chunks[1]);
        }
        AppState::SelectingDisk => {
            let items: Vec<ListItem> = app.devices.iter().map(|d| ListItem::new(format!("{} | {} | {}", d.path, d.size, d.model))).collect();
            let list = List::new(items).block(Block::default().borders(Borders::ALL).title("Select Disk")).highlight_symbol(">> ");
            f.render_stateful_widget(list, chunks[1], &mut app.list_state);
        }
        AppState::UserSetupKeymap | AppState::UserSetupTimezone => {
            let prompt = if app.state == AppState::UserSetupKeymap { "Search Keymap:" } else { "Search Timezone:" };
            let sub_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(5)]).split(chunks[1]);
            
            let search_p = Paragraph::new(app.input.clone()).block(Block::default().borders(Borders::ALL).title(prompt));
            f.render_widget(search_p, sub_chunks[0]);
            
            let items: Vec<ListItem> = app.filtered_items.iter().map(|i| ListItem::new(i.as_str())).collect();
            let list = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Results (Up/Down to scroll)"))
                .highlight_symbol(">> ")
                .highlight_style(ratatui::style::Style::default().fg(ratatui::style::Color::Cyan));
            f.render_stateful_widget(list, sub_chunks[1], &mut app.list_state);
        }
        AppState::UserSetupHostname | AppState::UserSetupUsername | AppState::UserSetupPassword | AppState::UserSetupGitName | AppState::UserSetupGitEmail => {
            let prompt = match app.state {
                AppState::UserSetupHostname => "Hostname:",
                AppState::UserSetupUsername => "Username:",
                AppState::UserSetupPassword => "Password:",
                AppState::UserSetupGitName => "Git Username (Optional, Enter to skip):",
                AppState::UserSetupGitEmail => "Git Email (Optional, Enter to skip):",
                _ => "",
            };
            let text = if app.state == AppState::UserSetupPassword { "*".repeat(app.input.len()) } else { app.input.clone() };
            let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title(prompt));
            f.render_widget(p, chunks[1]);
        }
        AppState::Confirmation => {
             let text = format!(
                 "Opinionated Configuration Summary:\n\nDisk: {}\nKeymap: {}\nTimezone: {}\nHostname: {}\nUser: {}\n\nDesktop: Hyprland + Zsh + Modern CLI Extras\nGit Config: {} <{}>\n\nPress Enter to CONFIRM and INSTALL.",
                 app.devices[app.selected_disk.unwrap()].path,
                 app.user_info.keymap,
                 app.user_info.timezone,
                 app.user_info.hostname,
                 app.user_info.username,
                 if app.user_info.git_name.is_empty() { "Skipped" } else { &app.user_info.git_name },
                 if app.user_info.git_email.is_empty() { "Skipped" } else { &app.user_info.git_email }
             );
             let p = Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Final Confirmation"));
             f.render_widget(p, chunks[1]);
        }
        AppState::Installing | AppState::Finished => {
            let install_chunks = Layout::default().direction(Direction::Vertical).constraints([Constraint::Length(3), Constraint::Min(5)]).split(chunks[1]);
            
            let status_title = if app.state == AppState::Finished { "INSTALLATION COMPLETE!" } else { "Installing..." };
            let gauge = Gauge::default().block(Block::default().borders(Borders::ALL).title(status_title)).percent(app.progress).gauge_style(if app.state == AppState::Finished { ratatui::style::Style::default().fg(ratatui::style::Color::Green) } else { ratatui::style::Style::default().fg(ratatui::style::Color::Cyan) });
            f.render_widget(gauge, install_chunks[0]);
            
            let log_text = app.logs.iter().rev().take(30).map(|l| l.as_str()).collect::<Vec<&str>>().join("\n");
            let p = Paragraph::new(log_text)
                .block(Block::default().borders(Borders::ALL).title("Logs"))
                .wrap(Wrap { trim: false });
            f.render_widget(p, install_chunks[1]);
            
            if app.state == AppState::Finished {
                let overlay = Rect::new(size.width / 4, size.height / 3, size.width / 2, size.height / 3);
                f.render_widget(ratatui::widgets::Clear, overlay);
                let p = Paragraph::new("\nSUCCESS\n\nInstallation Finished.\nYou can now reboot.\n\nPress Enter or 'q' to exit.")
                    .block(Block::default().borders(Borders::ALL).fg(ratatui::style::Color::Green))
                    .alignment(ratatui::layout::Alignment::Center);
                f.render_widget(p, overlay);
            }
        }
        AppState::Error(e) => {
            let p = Paragraph::new(format!("ERROR: {}\n\nPress Enter to exit.", e)).block(Block::default().borders(Borders::ALL).fg(ratatui::style::Color::Red));
            f.render_widget(p, chunks[1]);
        }
    }

    let help = Paragraph::new("Arrows: Scroll | Type: Search | Enter: Select/Skip | q: Quit").block(Block::default().borders(Borders::ALL)).alignment(ratatui::layout::Alignment::Center);
    f.render_widget(help, chunks[2]);
}
