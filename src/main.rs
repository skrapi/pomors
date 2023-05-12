use chrono::{DateTime, Utc};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use rusty_audio::Audio;
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs, io, thread,
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};

struct Task {
    name: String,
    is_complete: bool,
    work_periods: Vec<(DateTime<Utc>, DateTime<Utc>)>,
}

impl Task {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            is_complete: false,
            work_periods: Vec::new(),
        }
    }

    fn activate(&mut self) {
        let time = Utc::now();
        self.work_periods.push((time.clone(), time))
    }

    fn deactivate(&mut self) {
        if let Some(work_period) = self.work_periods.last_mut() {
            if work_period.0 != work_period.1 {
                return;
            }

            work_period.1 = Utc::now()
        }
    }

    fn task_total_duration(&self) -> chrono::Duration {
        self.work_periods
            .iter()
            .fold(chrono::Duration::zero(), |acc, work_period| acc + (work_period.1 - work_period.0))
    }
}

struct StatefulList {
    state: ListState,
    items: Vec<Task>,
}

impl StatefulList {
    fn with_items(items: Vec<Task>) -> StatefulList {
        StatefulList {
            state: ListState::default(),
            items,
        }
    }

    fn next(&mut self) {
        if let Some(selected_task) = self.get_selected_mut() {
            selected_task.deactivate()
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
        if let Some(selected_task) = self.get_selected_mut() {
            selected_task.activate()
        }
    }

    fn previous(&mut self) {
        if let Some(selected_task) = self.get_selected_mut() {
            selected_task.deactivate()
        }

        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));

        if let Some(selected_task) = self.get_selected_mut() {
            selected_task.activate()
        }
    }

    fn unselect(&mut self) {
        if let Some(selected_task) = self.get_selected_mut() {
            selected_task.deactivate()
        }
        self.state.select(None);
    }

    fn get_selected_mut(&mut self) -> Option<&mut Task> {
        if let Some(selected) = self.state.selected() {
            Some(&mut self.items[selected])
        } else {
            None
        }
    }

    fn get_selected(&self) -> Option<&Task> {
        if let Some(selected) = self.state.selected() {
            Some(&self.items[selected])
        } else {
            None
        }
    }
}

struct Period {
    start: Instant,
    length: Duration,
}

enum AppState {
    Working,
    TakingABreak,
}
struct App {
    pomodoro_length: Duration,
    break_length: Duration,
    tasks: StatefulList,
    state: AppState,
    start_of_period: Instant,
}

impl App {
    fn new(task_list: Vec<String>, pomodoro_length: Duration, break_length: Duration) -> App {
        App {
            state: AppState::Working,
            pomodoro_length,
            break_length,
            start_of_period: Instant::now(),
            tasks: StatefulList::with_items(
                task_list
                    .iter()
                    .map(|name| Task::new(name.trim()))
                    .collect(),
            ),
        }
    }

    fn period_length(&self) -> Duration {
        match self.state {
            AppState::Working => self.pomodoro_length,
            AppState::TakingABreak => self.break_length,
        }
    }

    fn on_tick(&mut self) {
        if self.elapsed() > self.period_length() {
            match self.state {
                AppState::Working => self.state = AppState::TakingABreak,
                AppState::TakingABreak => self.state = AppState::Working,
            }

            let mut audio = Audio::new();
            audio.add("startup", "creepy-church-bell-33827.mp3"); // Load the sound, give it a name
            audio.play("startup"); // Execution continues while playback occurs in another thread.
            thread::sleep(Duration::from_secs(5));

            self.start_of_period = Instant::now();
        }
    }

    fn elapsed(&self) -> Duration {
        Instant::now() - self.start_of_period
    }

    fn remaining(&self) -> Duration {
        self.period_length().saturating_sub(self.elapsed())
    }

    fn set_current(&mut self) {
        if let Some(selected_task) = self.tasks.get_selected_mut() {
            selected_task.is_complete = true;
        }
    }

    fn reset_current(&mut self) {
        if let Some(selected_task) = self.tasks.get_selected_mut() {
            selected_task.is_complete = false;
        }
    }

    fn toggle_current_task(&mut self) {
        if let Some(selected_task) = self.tasks.get_selected_mut() {
            selected_task.is_complete = !selected_task.is_complete;
        }
    }

    fn get_current_task_name(&self) -> Option<&String> {
        if let Some(selected_task) = self.tasks.get_selected() {
            Some(&selected_task.name)
        } else {
            None
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    pomodoro_length: Duration,
    break_length: Duration,
}

const DEFAULT_CONFIG: Config = Config {
    pomodoro_length: Duration::from_secs(25 * 60),
    break_length: Duration::from_secs(5 * 60),
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// List of tasks
    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ',')]
    task_list: Vec<String>,

    /// Length of one pomodoro [min]
    #[arg(short, long, default_value_t = 25)]
    length: u64,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Get args
    let args = Args::parse();

    let home_dir = home::home_dir().expect("Unable to find Home directory.");

    // Get config
    let pomors_dir = home_dir.join(".config/pomors");

    match fs::read_dir(&pomors_dir) {
        Ok(_) => {
            if let Ok(config_file) = fs::read_to_string(pomors_dir.join("config.json")) {
                let _config = serde_json::from_str::<Config>(&config_file);
            }
        }
        Err(e) => match e.kind() {
            io::ErrorKind::NotFound => {
                fs::create_dir_all(&pomors_dir).expect("Failed to created pomors directory.");
                fs::write(
                    pomors_dir.join("config.json"),
                    serde_json::to_string_pretty(&DEFAULT_CONFIG)
                        .expect("The default config is not serializable."),
                )
                .expect("Failed to write config.json.");
            }
            _ => panic!("Error reading .config/pomors: {e}"),
        },
    };

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    let mut app = App::new(
        args.task_list,
        Duration::from_secs(args.length * 60),
        Duration::from_secs(5 * 60),
    );

    // Select the first task
    app.tasks.next();
    let res = run_app(&mut terminal, app, tick_rate);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down => app.tasks.next(),
                    KeyCode::Up => app.tasks.previous(),
                    KeyCode::Enter => app.toggle_current_task(),
                    _ => {}
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
                Constraint::Ratio(1, 3),
            ]
            .as_ref(),
        )
        .split(f.size());

    let remaining_min = app.remaining().as_secs() / 60;
    let remaining_secs = app.remaining().as_secs() % 60;

    let (action, color) = match app.state {
        AppState::Working => ("Task", Color::Red),
        AppState::TakingABreak => ("Break", Color::Green),
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(Span::styled(" Pomodoro ", Style::default().fg(color)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color)),
        )
        .gauge_style(Style::default().fg(color))
        .percent(
            (app.elapsed().as_millis() * 100 / app.period_length().as_millis()).min(100) as u16,
        );
    f.render_widget(gauge, chunks[0]);

    let time_remaining_text = if !app.remaining().is_zero() {
        format!("{remaining_min} min {remaining_secs} secs")
    } else {
        format!("{action} completed")
    };

    let time = Spans::from(Span::styled(
        time_remaining_text,
        Style::default().fg(color),
    ));

    let q_to_quit = Spans::from(Span::styled("Press q to quit", Style::default().fg(color)));

    let paragraph = Paragraph::new(vec![time, q_to_quit])
        .style(Style::default())
        .block(Block::default());

    f.render_widget(paragraph, chunks[1]);

    let items: Vec<ListItem> = app
        .tasks
        .items
        .iter()
        .map(|task| {
            let color = if task.is_complete {
                Color::Green
            } else {
                Color::Red
            };
            ListItem::new(format!("{} : {:?}: {}", task.name, task.task_total_duration(), task.work_periods.len()))
                .style(Style::default().fg(color))
        })
        .collect();

    let items = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Task List ")
                .border_style(Style::default().fg(color)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    // We can now render the item list
    f.render_stateful_widget(items, chunks[2], &mut app.tasks.state);
}
