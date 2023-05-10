use chrono::{DateTime, Utc};
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    fs, io,
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

struct StatefulList<T> {
    state: ListState,
    items: Vec<T>,
}

impl<T> StatefulList<T> {
    fn with_items(items: Vec<T>) -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
            items,
        }
    }

    fn next(&mut self) {
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
    }

    fn previous(&mut self) {
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
    }

    fn unselect(&mut self) {
        self.state.select(None);
    }

    fn get_selected_mut(&mut self) -> Option<&mut T> {
        if let Some(selected) = self.state.selected() {
            Some(&mut self.items[selected])
        } else {
            None
        }
    }

    fn get_selected(&self) -> Option<&T> {
        if let Some(selected) = self.state.selected() {
            Some(&self.items[selected])
        } else {
            None
        }
    }
}

struct Task {
    name: String,
    is_complete: bool,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
}

impl Task {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            is_complete: false,
            start: None,
            end: None,
        }
    }
}
struct App {
    time_start: Instant,
    pomodoro_length: Duration,
    tasks: StatefulList<Task>,
}

impl App {
    fn new(task_list: Vec<String>, pomodoro_length: Duration) -> App {
        App {
            time_start: Instant::now(),
            pomodoro_length,
            tasks: StatefulList::with_items(
                task_list
                    .iter()
                    .map(|name| Task::new(name.trim()))
                    .collect(),
            ),
        }
    }

    fn on_tick(&mut self) {
        // TODO
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
    let mut app = App::new(args.task_list, Duration::from_secs(args.length * 60));


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
        .margin(3)
        .constraints(
            [
                Constraint::Percentage(20),
                Constraint::Percentage(40),
                Constraint::Percentage(40),
            ]
            .as_ref(),
        )
        .split(f.size());

    let elapsed = Instant::now() - app.time_start;
    let remaining = app.pomodoro_length.saturating_sub(elapsed);
    let remaining_min = remaining.as_secs() / 60;
    let remaining_secs = remaining.as_secs() % 60;

    let gauge = Gauge::default()
        .block(
            Block::default()
                .title(Span::styled(" Pomodoro ", Style::default().fg(Color::Red)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red)),
        )
        .gauge_style(Style::default().fg(Color::Red))
        .percent((elapsed.as_millis() * 100 / app.pomodoro_length.as_millis()).min(100) as u16);
    f.render_widget(gauge, chunks[0]);

    let time_remaining_text = if !remaining.is_zero() {
        format!("Time remaining: {remaining_min} min {remaining_secs} secs")
    } else {
        format!("Task completed")
    };

    let task = Spans::from(Span::styled(
        app.get_current_task_name()
            .unwrap_or(&"No task selected".to_string())
            .clone(),
        Style::default().fg(Color::Red),
    ));

    let time = Spans::from(Span::styled(
        time_remaining_text,
        Style::default().fg(Color::Red),
    ));

    let q_to_quit = Spans::from(Span::styled(
        "Press q to quit",
        Style::default().fg(Color::Red),
    ));

    let paragraph = Paragraph::new(vec![task, time, q_to_quit])
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
            ListItem::new(format!("{} : {:?}", task.name, task.start))
                .style(Style::default().fg(color))
        })
        .collect();

    let items = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Task List ")
                .border_style(Style::default().fg(Color::Red)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::BOLD))
        .highlight_symbol(">> ");

    // We can now render the item list
    f.render_stateful_widget(items, chunks[2], &mut app.tasks.state);
}
