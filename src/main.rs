use anyhow::{Context, Result};
use chrono_tz::Europe::Stockholm;
use clap::{Parser, ValueEnum};
use colored::*;
use log::{debug, error, info};
use tabled::{settings::Style, Table, Tabled};

use hp_instant_ink_cli::{
    format_json_output, Config, HPPrinterClient, HPPrinterError, PrinterData,
};

fn create_table_data(data: &PrinterData) -> Vec<PrinterDataTable> {
    vec![
        PrinterDataTable {
            metric: "Subscription Pages".to_string(),
            value: data.subscription_impressions.to_string(),
        },
        PrinterDataTable {
            metric: "Total Pages".to_string(),
            value: data.pages_printed.to_string(),
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

        #[arg(long, help = "Reset configuration to defaults")]
        reset: bool,
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

async fn handle_config_command(config_args: Command) -> Result<()> {
    match config_args {
        Command::Config {
            show,
            set_printer,
            set_timeout,
            set_format,
            reset,
        } => {
            let mut config = Config::load()?;

            if reset {
                config = Config::default();
                config.save()?;
                println!("{}", "Configuration reset to defaults".green());
                return Ok(());
            }

            if show {
                println!("{}", "Current configuration:".blue().bold());
                let config_json = serde_json::to_string_pretty(&config)?;
                println!("{config_json}");
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
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    setup_logging(args.verbose);

    if let Some(command) = args.command {
        return handle_config_command(command).await;
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
