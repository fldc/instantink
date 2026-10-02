# HP Instant Ink CLI Tool

I happened to want this for my workflow so here it is, a command-line tool written in Rust to query HP printers for Instant Ink subscription status, page counts, and ink levels using the printer's XML API endpoint.

## Features

- **Async HTTP requests** for fast performance
- **Real Instant Ink balance** from HP's cloud (pages printed / remaining per billing cycle)
- **Multiple output formats**: Table (default) and JSON
- **Robust XML parsing** with fallback support for different HP printer models
- **Configuration system** with persistent settings in `~/.config/hp-instant-ink/`
- **URL normalization** - just specify hostname/IP, no need for full XML path
- **Verbose logging** for debugging
- **Configurable timeouts**
- **Pretty JSON output** option
- **Cross-platform compatibility**

### Prerequisites

- Rust 1.88+

## Installation

### Method 1: Quick Install (Recommended)

```bash
# Clone and install in one go
git clone https://github.com/your-username/hp-instant-ink-cli.git
cd hp-instant-ink-cli
./install.sh
```

This will:

- Build the project in release mode
- Install the binary to `/usr/local/bin/hp-instant-ink-cli`
- Make it available system-wide

### Method 2: Using Make

```bash
# Install to /usr/local/bin (default)
make install

# Install to custom directory
INSTALL_DIR=/usr/bin make install

# Uninstall
make uninstall
```

### Method 3: Manual Build

```bash
# Build release version
cargo build --release

# Manually copy to your PATH
sudo cp target/release/hp-instant-ink-cli /usr/local/bin/

# Or install via Cargo
cargo install --path .
```

### Uninstallation

```bash
# Using the uninstall script
./uninstall.sh

# Using Make
make uninstall

# Or manually remove
sudo rm /usr/local/bin/hp-instant-ink-cli
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

### Real Instant Ink balance (cloud)

The local XML endpoint only exposes cumulative counters, not the current
billing-cycle allowance. To read the real balance (pages printed, rollover,
overage and pages remaining for the current cycle), the tool uses HP's cloud
API. The required `shell-session-id` cookie is read straight from your
installed browser, so all you need is to be logged in:

1. Log in at <https://portal.hpsmart.com> in your browser.
2. Run:

```bash
hp-instant-ink-cli login     # import the cookie from your browser
hp-instant-ink-cli balance   # print the current cycle
hp-instant-ink-cli balance --format json
```

`balance` imports the cookie automatically if none is stored, and re-imports it
if the stored one has expired. Use `--browser <id>` (e.g. `chromium`, `chrome`,
`brave`, `firefox`) to read a specific browser, or fall back to the manual
cookie with `config --set-session-id <value>`. The session cookie lasts about
90 days; cached access tokens are refreshed automatically.

Example output:

```plaintext
╭──────────────────┬──────────────────────╮
│ Metric           │ Value                │
├──────────────────┼──────────────────────┤
│ Billing Period   │ 06/09/2025 - 05/10/… │
│ Cycle            │ 06/09/2025 - 05/10/… │
│ Plan Pages       │ 50                   │
│ Pages Printed    │ 12                   │
│   from Plan      │ 10                   │
│   from Rollover  │ 2                    │
│ Overage Pages    │ 0                    │
│ Pages Remaining  │ 38                   │
│ Rollover Cap     │ 150                  │
│ Total Price      │ 0,00 €               │
│ Last Updated     │ 2025-09-20 09:14:02  │
╰──────────────────┴──────────────────────╯
```

If HP returns HTTP 401/403 and no fresh cookie can be read from the browser,
log in at <https://portal.hpsmart.com> again and run `hp-instant-ink-cli login`.

## Command Line Options

### Main Commands

- `login`: Import the HP `shell-session-id` cookie from your browser
- `balance`: Fetch the real Instant Ink balance from HP's cloud (see above)
  - `--browser <ID>`: Read the session cookie from a specific browser
  - `--session-id <ID>`: Use a given shell-session-id instead of the browser
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
- `config --set-session-id <ID>`: Set the HP `shell-session-id` cookie for the cloud balance
- `config --show`: Show current configuration
- `config --reset`: Reset configuration to defaults

## Output Examples

### Table format (default)

```plaintext
╭───────────────────────────────────────────────┬──────────────────────────╮
│ Metric                                        │ Value                    │
├───────────────────────────────────────────────┼──────────────────────────┤
│ Subscription Pages Printed (since enrollment) │ 727                      │
│ Total Pages Printed (lifetime)                │ 3489                     │
│ Colour Ink Remaining                          │ 87%                      │
│ Black Ink Remaining                           │ 49%                      │
│ Last Updated                                  │ 2025-07-19 12:54:11 CEST │
╰───────────────────────────────────────────────┴──────────────────────────╯
```

### JSON format

```json
{
  "timestamp": "2025-07-19T10:54:20.850043501Z",
  "total_pages_printed": 3489,
  "subscription_pages_printed": 727,
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
  "printer_url": "http://192.168.1.13/DevMgmt/ProductUsageDyn.xml",
  "timeout_seconds": 30,
  "last_updated": null,
  "shell_session_id": "00000000-0000-0000-0000-000000000000",
  "tenant_id": "…",
  "account_id": "…",
  "access_token": "…",
  "access_token_expires": 1750000000,
  "tenant_access_token": "…",
  "tenant_access_token_expires": 1750000000
}
```

The cloud token fields (`tenant_id`, `account_id`, `*_token*`) are cached
automatically after a successful `balance` run and can be omitted.

## Supported HP Printer Models

This tool works with HP printers that support the Instant Ink service and expose the ProductUsageDyn.xml endpoint. This includes most modern HP inkjet printers with network connectivity.
