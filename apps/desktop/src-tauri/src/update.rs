use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use reqwest::{redirect::Policy, Client, StatusCode, Url};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tauri::{Manager, Resource, ResourceId, WebviewWindow};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

const FEED_LIMIT: usize = 64 * 1024;
const PACKAGE_LIMIT: usize = 128 * 1024 * 1024;
const RELEASES_PATH: &str = "/MohamedSadat/cashgear-fotoeditor/releases/";
const ACCEPTANCE_ID: &str = "com.cashgear.fotoeditor.acceptance";

type Result<T> = std::result::Result<T, String>;

#[derive(Deserialize)]
struct UpdateConfig {
    pubkey: String,
    endpoints: Vec<Url>,
}

#[derive(Deserialize)]
struct SignedManifest {
    manifest: String,
    signature: String,
}

#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    version: Version,
    bytes: usize,
}

#[derive(Serialize)]
pub struct UpdateCheck {
    current: String,
    available: Option<AvailableUpdate>,
}

#[derive(Serialize)]
pub struct AvailableUpdate {
    id: ResourceId,
    version: String,
    bytes: usize,
}

struct Candidate {
    update: Update,
    bytes: usize,
}
impl Resource for Candidate {}

struct Downloaded(Update, Vec<u8>);
impl Resource for Downloaded {}

fn update_error(error: impl std::fmt::Display) -> String {
    format!("Windows update failed: {error}")
}

fn decode(value: &str) -> Result<String> {
    String::from_utf8(STANDARD.decode(value.trim()).map_err(update_error)?).map_err(update_error)
}

fn authenticate(bytes: &[u8], pubkey: &str) -> Result<(serde_json::Value, Manifest)> {
    let envelope: SignedManifest = serde_json::from_slice(bytes).map_err(update_error)?;
    let key = PublicKey::decode(&decode(pubkey)?).map_err(update_error)?;
    let signature = Signature::decode(&decode(&envelope.signature)?).map_err(update_error)?;
    key.verify(envelope.manifest.as_bytes(), &signature, false)
        .map_err(|_| "Update manifest signature did not verify.".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&envelope.manifest).map_err(update_error)?;
    let manifest: Manifest = serde_json::from_value(value.clone()).map_err(update_error)?;
    if manifest.schema != 1
        || !manifest.version.pre.is_empty()
        || !(1..=PACKAGE_LIMIT).contains(&manifest.bytes)
    {
        return Err("The update manifest has unsupported content.".into());
    }
    Ok((value, manifest))
}

fn loopback(url: &Url) -> bool {
    url.scheme() == "http" && url.host_str() == Some("127.0.0.1")
}

fn approved_url(url: &Url, path: &str, acceptance: bool) -> bool {
    if acceptance {
        return loopback(url);
    }
    url.scheme() == "https" && url.host_str() == Some("github.com") && url.path().starts_with(path)
}

fn client(timeout: u64, acceptance: bool) -> Result<Client> {
    Client::builder()
        .user_agent("CashGear-FotoEditor-Updater")
        .timeout(Duration::from_secs(timeout))
        .redirect(Policy::custom(move |attempt| {
            let valid = if acceptance {
                loopback(attempt.url())
            } else {
                attempt.url().scheme() == "https"
            };
            if !valid || attempt.previous().len() >= 8 {
                attempt.error("Invalid update redirect")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(update_error)
}

async fn limited_body(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>> {
    if !response.status().is_success() {
        return Err(update_error(response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err("The update download exceeded its signed size.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(update_error)? {
        if chunk.len() > limit - bytes.len() {
            return Err("The update download exceeded its size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[tauri::command]
pub async fn check_desktop_update(window: WebviewWindow) -> Result<UpdateCheck> {
    let config: UpdateConfig = serde_json::from_value(
        window
            .config()
            .plugins
            .0
            .get("updater")
            .cloned()
            .ok_or("The update channel is unavailable.")?,
    )
    .map_err(update_error)?;
    let endpoint = config
        .endpoints
        .into_iter()
        .next()
        .ok_or("The update channel is unavailable.")?;
    let acceptance = window.config().identifier == ACCEPTANCE_ID;
    if !approved_url(
        &endpoint,
        &format!("{RELEASES_PATH}latest/download/"),
        acceptance,
    ) {
        return Err("The update channel is invalid.".into());
    }
    let mut check = UpdateCheck {
        current: window.package_info().version.to_string(),
        available: None,
    };
    let response = client(20, acceptance)?
        .get(endpoint)
        .send()
        .await
        .map_err(update_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("The stable update feed is unavailable.".into());
    }
    let bytes = limited_body(response, FEED_LIMIT).await?;
    let (value, manifest) = authenticate(&bytes, &config.pubkey)?;
    let Some(update) = window
        .updater()
        .map_err(update_error)?
        .restore_update(value)
        .map_err(update_error)?
    else {
        return Ok(check);
    };
    let path = format!("{RELEASES_PATH}download/desktop-v{}/", manifest.version);
    if !approved_url(&update.download_url, &path, acceptance)
        || !update.download_url.path().ends_with(".exe")
    {
        return Err("The update installer URL is invalid.".into());
    }
    let version = update.version.clone();
    let id = window.resources_table().add(Candidate {
        update,
        bytes: manifest.bytes,
    });
    check.available = Some(AvailableUpdate {
        id,
        version,
        bytes: manifest.bytes,
    });
    Ok(check)
}

#[tauri::command]
pub async fn download_desktop_update(window: WebviewWindow, id: ResourceId) -> Result<ResourceId> {
    let candidate = window
        .resources_table()
        .get::<Candidate>(id)
        .map_err(update_error)?;
    let acceptance = window.config().identifier == ACCEPTANCE_ID;
    let response = client(180, acceptance)?
        .get(candidate.update.download_url.clone())
        .send()
        .await
        .map_err(update_error)?;
    let bytes = limited_body(response, candidate.bytes).await?;
    if bytes.len() != candidate.bytes {
        return Err("The update download is incomplete.".into());
    }
    candidate.update.verify(&bytes).map_err(update_error)?;
    let id = window
        .resources_table()
        .add(Downloaded(candidate.update.clone(), bytes));
    Ok(id)
}

#[tauri::command]
pub async fn install_desktop_update(window: WebviewWindow, id: ResourceId) -> Result<()> {
    let package = window
        .resources_table()
        .take::<Downloaded>(id)
        .map_err(update_error)?;
    window
        .app_handle()
        .save_window_state(StateFlags::all())
        .map_err(update_error)?;
    tauri::async_runtime::spawn_blocking(move || {
        package.0.install(&package.1).map_err(update_error)
    })
    .await
    .map_err(update_error)?
}
