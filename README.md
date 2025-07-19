# HP Instant Ink CLI Tool

I happened to want this for my workflow so here it is, a command-line tool written in Rust to query HP printers for Instant Ink subscription status, page counts, and ink levels using the printer's XML API endpoint.

## Features

- **Async HTTP requests** for fast performance
- **Multiple output formats**: Table (default) and JSON
- **Robust XML parsing** with fallback support for different HP printer models
- **Configuration system** with persistent settings in `~/.config/hp-instant-ink/`
- **URL normalization** - just specify hostname/IP, no need for full XML path
- **Verbose logging** for debugging
- **Configurable timeouts**
- **Pretty JSON output** option
- **Cross-platform compatibility**

## Installation

### Prerequisites

- Rust 1.70+

### Build from source

```bash
# Build release version
cargo build --release

# The binary will be available at ./target/release/hp-instant-ink-cli
```

## Usage

### Quick Start

```bash
# Set default printer (run once)
hp-instant-ink-cli config --set-printer 192.168.1.13

# Then just run without arguments
hp-instant-ink-cli

# Or specify printer directly
hp-instant-ink-cli --printer 192.168.1.13
```

### Configuration Management

```bash
# Set default printer
hp-instant-ink-cli config --set-printer 192.168.1.13

# Set default format
hp-instant-ink-cli config --set-format json

# Set default timeout
hp-instant-ink-cli config --set-timeout 30

# Show current configuration
hp-instant-ink-cli config --show

# Reset to defaults
hp-instant-ink-cli config --reset
```

### Direct binary usage

```bash
# Basic usage with IP (auto-adds /DevMgmt/ProductUsageDyn.xml)
hp-instant-ink-cli --printer 192.168.1.13

# JSON output
hp-instant-ink-cli --printer printer.local --format json

# With debugging
hp-instant-ink-cli --printer printer.local --verbose
```

## Command Line Options

### Main Commands

- `--printer <HOST>`: Printer hostname/IP (auto-adds /DevMgmt/ProductUsageDyn.xml)
- `--format <FORMAT>`: Output format - `table` (default) or `json`
- `--pretty`: Pretty-print JSON output (only used with `--format json`)
- `--timeout <TIMEOUT>`: Request timeout in seconds (default: 10)
- `--verbose`: Enable verbose logging
- `--help`: Show help information

### Configuration Commands

- `config --set-printer <HOST>`: Set default printer
- `config --set-format <FORMAT>`: Set default output format
- `config --set-timeout <SECONDS>`: Set default timeout
- `config --show`: Show current configuration
- `config --reset`: Reset configuration to defaults

## Output Examples

### Table format (default)

```plaintext
╭──────────────────────┬──────────────────────────╮
│ Metric               │ Value                    │
├──────────────────────┼──────────────────────────┤
│ Subscription Pages   │ 727                      │
│ Total Pages          │ 3489                     │
│ Colour Ink Remaining │ 87%                      │
│ Black Ink Remaining  │ 49%                      │
│ Last Updated         │ 2025-07-19 12:54:11 CEST │
╰──────────────────────┴──────────────────────────╯
```

### JSON format

```json
{
  "timestamp": "2025-07-19T10:54:20.850043501Z",
  "pages_printed": 3489,
  "subscription_impressions": 727,
  "colour_ink_level": 87,
  "black_ink_level": 49
}
```

## Finding Your Printer

Your HP printer's IP address or hostname is all you need. The tool automatically adds the XML endpoint path.

To find your printer:

1. Check your printer's network settings display
2. Use your router's admin interface  
3. Use network discovery tools like `nmap`
4. Look for printers on your network: `nmap -p 80 192.168.1.0/24`

Examples:

- `192.168.1.100` (tool converts to `http://192.168.1.100/DevMgmt/ProductUsageDyn.xml`)
- `hp-printer.local` (tool converts to `http://hp-printer.local/DevMgmt/ProductUsageDyn.xml`)
- `http://192.168.1.100` (tool adds `/DevMgmt/ProductUsageDyn.xml`)

## Configuration

The tool stores configuration in `~/.config/hp-instant-ink/config.json`:

```json
{
  "default_printer": "http://192.168.1.13/DevMgmt/ProductUsageDyn.xml",
  "timeout": 10,
  "format": "table",
  "pretty_json": false
}
```

## Supported HP Printer Models

This tool works with HP printers that support the Instant Ink service and expose the ProductUsageDyn.xml endpoint. This includes most modern HP inkjet printers with network connectivity.
