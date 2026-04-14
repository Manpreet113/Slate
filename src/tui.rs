use crate::installer::{self, EventSink, InstallEvent, InstallPlan, StageId};
use crate::system::BlockDevice;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

const FORM_FIELDS: usize = 9;

#[derive(Clone)]
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

#[derive(Clone)]
enum SelectorKind {
    Disk,
    Keymap,
    Timezone,
}

enum Screen {
    Plan,
    Selector(SelectorKind),
    Review,
    Installing,
    Result,
}

struct App {
    screen: Screen,
    selected_field: usize,
    user_info: UserInfo,
    devices: Vec<BlockDevice>,
    selected_disk: usize,
    keymaps: Vec<String>,
    timezones: Vec<String>,
    selector_input: String,
    selector_state: ListState,
    logs: Vec<String>,
    stage_states: Vec<(StageId, StageStatus)>,
    rx: Option<Receiver<InstallEvent>>,
    result_message: Option<String>,
    install_failed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum StageStatus {
    Pending,
    Active,
    Done,
}

impl App {
    fn new(devices: Vec<BlockDevice>, keymaps: Vec<String>, timezones: Vec<String>) -> Self {
        let mut selector_state = ListState::default();
        selector_state.select(Some(0));
        Self {
            screen: Screen::Plan,
            selected_field: 0,
            user_info: UserInfo::default(),
            devices,
            selected_disk: 0,
            keymaps,
            timezones,
            selector_input: String::new(),
            selector_state,
            logs: vec!["Slate installer ready".to_string()],
            stage_states: StageId::ALL
                .into_iter()
                .map(|stage| (stage, StageStatus::Pending))
                .collect(),
            rx: None,
            result_message: None,
            install_failed: false,
        }
    }

    fn build_plan(&self) -> Result<InstallPlan> {
        let plan = InstallPlan {
            disk: self
                .devices
                .get(self.selected_disk)
                .map(|device| device.path.clone())
                .unwrap_or_default(),
            hostname: self.user_info.hostname.clone(),
            username: self.user_info.username.clone(),
            password: self.user_info.password.clone(),
            keymap: self.user_info.keymap.clone(),
            timezone: self.user_info.timezone.clone(),
            git_name: self.user_info.git_name.clone(),
            git_email: self.user_info.git_email.clone(),
            desktop_profile: "Slate".to_string(),
        };
        plan.validate()?;
        Ok(plan)
    }

    fn progress(&self) -> u16 {
        let completed = self
            .stage_states
            .iter()
            .filter(|(_, state)| *state == StageStatus::Done)
            .count() as u16;
        completed * 100 / StageId::ALL.len() as u16
    }

    fn selected_disk_label(&self) -> String {
        self.devices
            .get(self.selected_disk)
            .map(|disk| format!("{}  {}  {}", disk.path, disk.size, disk.model))
            .unwrap_or_else(|| "No disk".to_string())
    }

    fn selector_items(&self, kind: &SelectorKind) -> Vec<String> {
        let query = self.selector_input.to_lowercase();
        let items: Vec<String> = match kind {
            SelectorKind::Disk => self
                .devices
                .iter()
                .map(|disk| format!("{}  {}  {}", disk.path, disk.size, disk.model))
                .collect(),
            SelectorKind::Keymap => self.keymaps.clone(),
            SelectorKind::Timezone => self.timezones.clone(),
        };

        if query.is_empty() {
            return items;
        }

        items
            .into_iter()
            .filter(|item| item.to_lowercase().contains(&query))
            .collect()
    }

    fn current_stage_label(&self) -> String {
        self.stage_states
            .iter()
            .find_map(|(stage, status)| {
                if *status == StageStatus::Active {
                    Some(stage.label().to_string())
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "Idle".to_string())
    }
}

pub fn run_installer(devices: Vec<BlockDevice>) -> Result<()> {
    let keymaps = crate::system::list_keymaps().unwrap_or_else(|_| vec!["us".to_string()]);
    let timezones = crate::system::list_timezones().unwrap_or_else(|_| vec!["UTC".to_string()]);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, App::new(devices, keymaps, timezones));
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, mut app: App) -> Result<()> {
    loop {
        drain_events(&mut app);
        terminal.draw(|frame| render(frame, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            let ev = event::read()?;
            if let Event::Key(key) = ev {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match app.screen {
                    Screen::Plan => handle_plan_keys(&mut app, key.code)?,
                    Screen::Selector(_) => handle_selector_keys(&mut app, key.code),
                    Screen::Review => handle_review_keys(&mut app, key.code)?,
                    Screen::Installing => handle_installing_keys(&mut app, key.code),
                    Screen::Result => {
                        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
                            break;
                        }
                    }
                }
            }
        }

        if matches!(app.screen, Screen::Result) && app.result_message.is_some() {
            continue;
        }
    }

    Ok(())
}

fn handle_plan_keys(app: &mut App, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Up => {
            if app.selected_field == 0 {
                app.selected_field = FORM_FIELDS - 1;
            } else {
                app.selected_field -= 1;
            }
        }
        KeyCode::Down | KeyCode::Tab => {
            app.selected_field = (app.selected_field + 1) % FORM_FIELDS;
        }
        KeyCode::Enter => match app.selected_field {
            0 => enter_selector(app, SelectorKind::Disk),
            4 => enter_selector(app, SelectorKind::Keymap),
            5 => enter_selector(app, SelectorKind::Timezone),
            8 => {
                app.build_plan()?;
                app.screen = Screen::Review;
            }
            _ => {}
        },
        KeyCode::Backspace => {
            if let Some(field) = current_text_field_mut(app) {
                field.pop();
            }
        }
        KeyCode::Esc | KeyCode::Char('q') => std::process::exit(0),
        KeyCode::Char(ch) => {
            if let Some(field) = current_text_field(app) {
                if !field.read_only {
                    if let Some(text) = current_text_field_mut(app) {
                        text.push(ch);
                    }
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn handle_selector_keys(app: &mut App, code: KeyCode) {
    let kind = match &app.screen {
        Screen::Selector(kind) => kind.clone(),
        _ => return,
    };

    let items = app.selector_items(&kind);
    match code {
        KeyCode::Esc => {
            app.screen = Screen::Plan;
            app.selector_input.clear();
        }
        KeyCode::Up => {
            let next = app.selector_state.selected().unwrap_or(0).saturating_sub(1);
            app.selector_state.select(Some(next));
        }
        KeyCode::Down => {
            let max_index = items.len().saturating_sub(1);
            let next = (app.selector_state.selected().unwrap_or(0) + 1).min(max_index);
            app.selector_state.select(Some(next));
        }
        KeyCode::Backspace => {
            app.selector_input.pop();
            app.selector_state.select(Some(0));
        }
        KeyCode::Char(ch) => {
            app.selector_input.push(ch);
            app.selector_state.select(Some(0));
        }
        KeyCode::Enter => {
            let index = app.selector_state.selected().unwrap_or(0);
            if let Some(value) = items.get(index) {
                match kind {
                    SelectorKind::Disk => {
                        if let Some(device_index) = app
                            .devices
                            .iter()
                            .position(|disk| value.starts_with(&disk.path))
                        {
                            app.selected_disk = device_index;
                        }
                    }
                    SelectorKind::Keymap => app.user_info.keymap = value.clone(),
                    SelectorKind::Timezone => app.user_info.timezone = value.clone(),
                }
                app.screen = Screen::Plan;
                app.selector_input.clear();
            }
        }
        _ => {}
    }
}

fn handle_review_keys(app: &mut App, code: KeyCode) -> Result<()> {
    match code {
        KeyCode::Esc => app.screen = Screen::Plan,
        KeyCode::Enter => {
            let plan = app.build_plan()?;
            let (tx, rx) = mpsc::channel();
            app.rx = Some(rx);
            app.logs.clear();
            app.logs.push("Starting install...".to_string());
            reset_stage_states(app);
            app.screen = Screen::Installing;
            thread::spawn(move || installer::run_install(plan, EventSink::new(tx)));
        }
        _ => {}
    }
    Ok(())
}

fn handle_installing_keys(app: &mut App, code: KeyCode) {
    if matches!(code, KeyCode::Char('q')) {
        app.logs
            .push("Install is running. Wait for completion before exiting.".to_string());
    }
}

fn drain_events(app: &mut App) {
    if let Some(rx) = &app.rx {
        while let Ok(event) = rx.try_recv() {
            match event {
                InstallEvent::Log(line) => app.logs.push(line),
                InstallEvent::StageStarted(stage) => {
                    for (_, status) in &mut app.stage_states {
                        if *status == StageStatus::Active {
                            *status = StageStatus::Done;
                        }
                    }
                    if let Some((_, status)) =
                        app.stage_states.iter_mut().find(|(id, _)| *id == stage)
                    {
                        *status = StageStatus::Active;
                    }
                }
                InstallEvent::StageFinished(stage) => {
                    if let Some((_, status)) =
                        app.stage_states.iter_mut().find(|(id, _)| *id == stage)
                    {
                        *status = StageStatus::Done;
                    }
                }
                InstallEvent::Failed { stage, message } => {
                    app.install_failed = true;
                    app.result_message = Some(match stage {
                        Some(stage) => format!("{}: {}", stage.label(), message),
                        None => message,
                    });
                    app.screen = Screen::Result;
                }
                InstallEvent::Finished => {
                    app.install_failed = false;
                    app.result_message = Some("Install completed successfully.".to_string());
                    app.screen = Screen::Result;
                }
            }
        }
    }
}

fn render(frame: &mut Frame<'_>, app: &mut App) {
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Slate ")
        .border_style(Style::default().fg(Color::Rgb(120, 135, 150)));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 2,
    });

    let selector_kind = match &app.screen {
        Screen::Selector(kind) => Some(kind.clone()),
        _ => None,
    };

    match selector_kind {
        Some(kind) => {
            render_plan(frame, inner, app);
            render_selector(frame, inner, app, &kind);
        }
        None => match &app.screen {
            Screen::Plan => render_plan(frame, inner, app),
            Screen::Review => render_review(frame, inner, app),
            Screen::Installing => render_installing(frame, inner, app),
            Screen::Result => render_result(frame, inner, app),
            Screen::Selector(_) => {}
        },
    }
}

fn render_plan(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    let header = Paragraph::new(vec![
        Line::from(Span::styled(
            "One-pass Arch + Slate desktop install",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            "Full-disk wipe only. Enter opens selectors. Tab moves forward.",
            Style::default().fg(Color::Rgb(160, 170, 180)),
        )),
    ]);
    frame.render_widget(header, rows[0]);

    let disk_label = app.selected_disk_label();
    let password_mask = "*".repeat(app.user_info.password.chars().count());
    let items = vec![
        field_line("Disk", &disk_label, app.selected_field == 0),
        field_line("Hostname", &app.user_info.hostname, app.selected_field == 1),
        field_line("Username", &app.user_info.username, app.selected_field == 2),
        field_line("Password", &password_mask, app.selected_field == 3),
        field_line("Keymap", &app.user_info.keymap, app.selected_field == 4),
        field_line("Timezone", &app.user_info.timezone, app.selected_field == 5),
        field_line("Git Name", &app.user_info.git_name, app.selected_field == 6),
        field_line(
            "Git Email",
            &app.user_info.git_email,
            app.selected_field == 7,
        ),
        field_line(
            "Continue",
            "Review destructive summary",
            app.selected_field == 8,
        ),
    ];
    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Plan ")
            .border_style(Style::default().fg(Color::Rgb(120, 135, 150))),
    );
    frame.render_widget(list, rows[1]);

    let footer = Paragraph::new("Up/Down: move  Enter: select/open  Esc: quit")
        .style(Style::default().fg(Color::Rgb(150, 160, 170)));
    frame.render_widget(footer, rows[2]);
}

fn render_selector(
    frame: &mut Frame<'_>,
    area: ratatui::layout::Rect,
    app: &mut App,
    kind: &SelectorKind,
) {
    let popup = centered_rect(area, 70, 70);
    frame.render_widget(Clear, popup);
    let items = app.selector_items(kind);
    let list_items: Vec<ListItem<'_>> = items
        .iter()
        .map(|item| ListItem::new(item.as_str()))
        .collect();
    let title = match kind {
        SelectorKind::Disk => "Select Disk",
        SelectorKind::Keymap => "Select Keymap",
        SelectorKind::Timezone => "Select Timezone",
    };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(5)])
        .split(popup);
    frame.render_widget(
        Paragraph::new(format!("Filter: {}", app.selector_input)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .border_style(Style::default().fg(Color::Rgb(220, 180, 90))),
        ),
        layout[0],
    );
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(40, 58, 74))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    frame.render_stateful_widget(list, layout[1], &mut app.selector_state);
}

fn render_review(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let text = vec![
        Line::from(Span::styled(
            "This will wipe the selected disk.",
            Style::default()
                .fg(Color::Rgb(230, 110, 90))
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("Disk: {}", app.selected_disk_label())),
        Line::from("Layout: 1G EFI + remaining Btrfs with @, @home, @log, @pkg, @snapshots"),
        Line::from(format!("Hostname: {}", app.user_info.hostname)),
        Line::from(format!("User: {}", app.user_info.username)),
        Line::from(format!("Keymap: {}", app.user_info.keymap)),
        Line::from(format!("Timezone: {}", app.user_info.timezone)),
        Line::from("Desktop: Slate (Hyprland + shell assets)"),
        Line::from(""),
        Line::from("Enter to start install. Esc to go back."),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(" Review "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_installing(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(area);
    let upper = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(20)])
        .split(layout[0]);

    let stage_items: Vec<ListItem<'_>> = app
        .stage_states
        .iter()
        .map(|(stage, status)| {
            let marker = match status {
                StageStatus::Pending => "·",
                StageStatus::Active => ">",
                StageStatus::Done => "✓",
            };
            let style = match status {
                StageStatus::Pending => Style::default().fg(Color::Rgb(140, 150, 160)),
                StageStatus::Active => Style::default()
                    .fg(Color::Rgb(230, 190, 95))
                    .add_modifier(Modifier::BOLD),
                StageStatus::Done => Style::default().fg(Color::Rgb(120, 190, 130)),
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} "), style),
                Span::styled(stage.label(), style),
            ]))
        })
        .collect();

    frame.render_widget(
        List::new(stage_items).block(Block::default().borders(Borders::ALL).title(" Stages ")),
        upper[0],
    );

    let log_lines: Vec<Line<'_>> = app
        .logs
        .iter()
        .rev()
        .take((upper[1].height as usize).saturating_sub(2))
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(Line::from)
        .collect();
    frame.render_widget(
        Paragraph::new(log_lines)
            .block(Block::default().borders(Borders::ALL).title(" Logs "))
            .wrap(Wrap { trim: false }),
        upper[1],
    );

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Progress "))
        .gauge_style(
            Style::default()
                .fg(Color::Rgb(210, 170, 85))
                .bg(Color::Rgb(36, 42, 48)),
        )
        .percent(app.progress())
        .label(app.current_stage_label());
    frame.render_widget(gauge, layout[1]);
}

fn render_result(frame: &mut Frame<'_>, area: ratatui::layout::Rect, app: &App) {
    let title = if app.install_failed {
        " Failed "
    } else {
        " Complete "
    };
    let color = if app.install_failed {
        Color::Rgb(230, 110, 90)
    } else {
        Color::Rgb(120, 190, 130)
    };
    frame.render_widget(
        Paragraph::new(app.result_message.clone().unwrap_or_default())
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(Style::default().fg(color)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn field_line<'a>(label: &'a str, value: &'a str, selected: bool) -> ListItem<'a> {
    let marker = if selected { ">" } else { " " };
    let style = if selected {
        Style::default()
            .bg(Color::Rgb(40, 58, 74))
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Rgb(205, 210, 215))
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{marker} "), style),
        Span::styled(format!("{label:<10} "), style),
        Span::styled(value, style),
    ]))
}

struct FieldMeta {
    read_only: bool,
}

fn current_text_field(app: &App) -> Option<FieldMeta> {
    match app.selected_field {
        1 | 2 | 3 | 6 | 7 => Some(FieldMeta { read_only: false }),
        0 | 4 | 5 | 8 => Some(FieldMeta { read_only: true }),
        _ => None,
    }
}

fn current_text_field_mut(app: &mut App) -> Option<&mut String> {
    match app.selected_field {
        1 => Some(&mut app.user_info.hostname),
        2 => Some(&mut app.user_info.username),
        3 => Some(&mut app.user_info.password),
        6 => Some(&mut app.user_info.git_name),
        7 => Some(&mut app.user_info.git_email),
        _ => None,
    }
}

fn enter_selector(app: &mut App, kind: SelectorKind) {
    app.selector_input.clear();
    app.selector_state.select(Some(0));
    app.screen = Screen::Selector(kind);
}

fn reset_stage_states(app: &mut App) {
    for (_, status) in &mut app.stage_states {
        *status = StageStatus::Pending;
    }
    app.result_message = None;
    app.install_failed = false;
}

fn centered_rect(
    area: ratatui::layout::Rect,
    width_pct: u16,
    height_pct: u16,
) -> ratatui::layout::Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_pct) / 2),
            Constraint::Percentage(height_pct),
            Constraint::Percentage((100 - height_pct) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_pct) / 2),
            Constraint::Percentage(width_pct),
            Constraint::Percentage((100 - width_pct) / 2),
        ])
        .split(vertical[1])[1]
}
