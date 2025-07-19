use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use log::{debug, error, warn};
use quick_xml::de::from_str;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub printer_url: String,
    pub timeout_seconds: u64,
    pub last_updated: Option<DateTime<Utc>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            printer_url: String::new(),
            timeout_seconds: 30,
            last_updated: None,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path()?;

        if config_path.exists() {
            let content = fs::read_to_string(&config_path).context("Failed to read config file")?;
            let config: Config =
                serde_json::from_str(&content).context("Failed to parse config file")?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).context("Failed to create config directory")?;
        }

        let content = serde_json::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_path, content).context("Failed to write config file")?;

        Ok(())
    }

    fn get_config_path() -> Result<PathBuf> {
        if let Some(config_dir) = dirs::config_dir() {
            let hp_config_dir = config_dir.join("hp-instant-ink");
            Ok(hp_config_dir.join("config.json"))
        } else {
            anyhow::bail!("Could not determine config directory")
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HPPrinterError {
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("XML parsing error: {0}")]
    XmlParsingError(quick_xml::DeError),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct PrinterData {
    pub timestamp: DateTime<Utc>,
    pub pages_printed: u32,
    pub subscription_impressions: u32,
    pub colour_ink_level: u32,
    pub black_ink_level: u32,
}

impl PrinterData {
    pub fn new(
        pages_printed: u32,
        subscription_impressions: u32,
        colour_ink_level: u32,
        black_ink_level: u32,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            pages_printed,
            subscription_impressions,
            colour_ink_level,
            black_ink_level,
        }
    }
}

pub fn format_json_output(data: &PrinterData) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(data)
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ConsumableSubunit {
    #[serde(rename = "Consumable")]
    consumables: Vec<Consumable>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Consumable {
    #[serde(rename = "MarkerColor")]
    marker_color: String,
    #[serde(rename = "ConsumableLabelCode")]
    label_code: Option<String>,
    #[serde(rename = "ConsumableRawPercentageLevelRemaining")]
    percentage_remaining: Option<String>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct ProductUsageDyn {
    #[serde(rename = "PrinterSubunit")]
    printer_subunit: PrinterSubunit,
    #[serde(rename = "ConsumableSubunit")]
    consumable_subunit: ConsumableSubunit,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct PrinterSubunit {
    #[serde(rename = "SubscriptionImpressions")]
    subscription_impressions: Option<String>,
    #[serde(rename = "TotalImpressions")]
    total_impressions: Option<TotalImpressions>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum TotalImpressions {
    WithAttributes {
        #[serde(rename = "$text")]
        text: Option<String>,
        #[serde(rename = "#text")]
        content: Option<String>,
    },
    Direct(String),
    Nested {
        #[serde(rename = "#text")]
        text: Option<String>,
        #[serde(rename = "text")]
        dd_text: Option<String>,
    },
}

pub struct HPPrinterClient {
    client: Client,
    printer_url: String,
    #[allow(dead_code)]
    timeout: Duration,
}

impl HPPrinterClient {
    pub fn new(printer_url: String, timeout_seconds: u64) -> Result<Self> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/70.0.3538.77 Safari/537.36")
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self {
            client,
            printer_url,
            timeout: Duration::from_secs(timeout_seconds),
        })
    }

    pub fn normalize_printer_url(input: &str) -> String {
        if input.contains("/DevMgmt/ProductUsageDyn.xml") {
            input.to_string()
        } else if input.starts_with("http://") || input.starts_with("https://") {
            let base_url = input.trim_end_matches('/');
            format!("{base_url}/DevMgmt/ProductUsageDyn.xml")
        } else {
            format!("http://{input}/DevMgmt/ProductUsageDyn.xml")
        }
    }

    pub async fn get_printer_data(&self) -> Result<PrinterData, HPPrinterError> {
        debug!("Fetching data from: {}", self.printer_url);

        let response = self.client.get(&self.printer_url).send().await?;

        let xml_content = response.text().await?;
        debug!("Received XML content length: {} bytes", xml_content.len());

        let pages_printed = self.extract_pages_from_xml(&xml_content);

        let subscription_impressions = self.extract_subscription_impressions(&xml_content);

        let parsed: ProductUsageDyn = from_str(&xml_content).map_err(|e| {
            error!("Failed to parse XML: {e}");
            debug!("XML content: {xml_content}");
            HPPrinterError::XmlParsingError(e)
        })?;

        let mut colour_ink = 0u32;
        let mut black_ink = 0u32;

        for consumable in &parsed.consumable_subunit.consumables {
            if let Some(percentage) = &consumable.percentage_remaining {
                match consumable.marker_color.as_str() {
                    "CyanMagentaYellow" => {
                        colour_ink = percentage.parse::<u32>().unwrap_or_else(|_| {
                            warn!("Could not parse colour ink percentage: {percentage}");
                            0
                        });
                    }
                    "Black" => {
                        black_ink = percentage.parse::<u32>().unwrap_or_else(|_| {
                            warn!("Could not parse black ink percentage: {percentage}");
                            0
                        });
                    }
                    _ => debug!("Unknown marker color: {}", consumable.marker_color),
                }
            }
        }

        Ok(PrinterData::new(
            pages_printed,
            subscription_impressions,
            colour_ink,
            black_ink,
        ))
    }

    fn extract_pages_from_xml(&self, xml_content: &str) -> u32 {
        let re = Regex::new(
            r#"<[^:]*:?TotalImpressions[^>]*PEID="[^"]*"[^>]*>(\d+)</[^:]*:?TotalImpressions>"#,
        )
        .unwrap();
        if let Some(captures) = re.captures(xml_content) {
            if let Some(value) = captures.get(1) {
                if let Ok(pages) = value.as_str().parse::<u32>() {
                    debug!("Found TotalImpressions with PEID: {pages}");
                    return pages;
                }
            }
        }

        let re_fallback = Regex::new(r"<pudyn:PrinterSubunit>.*?<[^:]*:?TotalImpressions[^>]*>(\d+)</[^:]*:?TotalImpressions>").unwrap();
        if let Some(captures) = re_fallback.captures(xml_content) {
            if let Some(value) = captures.get(1) {
                if let Ok(pages) = value.as_str().parse::<u32>() {
                    debug!("Found fallback TotalImpressions: {pages}");
                    return pages;
                }
            }
        }

        warn!("Could not extract pages printed from XML");
        0
    }

    fn extract_subscription_impressions(&self, xml_content: &str) -> u32 {
        let re = Regex::new(
            r"<[^:]*:?SubscriptionImpressions[^>]*>(\d+)</[^:]*:?SubscriptionImpressions>",
        )
        .unwrap();
        if let Some(captures) = re.captures(xml_content) {
            if let Some(value) = captures.get(1) {
                if let Ok(impressions) = value.as_str().parse::<u32>() {
                    debug!("Found SubscriptionImpressions: {impressions}");
                    return impressions;
                }
            }
        }

        warn!("Could not extract subscription impressions from XML");
        0
    }
}
