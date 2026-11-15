use std::env::VarError;
use std::fmt::Display;
use std::net::{Ipv4Addr, SocketAddr};
use std::num::{NonZeroUsize, ParseIntError};
use std::path::PathBuf;
use std::time::Duration;

use axum_extra::extract::cookie::Key;

use wastebin_core::env::vars::{
    self, ADDRESS_PORT, BASE_URL, BUTTON_LABELS, CACHE_SIZE, DATABASE_PATH, DRAFT_AUTOSAVE,
    HTTP_TIMEOUT, MAX_BODY_SIZE, PASTE_EXPIRATIONS, SIGNING_KEY,
};

use crate::page::CustomAssets;
use wastebin_core::{db, expiration};
use wastebin_highlight::{Theme, theme::ParseThemeNameError};

pub const DEFAULT_HTTP_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("failed to construct cache")]
    CacheConstruction(#[from] cached::BuildError),
    #[error("failed to parse {CACHE_SIZE}, expected number of elements: {0}")]
    CacheSize(ParseIntError),
    #[error("failed to parse {DATABASE_PATH}, contains non-Unicode data")]
    DatabasePath,
    #[error("failed to parse {MAX_BODY_SIZE}, expected number of bytes: {0}")]
    MaxBodySize(ParseIntError),
    #[error("failed to parse {ADDRESS_PORT}, expected `host:port`")]
    AddressPort,
    #[error("failed to parse {BASE_URL}: {0}")]
    BaseUrl(String),
    #[error("failed to generate key from {SIGNING_KEY}: {0}")]
    SigningKey(String),
    #[error("failed to parse {HTTP_TIMEOUT}: {0}")]
    HttpTimeout(ParseIntError),
    #[error("failed to parse {PASTE_EXPIRATIONS}: {0}")]
    ParsePasteExpiration(#[from] expiration::Error),
    #[error("failed to parse theme name")]
    ParseTheme(#[from] ParseThemeNameError),
    #[error("failed to parse {BUTTON_LABELS}, expected `true` or `false`")]
    ButtonLabels,
    #[error("failed to parse {DRAFT_AUTOSAVE}, expected `true` or `false`")]
    DraftAutosave,
    #[error("failed to read {0} from {1}: {2}")]
    CustomAsset(&'static str, String, std::io::Error),
    #[error("failed to parse {0}, contains non-Unicode data")]
    CustomAssetPath(&'static str),
    #[error("binding to both TCP and Unix socket is not possible")]
    BothListeners,
}

pub(crate) enum SocketType {
    Tcp(SocketAddr),
    Unix(PathBuf),
}

impl Display for SocketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SocketType::Tcp(addr) => {
                write!(f, "{addr}")
            }
            SocketType::Unix(path) => {
                write!(f, "{}", path.display())
            }
        }
    }
}

pub fn title() -> String {
    std::env::var(vars::TITLE).unwrap_or_else(|_| "wastebin".to_string())
}

pub fn theme() -> Result<Theme, Error> {
    Ok(std::env::var(vars::THEME).map_or_else(|_| Ok(Theme::Ayu), |var| var.parse())?)
}

/// Whether to show text labels next to the navigation icons, off by default.
pub fn button_labels() -> Result<bool, Error> {
    std::env::var(vars::BUTTON_LABELS)
        .map_or(Ok(false), |var| parse_bool(&var).ok_or(Error::ButtonLabels))
}

/// Whether the new-paste form keeps its draft in the browser's session storage, off by default.
pub fn draft_autosave() -> Result<bool, Error> {
    std::env::var(vars::DRAFT_AUTOSAVE).map_or(Ok(false), |var| {
        parse_bool(&var).ok_or(Error::DraftAutosave)
    })
}

/// Parse a boolean flag such as `true`, `1`, `yes` or `on`.
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" | "" => Some(false),
        _ => None,
    }
}

pub fn cache_size() -> Result<NonZeroUsize, Error> {
    std::env::var(vars::CACHE_SIZE)
        .map_or_else(
            |_| Ok(NonZeroUsize::new(128).expect("128 is non-zero")),
            |s| s.parse::<NonZeroUsize>(),
        )
        .map_err(Error::CacheSize)
}

pub fn database_method() -> Result<db::Open, Error> {
    match std::env::var(vars::DATABASE_PATH) {
        Ok(path) => Ok(db::Open::Path(PathBuf::from(path))),
        Err(VarError::NotUnicode(_)) => Err(Error::DatabasePath),
        Err(VarError::NotPresent) => Ok(db::Open::Memory),
    }
}

pub fn signing_key() -> Result<Key, Error> {
    std::env::var(vars::SIGNING_KEY).map_or_else(
        |_| Ok(Key::generate()),
        |s| Key::try_from(s.as_bytes()).map_err(|err| Error::SigningKey(err.to_string())),
    )
}

pub fn socket_type() -> Result<SocketType, Error> {
    match (
        std::env::var(vars::ADDRESS_PORT),
        std::env::var(vars::SOCKET_PATH),
    ) {
        (Ok(_), Ok(_)) => Err(Error::BothListeners),
        (Ok(var), Err(_)) => {
            let addr: SocketAddr = var.parse().map_err(|_| Error::AddressPort)?;
            Ok(SocketType::Tcp(addr))
        }
        (Err(_), Ok(var)) => Ok(SocketType::Unix(var.into())),
        (Err(_), Err(_)) => {
            let addr = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 8088);
            Ok(SocketType::Tcp(addr))
        }
    }
}

pub fn max_body_size() -> Result<usize, Error> {
    std::env::var(vars::MAX_BODY_SIZE)
        .map_or_else(|_| Ok(1024 * 1024), |s| s.parse::<usize>())
        .map_err(Error::MaxBodySize)
}

/// Read base URL either from the environment variable or fallback to the hostname.
pub fn base_url() -> Result<url::Url, Error> {
    if let Some(base_url) = std::env::var(vars::BASE_URL).map_or_else(
        |err| {
            if matches!(err, VarError::NotUnicode(_)) {
                Err(Error::BaseUrl(format!("{BASE_URL} is not unicode")))
            } else {
                Ok(None)
            }
        },
        |var| {
            Ok(Some(
                url::Url::parse(&var).map_err(|err| Error::BaseUrl(err.to_string()))?,
            ))
        },
    )? {
        return Ok(base_url);
    }

    let hostname =
        hostname::get().map_err(|err| Error::BaseUrl(format!("failed to get hostname: {err}")))?;

    url::Url::parse(&format!("https://{}", hostname.to_string_lossy()))
        .map_err(|err| Error::BaseUrl(err.to_string()))
}

pub fn http_timeout() -> Result<Duration, Error> {
    std::env::var(vars::HTTP_TIMEOUT)
        .map_or_else(
            |_| Ok(DEFAULT_HTTP_TIMEOUT),
            |s| s.parse::<u64>().map(|v| Duration::new(v, 0)),
        )
        .map_err(Error::HttpTimeout)
}

/// Parse [`expiration::ExpirationSet`] from environment or return default.
pub fn expiration_set() -> Result<expiration::ExpirationSet, Error> {
    let set = std::env::var(vars::PASTE_EXPIRATIONS).map_or_else(
        |_| "0=d,10m,1h,1d,1w,1M,1y".parse::<expiration::ExpirationSet>(),
        |value| value.parse::<expiration::ExpirationSet>(),
    )?;

    Ok(set)
}

/// Read the file named by the environment variable `var`, `None` if it is not set.
fn custom_asset(var: &'static str) -> Result<Option<Vec<u8>>, Error> {
    match std::env::var(var) {
        Ok(path) if path.is_empty() => Ok(None),
        Ok(path) => read_custom_asset(var, path).map(Some),
        Err(VarError::NotUnicode(_)) => Err(Error::CustomAssetPath(var)),
        Err(VarError::NotPresent) => Ok(None),
    }
}

/// Operator-provided stylesheet and script, read once at startup.
pub fn custom_assets() -> Result<CustomAssets, Error> {
    Ok(CustomAssets {
        css: custom_asset(vars::CUSTOM_CSS_PATH)?,
        js: custom_asset(vars::CUSTOM_JS_PATH)?,
    })
}

fn read_custom_asset(var: &'static str, path: String) -> Result<Vec<u8>, Error> {
    std::fs::read(&path).map_err(|err| Error::CustomAsset(var, path, err))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_bool_values() {
        for value in ["true", "TRUE", "1", "yes", "on", " true "] {
            assert_eq!(parse_bool(value), Some(true), "{value}");
        }

        for value in ["false", "False", "0", "no", "off", ""] {
            assert_eq!(parse_bool(value), Some(false), "{value}");
        }

        assert_eq!(parse_bool("maybe"), None);
    }

    #[test]
    fn custom_asset_is_read() -> Result<(), Box<dyn std::error::Error>> {
        let path =
            std::env::temp_dir().join(format!("wastebin-custom-asset-{}.js", std::process::id()));
        std::fs::write(&path, "console.log(1);")?;

        let content = read_custom_asset(vars::CUSTOM_JS_PATH, path.display().to_string());
        std::fs::remove_file(&path)?;
        assert_eq!(content?, b"console.log(1);");

        Ok(())
    }

    #[test]
    fn missing_custom_asset_is_an_error() {
        let err = read_custom_asset(
            vars::CUSTOM_CSS_PATH,
            String::from("/nonexistent/custom.css"),
        )
        .unwrap_err();

        assert!(matches!(
            err,
            Error::CustomAsset(vars::CUSTOM_CSS_PATH, _, _)
        ));
        assert!(err.to_string().contains("/nonexistent/custom.css"));
    }
}
