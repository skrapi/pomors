use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, Gauge},
    Frame, Terminal,
};

struct App {
    time_start: Instant,
    pomodoro_length: Duration
}

impl App {
    fn new() -> App {
        App {
            time_start: Instant::now(),
            pomodoro_length: Duration::from_secs(25 * 60),
        }
    }

    fn on_tick(&mut self) {
        // TODO
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    let app = App::new();
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
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if let KeyCode::Char('q') = key.code {
                    return Ok(());
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([Constraint::Max(5), Constraint::Max(12), Constraint::Max(12)].as_ref())
        .split(f.size());

    let elapsed = Instant::now() - app.time_start;
    let remaining = app.pomodoro_length - elapsed;
    let remaining_min = remaining.as_secs() / 60;
    let remaining_secs = remaining.as_secs() % 60;

    let gauge = Gauge::default()
        .block(Block::default().title(" Pomodoro ").borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Red))
        .percent((elapsed.as_millis() * 100 / app.pomodoro_length.as_millis()) as u16);
    f.render_widget(gauge, chunks[0]);

    let block = Block::default().title(Span::styled(
        format!("Time remaining: {remaining_min} min {remaining_secs} secs"),
        Style::default().fg(Color::Red),
    ));

    f.render_widget(block, chunks[1])
}
