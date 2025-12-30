mod app;
mod cli;
mod config;
mod iceberg;
mod ui;

use anyhow::Result;
use app::App;
use cli::Cli;
use config::Config;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // Parse CLI arguments
    let args = Cli::parse_args();

    // Load configuration
    let config = Config::load(args.config.clone(), args.catalogs.clone())?;

    // If there's a subcommand, execute it and exit
    if let Some(command) = args.command {
        return execute_command(command, config).await;
    }

    // Otherwise, start the TUI
    run_tui(config).await
}

async fn execute_command(command: cli::Commands, config: Config) -> Result<()> {
    match command {
        cli::Commands::List { catalog } => {
            println!("Listing catalogs and tables...");
            if let Some(catalog_name) = catalog {
                println!("Catalog: {}", catalog_name);
                // TODO: Implement listing
            } else {
                println!("Available catalogs:");
                for (name, cfg) in &config.catalogs {
                    println!("  - {} ({})", name, cfg.catalog_type);
                }
            }
            Ok(())
        }
        cli::Commands::Query { query, format } => {
            println!("Executing query: {}", query);
            println!("Output format: {}", format);
            // TODO: Implement query execution
            Ok(())
        }
    }
}

async fn run_tui(config: Config) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(config);
    app.initialize().await?;

    // Run the main loop
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

async fn run_app<B>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<()>
where
    B: ratatui::backend::Backend,
    B::Error: Send + Sync + 'static,
{
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Handle Ctrl+C to quit
                if key.code == KeyCode::Char('c')
                    && key.modifiers.contains(event::KeyModifiers::CONTROL)
                {
                    app.should_quit = true;
                }

                app.handle_key_event(key);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
