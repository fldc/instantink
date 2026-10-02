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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shell_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_token_expires: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_access_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant_access_token_expires: Option<i64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            printer_url: String::new(),
            timeout_seconds: 30,
            last_updated: None,
            shell_session_id: None,
            tenant_id: None,
            account_id: None,
            access_token: None,
            access_token_expires: None,
            tenant_access_token: None,
            tenant_access_token_expires: None,
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
    pub total_pages_printed: u32,
    pub subscription_pages_printed: u32,
    pub colour_ink_level: u32,
    pub black_ink_level: u32,
}

impl PrinterData {
    pub fn new(
        total_pages_printed: u32,
        subscription_pages_printed: u32,
        colour_ink_level: u32,
        black_ink_level: u32,
    ) -> Self {
        Self {
            timestamp: Utc::now(),
            total_pages_printed,
            subscription_pages_printed,
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

        let total_pages_printed = self.extract_total_pages_printed(&xml_content);

        let subscription_pages_printed = self.extract_subscription_pages_printed(&xml_content);

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
            total_pages_printed,
            subscription_pages_printed,
            colour_ink,
            black_ink,
        ))
    }

    fn extract_total_pages_printed(&self, xml_content: &str) -> u32 {
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

        let re_fallback = Regex::new(
            r"(?s)<pudyn:PrinterSubunit>.*?<[^:]*:?TotalImpressions[^>]*>(\d+)</[^:]*:?TotalImpressions>",
        )
        .unwrap();
        if let Some(captures) = re_fallback.captures(xml_content) {
            if let Some(value) = captures.get(1) {
                if let Ok(pages) = value.as_str().parse::<u32>() {
                    debug!("Found fallback TotalImpressions: {pages}");
                    return pages;
                }
            }
        }

        warn!("Could not extract total pages printed from XML");
        0
    }

    fn extract_subscription_pages_printed(&self, xml_content: &str) -> u32 {
        let re = Regex::new(
            r"<[^:]*:?SubscriptionImpressions[^>]*>(\d+)</[^:]*:?SubscriptionImpressions>",
        )
        .unwrap();
        if let Some(captures) = re.captures(xml_content) {
            if let Some(value) = captures.get(1) {
                if let Ok(pages) = value.as_str().parse::<u32>() {
                    debug!("Found SubscriptionImpressions: {pages}");
                    return pages;
                }
            }
        }

        warn!("Could not extract subscription pages printed from XML");
        0
    }
}

pub const HP_PORTAL_URL: &str = "https://portal.hpsmart.com";
pub const HP_USERMGMT_URL: &str = "https://us1.api.ws-hp.com";
pub const HP_INSTANTINK_URL: &str = "https://instantink.hpconnected.com";
const HP_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstantInkBalance {
    pub timestamp: DateTime<Utc>,
    pub period: Option<String>,
    pub cycle_start: Option<String>,
    pub cycle_end: Option<String>,
    pub plan_pages: u32,
    pub rollover_cap: u32,
    pub regular_pages: u32,
    pub rollover_pages: u32,
    pub initial_rollover_pages: u32,
    pub additional_pages: u32,
    pub total_pages: u32,
    pub pages_remaining: i64,
    pub total_price: Option<String>,
}

pub fn format_balance_json_output(data: &InstantInkBalance) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(data)
}

fn json_u32(value: &serde_json::Value) -> u32 {
    value
        .as_u64()
        .map(|n| n as u32)
        .or_else(|| value.as_str().and_then(|s| s.parse::<u32>().ok()))
        .unwrap_or(0)
}

fn decode_jwt_stratus_id(token: &str) -> Result<String> {
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    use base64::Engine;

    let payload = token
        .split('.')
        .nth(1)
        .context("Access token is not a JWT")?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| URL_SAFE.decode(payload))
        .context("Failed to base64-decode JWT payload")?;
    let json: serde_json::Value =
        serde_json::from_slice(&bytes).context("Failed to parse JWT payload as JSON")?;

    json["stratus_id"]
        .as_str()
        .map(String::from)
        .context("No stratus_id found in JWT payload")
}

pub const HP_COOKIE_NAME: &str = "shell-session-id";

pub const KNOWN_BROWSERS: &[&str] = &[
    "chromium", "chrome", "brave", "edge", "vivaldi", "opera", "arc", "zen", "librewolf",
];

/// Reads the HP `shell-session-id` cookie straight from an installed browser's
/// cookie store. Returns `(cookie_value, browser_id)`.
///
/// Pass `Some(id)` to read a single browser, or `None` to try every known
/// browser in order.
pub fn import_shell_session_id(browser: Option<&str>) -> Result<(String, String)> {
    let candidates: Vec<String> = match browser {
        Some(id) => vec![id.to_string()],
        None => KNOWN_BROWSERS.iter().map(|id| id.to_string()).collect(),
    };

    let mut read_any_store = false;
    let mut errors: Vec<String> = Vec::new();

    for id in candidates {
        let request = rookie_cookies::ExtractRequest::browser(id.clone()).include_session();
        match rookie_cookies::extract(request) {
            Ok(cookies) => {
                read_any_store = true;
                if let Some(cookie) = cookies.iter().find(|cookie| cookie.name == HP_COOKIE_NAME) {
                    if !cookie.value.is_empty() {
                        return Ok((cookie.value.clone(), id));
                    }
                }
            }
            Err(err) => errors.push(format!("{id}: {err}")),
        }
    }

    if read_any_store {
        anyhow::bail!(
            "No '{HP_COOKIE_NAME}' cookie found in any browser. Log in at {HP_PORTAL_URL} first, then retry."
        );
    }

    anyhow::bail!(
        "Could not read cookies from any supported browser ({}). Use 'config --set-session-id <value>' instead.",
        errors.join("; ")
    )
}

pub struct InstantInkClient {
    client: Client,
}

impl InstantInkClient {
    pub fn new(timeout_seconds: u64) -> Result<Self> {
        let client = Client::builder()
            .user_agent(HP_USER_AGENT)
            .timeout(Duration::from_secs(timeout_seconds))
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { client })
    }

    async fn post_token(
        &self,
        session_id: &str,
        tenant_type: &str,
        organization: Option<&str>,
    ) -> Result<serde_json::Value> {
        let body = match organization {
            Some(org) => serde_json::json!({
                "tenantType": tenant_type,
                "shellTenantsData": { "organization": org }
            }),
            None => serde_json::json!({
                "tenantType": tenant_type,
                "shellTenantsData": {}
            }),
        };

        let response = self
            .client
            .post(format!("{HP_PORTAL_URL}/api/session/v3/token"))
            .header("Accept", "application/json")
            .header("Origin", HP_PORTAL_URL)
            .header("Referer", format!("{HP_PORTAL_URL}/"))
            .header("Cookie", format!("shell-session-id={session_id}"))
            .json(&body)
            .send()
            .await
            .context("Failed to contact the HP login endpoint")?;

        let status = response.status();
        let text = response
            .text()
            .await
            .context("Failed to read the HP login response")?;

        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN
        {
            anyhow::bail!(
                "HP rejected the shell-session-id (HTTP {status}). Log in at {HP_PORTAL_URL} again and copy a fresh 'shell-session-id' cookie."
            );
        }
        if !status.is_success() {
            anyhow::bail!("HP token request failed (HTTP {status}): {text}");
        }

        serde_json::from_str(&text).context("Failed to parse the HP token response")
    }

    async fn get_tenant_id(&self, access_token: &str, stratus_id: &str) -> Result<String> {
        let url = format!(
            "{HP_USERMGMT_URL}/v3/usermgtsvc/usertenantdetails?userResourceId={stratus_id}&state=Active&tenantType=Personal"
        );

        let response = self
            .client
            .get(&url)
            .header("Accept", "application/json")
            .header("Origin", HP_PORTAL_URL)
            .header("Referer", format!("{HP_PORTAL_URL}/"))
            .header("Authorization", format!("Bearer {access_token}"))
            .send()
            .await
            .context("Failed to contact the HP user management service")?;

        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("HP usertenantdetails request failed (HTTP {status}): {text}");
        }

        let json: serde_json::Value = serde_json::from_str(&text)
            .context("Failed to parse the HP usertenantdetails response")?;
        json["resourceList"][0]["tenantResourceId"]
            .as_str()
            .map(String::from)
            .context("No tenantResourceId found in the HP response")
    }

    async fn get_instantink_json(
        &self,
        url: &str,
        tenant_token: &str,
    ) -> Result<serde_json::Value> {
        let response = self
            .client
            .get(url)
            .header("Accept", "application/json")
            .header("Origin", HP_PORTAL_URL)
            .header("Referer", format!("{HP_PORTAL_URL}/"))
            .header("Authorization", format!("Bearer {tenant_token}"))
            .send()
            .await
            .context("Failed to contact the HP Instant Ink service")?;

        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            anyhow::bail!("HP Instant Ink request failed (HTTP {status}): {text}");
        }

        serde_json::from_str(&text).context("Failed to parse the HP Instant Ink response")
    }

    async fn refresh_tokens(&self, config: &mut Config) -> Result<()> {
        let mut session_id = config.shell_session_id.clone().context(
            "No shell-session-id configured. Run: hp-instant-ink-cli config --set-session-id <value>",
        )?;

        let json = self.post_token(&session_id, "orgless", None).await?;
        let access_token = json["shellTenantlessData"]["token"]
            .as_str()
            .context("No access token in the HP response (the shell-session-id may be invalid)")?
            .to_string();
        let access_expire_in = json["shellStratusAccessTokenExpireIn"]
            .as_i64()
            .unwrap_or(3599);
        config.access_token = Some(access_token.clone());
        config.access_token_expires = Some(Utc::now().timestamp() + access_expire_in);

        if let Some(new_session_id) = json["shellSessionId"].as_str() {
            session_id = new_session_id.to_string();
            config.shell_session_id = Some(session_id.clone());
        }

        if config.tenant_id.is_none() {
            let stratus_id = decode_jwt_stratus_id(&access_token)?;
            config.tenant_id = Some(self.get_tenant_id(&access_token, &stratus_id).await?);
        }
        let tenant_id = config.tenant_id.clone().unwrap();

        let json2 = self
            .post_token(&session_id, "organization", Some(&tenant_id))
            .await?;
        let tenant_token = json2["shellTenantData"]["token"]
            .as_str()
            .context("No tenant access token in the HP response")?
            .to_string();
        let tenant_expire_in = json2["shellStratusAccessTokenExpireIn"]
            .as_i64()
            .unwrap_or(3599);
        config.tenant_access_token = Some(tenant_token.clone());
        config.tenant_access_token_expires = Some(Utc::now().timestamp() + tenant_expire_in);

        config.account_id = Some(self.get_account_id(&tenant_token).await?);

        config.save()?;
        Ok(())
    }

    async fn ensure_tokens(&self, config: &mut Config) -> Result<()> {
        let now = Utc::now().timestamp();
        let access_ok = config.access_token.is_some()
            && config
                .access_token_expires
                .is_some_and(|expires| now < expires - 60);
        let tenant_ok = config.tenant_access_token.is_some()
            && config
                .tenant_access_token_expires
                .is_some_and(|expires| now < expires - 60);

        if access_ok && tenant_ok && config.account_id.is_some() {
            return Ok(());
        }

        self.refresh_tokens(config).await
    }

    async fn get_account_id(&self, tenant_token: &str) -> Result<String> {
        let json = self
            .get_instantink_json(&format!("{HP_INSTANTINK_URL}/api/dashboard/v1/ucde"), tenant_token)
            .await?;
        json["account_identifier"]
            .as_str()
            .map(String::from)
            .context("No account_identifier found in the HP response")
    }

    pub async fn get_balance(&self, config: &mut Config) -> Result<InstantInkBalance> {
        self.ensure_tokens(config).await?;

        let account_id = config
            .account_id
            .clone()
            .context("No Instant Ink account id available")?;
        let tenant_token = config
            .tenant_access_token
            .clone()
            .context("No Instant Ink tenant token available")?;

        let dashboard = self
            .get_instantink_json(
                &format!(
                    "{HP_INSTANTINK_URL}/api/dashboard/v1/subscription/{account_id}?flow=dashboard"
                ),
                &tenant_token,
            )
            .await?;

        let cycle = &dashboard["billingCycleSelectionList"][0];
        let cycle_id = cycle["id"]
            .as_i64()
            .map(|id| id.to_string())
            .or_else(|| cycle["id"].as_str().map(String::from))
            .context("No billing cycle found in the HP response")?;
        let period = cycle["label"].as_str().map(String::from);

        let billing_cycle = self
            .get_instantink_json(
                &format!(
                    "{HP_INSTANTINK_URL}/api/dashboard/v1/subscription/{account_id}/billing_cycle/{cycle_id}"
                ),
                &tenant_token,
            )
            .await?;

        let plan_pages = json_u32(&billing_cycle["plan"]["pages"]);
        let rollover_cap = json_u32(&billing_cycle["plan"]["rollover_cap"]);
        let regular_pages = json_u32(&billing_cycle["totals"]["regular_pages"]);
        let rollover_pages = json_u32(&billing_cycle["totals"]["rollover_pages"]);
        let initial_rollover_pages = json_u32(&billing_cycle["totals"]["initial_rollover_pages"]);
        let additional_pages = json_u32(&billing_cycle["totals"]["additional_pages"]);

        let total_pages = {
            let reported = json_u32(&billing_cycle["totals"]["total_pages"]);
            if reported == 0 {
                regular_pages + rollover_pages + additional_pages
            } else {
                reported
            }
        };

        let pages_remaining = plan_pages as i64 - regular_pages as i64 + initial_rollover_pages as i64
            - rollover_pages as i64;

        let total_price = billing_cycle["totals"]["total_price"]
            .as_str()
            .map(String::from)
            .or_else(|| {
                billing_cycle["totals"]["total_price"]
                    .as_f64()
                    .map(|price| format!("{price:.2}"))
            });

        Ok(InstantInkBalance {
            timestamp: Utc::now(),
            period,
            cycle_start: dashboard["currentCycleStartDate"].as_str().map(String::from),
            cycle_end: dashboard["currentCycleEndDate"].as_str().map(String::from),
            plan_pages,
            rollover_cap,
            regular_pages,
            rollover_pages,
            initial_rollover_pages,
            additional_pages,
            total_pages,
            pages_remaining,
            total_price,
        })
    }
}
