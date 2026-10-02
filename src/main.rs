use anyhow::{Context, Result};
use chrono_tz::Europe::Stockholm;
use clap::{Parser, ValueEnum};
use colored::*;
use log::{debug, error, info};
use tabled::{settings::Style, Table, Tabled};

use hp_instant_ink_cli::{
    format_balance_json_output, format_json_output, import_shell_session_id, Config,
    HPPrinterClient, HPPrinterError, InstantInkBalance, InstantInkClient, PrinterData,
    HP_PORTAL_URL,
};

fn create_table_data(data: &PrinterData) -> Vec<PrinterDataTable> {
    vec![
        PrinterDataTable {
            metric: "Subscription Pages Printed (since enrollment)".to_string(),
            value: data.subscription_pages_printed.to_string(),
        },
        PrinterDataTable {
            metric: "Total Pages Printed (lifetime)".to_string(),
            value: data.total_pages_printed.to_string(),
        },
        PrinterDataTable {
            metric: "Colour Ink Remaining".to_string(),
            value: format!("{}%", data.colour_ink_level),
        },
        PrinterDataTable {
            metric: "Black Ink Remaining".to_string(),
            value: format!("{}%", data.black_ink_level),
        },
        PrinterDataTable {
            metric: "Last Updated".to_string(),
            value: data
                .timestamp
                .with_timezone(&Stockholm)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string(),
        },
    ]
}

#[derive(Tabled)]
struct PrinterDataTable {
    #[tabled(rename = "Metric")]
    metric: String,
    #[tabled(rename = "Value")]
    value: String,
}

#[derive(ValueEnum, Clone, Debug)]
enum OutputFormat {
    Table,
    Json,
}

#[derive(Parser, Debug)]
#[command(
    name = "hp-instant-ink-cli",
    about = "HP Instant Ink CLI Tool - Query HP printer status and ink levels",
    long_about = "This CLI tool queries HP printers locally to obtain page usage and ink levels.\n\nExamples:\n  hp-instant-ink-cli --printer 192.168.1.13\n  hp-instant-ink-cli --printer hp-printer.local --format json\n  hp-instant-ink-cli config --set-printer 192.168.1.13\n  hp-instant-ink-cli config --show"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Command>,

    #[arg(
        short,
        long,
        help = "Printer URL/hostname/IP (will auto-add /DevMgmt/ProductUsageDyn.xml)",
        value_name = "HOST"
    )]
    printer: Option<String>,

    #[arg(short, long, value_enum, help = "Output format")]
    format: Option<OutputFormat>,

    #[arg(short, long, help = "Request timeout in seconds")]
    timeout: Option<u64>,

    #[arg(short, long, help = "Enable verbose logging")]
    verbose: bool,
}

#[derive(Parser, Debug)]
enum Command {
    Config {
        #[arg(long, help = "Show current configuration")]
        show: bool,

        #[arg(long, help = "Set default printer", value_name = "HOST")]
        set_printer: Option<String>,

        #[arg(long, help = "Set default timeout", value_name = "SECONDS")]
        set_timeout: Option<u64>,

        #[arg(long, help = "Set default output format", value_name = "FORMAT")]
        set_format: Option<String>,

        #[arg(
            long,
            help = "Set the HP 'shell-session-id' cookie used for the cloud API",
            value_name = "ID"
        )]
        set_session_id: Option<String>,

        #[arg(long, help = "Reset configuration to defaults")]
        reset: bool,
    },

    #[command(about = "Import the HP shell-session-id cookie from your browser")]
    Login {
        #[arg(
            long,
            help = "Browser to read cookies from (default: try every known browser)",
            value_name = "ID"
        )]
        browser: Option<String>,
    },

    #[command(about = "Fetch the real HP Instant Ink balance from the HP cloud")]
    Balance {
        #[arg(short, long, value_enum, help = "Output format")]
        format: Option<OutputFormat>,

        #[arg(
            long,
            help = "Browser to read the session cookie from (default: try every known browser)",
            value_name = "ID"
        )]
        browser: Option<String>,

        #[arg(
            long,
            help = "Use this shell-session-id instead of reading a browser",
            value_name = "ID"
        )]
        session_id: Option<String>,
    },
}

fn format_table_output(data: &PrinterData) -> Result<String> {
    let table_data = create_table_data(data);
    let mut table = Table::new(table_data);
    table.with(Style::rounded());
    Ok(table.to_string())
}

fn setup_logging(verbose: bool) {
    let log_level = if verbose { "debug" } else { "warn" };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(log_level))
        .format_timestamp_secs()
        .init();
}

fn print_alerts(data: &PrinterData) {
    let mut alerts = Vec::new();

    if data.colour_ink_level <= 20 {
        alerts.push(format!(
            "LOW COLOUR INK: {}% remaining",
            data.colour_ink_level
        ));
    }

    if data.black_ink_level <= 20 {
        alerts.push(format!(
            "LOW BLACK INK: {}% remaining",
            data.black_ink_level
        ));
    }

    if !alerts.is_empty() {
        eprintln!("\n{}", "ALERTS:".red().bold());
        for alert in alerts {
            eprintln!("  {}", alert.yellow());
        }
    }
}

async fn handle_config_command(
    show: bool,
    set_printer: Option<String>,
    set_timeout: Option<u64>,
    set_format: Option<String>,
    set_session_id: Option<String>,
    reset: bool,
) -> Result<()> {
    let mut config = Config::load()?;

    if reset {
        config = Config::default();
        config.save()?;
        println!("{}", "Configuration reset to defaults".green());
        return Ok(());
    }

    if show {
        println!("{}", "Current configuration:".blue().bold());
        let mut safe = config.clone();
        for secret in [
            &mut safe.shell_session_id,
            &mut safe.access_token,
            &mut safe.tenant_access_token,
        ] {
            if secret.is_some() {
                *secret = Some("***".to_string());
            }
        }
        println!("{}", serde_json::to_string_pretty(&safe)?);
        return Ok(());
    }

    let mut changed = false;

    if let Some(printer) = set_printer {
        let normalized = HPPrinterClient::normalize_printer_url(&printer);
        config.printer_url = normalized.clone();
        changed = true;
        println!("{} {}", "Set default printer:".green(), normalized);
    }

    if let Some(timeout) = set_timeout {
        config.timeout_seconds = timeout;
        changed = true;
        println!("{} {}", "Set default timeout:".green(), timeout);
    }

    if let Some(session_id) = set_session_id {
        config.shell_session_id = Some(session_id.trim().to_string());
        clear_cached_tokens(&mut config);
        changed = true;
        println!(
            "{} ***",
            "Set shell-session-id (cached tokens cleared):".green()
        );
    }

    if set_format.is_some() {
        println!("{}", "Note: Format configuration is no longer supported in config. Use --format flag.".yellow());
    }

    if changed {
        config.save()?;
        println!("{}", "Configuration saved".green());
    } else {
        println!("No configuration changes made. Use --help to see available options.");
    }

    Ok(())
}

fn clear_cached_tokens(config: &mut Config) {
    config.tenant_id = None;
    config.account_id = None;
    config.access_token = None;
    config.access_token_expires = None;
    config.tenant_access_token = None;
    config.tenant_access_token_expires = None;
}

fn is_auth_error(err: &anyhow::Error) -> bool {
    err.to_string().contains("HP rejected the shell-session-id")
}

fn emit_balance(data: &InstantInkBalance, format: &OutputFormat) -> Result<()> {
    let output = match format {
        OutputFormat::Json => format_balance_json_output(data)?,
        OutputFormat::Table => format_balance_table_output(data)?,
    };
    println!("{output}");
    Ok(())
}

fn import_session_into_config(
    config: &mut Config,
    browser: Option<&str>,
) -> Result<String> {
    let (session_id, browser_id) = import_shell_session_id(browser)?;
    config.shell_session_id = Some(session_id);
    clear_cached_tokens(config);
    config.save()?;
    Ok(browser_id)
}

async fn handle_login_command(browser: Option<String>) -> Result<()> {
    let mut config = Config::load()?;
    match import_session_into_config(&mut config, browser.as_deref()) {
        Ok(browser_id) => {
            println!("{} {}", "Imported shell-session-id from".green(), browser_id);
            Ok(())
        }
        Err(e) => {
            error!("Could not import the shell-session-id: {e:#}");
            std::process::exit(1);
        }
    }
}

async fn handle_balance_command(
    format: Option<OutputFormat>,
    browser: Option<String>,
    session_id: Option<String>,
) -> Result<()> {
    let mut config = Config::load()?;
    let format = format.unwrap_or(OutputFormat::Table);

    if let Some(id) = session_id {
        config.shell_session_id = Some(id.trim().to_string());
        clear_cached_tokens(&mut config);
        config.save()?;
    }

    if config.shell_session_id.is_none() {
        match import_session_into_config(&mut config, browser.as_deref()) {
            Ok(browser_id) => {
                eprintln!("{} {}", "Using shell-session-id from".green(), browser_id);
            }
            Err(e) => {
                error!("No shell-session-id available: {e:#}");
                error!("Log in at {HP_PORTAL_URL}, then run 'hp-instant-ink-cli login' (or 'config --set-session-id <value>').");
                std::process::exit(1);
            }
        }
    }

    let client = InstantInkClient::new(config.timeout_seconds)
        .context("Failed to create HP Instant Ink client")?;

    match client.get_balance(&mut config).await {
        Ok(data) => {
            emit_balance(&data, &format)?;
            info!("Successfully retrieved Instant Ink balance");
            Ok(())
        }
        Err(e) if is_auth_error(&e) => {
            // The cached cookie expired. Try to pick up a fresh one from the browser.
            match import_session_into_config(&mut config, browser.as_deref()) {
                Ok(browser_id) => {
                    eprintln!(
                        "{} {}",
                        "Session expired, re-imported from".yellow(),
                        browser_id
                    );
                    match client.get_balance(&mut config).await {
                        Ok(data) => {
                            emit_balance(&data, &format)?;
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to fetch the Instant Ink balance: {e:#}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(_) => {
                    error!("The stored shell-session-id expired: {e:#}");
                    error!("Log in at {HP_PORTAL_URL}, then run 'hp-instant-ink-cli login'.");
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            error!("Failed to fetch the Instant Ink balance: {e:#}");
            std::process::exit(1);
        }
    }
}

fn create_balance_table_data(data: &InstantInkBalance) -> Vec<PrinterDataTable> {
    vec![
        PrinterDataTable {
            metric: "Billing Period".to_string(),
            value: data.period.clone().unwrap_or_else(|| "-".to_string()),
        },
        PrinterDataTable {
            metric: "Cycle".to_string(),
            value: format!(
                "{} - {}",
                data.cycle_start.clone().unwrap_or_else(|| "?".to_string()),
                data.cycle_end.clone().unwrap_or_else(|| "?".to_string())
            ),
        },
        PrinterDataTable {
            metric: "Plan Pages".to_string(),
            value: data.plan_pages.to_string(),
        },
        PrinterDataTable {
            metric: "Pages Printed".to_string(),
            value: data.total_pages.to_string(),
        },
        PrinterDataTable {
            metric: "  from Plan".to_string(),
            value: data.regular_pages.to_string(),
        },
        PrinterDataTable {
            metric: "  from Rollover".to_string(),
            value: data.rollover_pages.to_string(),
        },
        PrinterDataTable {
            metric: "Overage Pages".to_string(),
            value: data.additional_pages.to_string(),
        },
        PrinterDataTable {
            metric: "Pages Remaining".to_string(),
            value: data.pages_remaining.to_string(),
        },
        PrinterDataTable {
            metric: "Rollover Cap".to_string(),
            value: data.rollover_cap.to_string(),
        },
        PrinterDataTable {
            metric: "Total Price".to_string(),
            value: data.total_price.clone().unwrap_or_else(|| "-".to_string()),
        },
        PrinterDataTable {
            metric: "Last Updated".to_string(),
            value: data
                .timestamp
                .with_timezone(&Stockholm)
                .format("%Y-%m-%d %H:%M:%S %Z")
                .to_string(),
        },
    ]
}

fn format_balance_table_output(data: &InstantInkBalance) -> Result<String> {
    let table_data = create_balance_table_data(data);
    let mut table = Table::new(table_data);
    table.with(Style::rounded());
    Ok(table.to_string())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    setup_logging(args.verbose);

    if let Some(command) = args.command {
        return match command {
            Command::Config {
                show,
                set_printer,
                set_timeout,
                set_format,
                set_session_id,
                reset,
            } => {
                handle_config_command(
                    show,
                    set_printer,
                    set_timeout,
                    set_format,
                    set_session_id,
                    reset,
                )
                .await
            }
            Command::Login { browser } => handle_login_command(browser).await,
            Command::Balance {
                format,
                browser,
                session_id,
            } => handle_balance_command(format, browser, session_id).await,
        };
    }

    info!("HP Instant Ink CLI Tool starting");
    debug!("Arguments: {args:?}");

    let config = Config::load()?;
    debug!("Loaded config: {config:?}");

    let printer_url = if let Some(printer) = args.printer {
        HPPrinterClient::normalize_printer_url(&printer)
    } else if !config.printer_url.is_empty() {
        config.printer_url
    } else {
        error!("No printer specified. Use --printer <host> or set a default with 'config --set-printer <host>'");
        error!("Example: hp-instant-ink-cli --printer 192.168.1.13");
        error!("         hp-instant-ink-cli config --set-printer 192.168.1.13");
        std::process::exit(1);
    };

    let timeout = args.timeout.unwrap_or(config.timeout_seconds);
    let format = args.format.unwrap_or(OutputFormat::Table);

    info!("Using printer: {printer_url}");
    debug!("Settings - timeout: {timeout}s, format: {format:?}");

    let client = HPPrinterClient::new(printer_url.clone(), timeout)
        .context("Failed to create HP printer client")?;

    match client.get_printer_data().await {
        Ok(data) => {
            let output = match format {
                OutputFormat::Json => format_json_output(&data)?,
                OutputFormat::Table => format_table_output(&data)?,
            };

            println!("{output}");

            print_alerts(&data);

            info!("Successfully retrieved printer data");
        }
        Err(HPPrinterError::NetworkError(e)) => {
            error!("Could not connect to printer at {printer_url}");
            error!("Please check that the printer is online and the URL is correct");
            error!("Network error: {e}");
            std::process::exit(1);
        }
        Err(HPPrinterError::XmlParsingError(e)) => {
            error!("Failed to parse XML from printer");
            error!("Your printer may have a different XML format than expected");
            error!("XML parsing error: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            error!("Unexpected error: {e}");
            std::process::exit(1);
        }
    }

    Ok(())
}
