use url::Url;

use crate::assets::{Asset, Css, Kind};
use wastebin_core::expiration::{Expiration, ExpirationSet};
use wastebin_highlight::Theme;

/// Static page assets.
pub(crate) struct Assets {
    pub favicon: Asset,
    pub css: Css,
    pub index_js: Asset,
    pub paste_js: Asset,
    pub burn_js: Asset,
    pub password_toggle_js: Asset,
    /// Operator-provided stylesheet, see [`CustomAssets`].
    pub custom_css: Option<Asset>,
    /// Operator-provided script, see [`CustomAssets`].
    pub custom_js: Option<Asset>,
}

/// Content of an operator-provided stylesheet and script that every page includes.
///
/// They are served like the built-in assets: from the same origin, under a route derived from
/// their content hash, so that the `script-src 'self'` and `style-src 'self'` policies hold.
#[derive(Default)]
pub(crate) struct CustomAssets {
    pub css: Option<Vec<u8>>,
    pub js: Option<Vec<u8>>,
}

pub(crate) struct Page {
    pub version: &'static str,
    pub title: String,
    pub assets: Assets,
    pub base_url: Url,
    pub expirations: Vec<Expiration>,
    pub max_body_size: usize,
}

impl Page {
    /// Create new page meta data from generated  `assets`, `title` and optional `base_url`.
    #[must_use]
    pub fn new(
        title: String,
        base_url: Url,
        theme: Theme,
        expirations: ExpirationSet,
        max_body_size: usize,
    ) -> Self {
        let assets = Assets::new(theme);
        let expirations = expirations.into_inner();

        Self {
            version: env!("CARGO_PKG_VERSION"),
            title,
            assets,
            base_url,
            expirations,
            max_body_size,
        }
    }

    /// Include the operator-provided stylesheet and script of `custom` on every page.
    #[must_use]
    pub fn with_custom_assets(mut self, custom: CustomAssets) -> Self {
        self.assets.custom_css = custom
            .css
            .map(|content| Asset::new("custom", Kind::Css, content));
        self.assets.custom_js = custom
            .js
            .map(|content| Asset::new("custom", Kind::Js, content));
        self
    }
}

impl Assets {
    /// Create page [`Assets`] for the given `theme`.
    fn new(theme: Theme) -> Self {
        Self {
            favicon: Asset::new(
                "favicon",
                Kind::Png,
                include_bytes!("../../../assets/favicon.png").to_vec(),
            ),
            css: Css::new(theme),
            index_js: Asset::new(
                "index",
                Kind::Js,
                include_bytes!("javascript/index.js").to_vec(),
            ),
            paste_js: Asset::new(
                "paste",
                Kind::Js,
                include_bytes!("javascript/paste.js").to_vec(),
            ),
            burn_js: Asset::new(
                "burn",
                Kind::Js,
                include_bytes!("javascript/burn.js").to_vec(),
            ),
            password_toggle_js: Asset::new(
                "password-toggle",
                Kind::Js,
                include_bytes!("javascript/password-toggle.js").to_vec(),
            ),
            custom_css: None,
            custom_js: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CustomAssets;
    use crate::handlers::insert::form::Entry;
    use crate::test_helpers::{Client, StoreCookies};
    use reqwest::StatusCode;

    const CUSTOM_CSS: &str = "body { outline: 1px solid red; }";
    const CUSTOM_JS: &str = "document.body.dataset.custom = 'yes';";

    /// Route of the custom asset with extension `ext` referenced by `body`, if any.
    fn custom_route(body: &str, ext: &str) -> Option<String> {
        body.split('"')
            .find(|s| s.starts_with("/custom.") && s.ends_with(&format!(".{ext}")))
            .map(String::from)
    }

    fn custom() -> CustomAssets {
        CustomAssets {
            css: Some(CUSTOM_CSS.as_bytes().to_vec()),
            js: Some(CUSTOM_JS.as_bytes().to_vec()),
        }
    }

    #[tokio::test]
    async fn no_custom_assets_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::new(StoreCookies(false)).await;
        let body = client.get("/").send().await?.text().await?;

        assert!(!body.contains("/custom."));

        Ok(())
    }

    #[tokio::test]
    async fn custom_assets_on_every_page() -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::with_custom_assets(StoreCookies(false), custom()).await;

        let entry = Entry {
            text: String::from("FooBarBaz"),
            ..Default::default()
        };
        let res = client.post_form().form(&entry).send().await?;
        let paste = res.headers().get("location").unwrap().to_str()?.to_owned();

        let index = client.get("/").send().await?.text().await?;
        let css = custom_route(&index, "css").expect("index links the custom stylesheet");
        let js = custom_route(&index, "js").expect("index loads the custom script");

        // The custom stylesheet comes after the built-in one so it can override it.
        assert!(index.find("/style.").unwrap() < index.find(&css).unwrap());
        assert!(index.contains(&format!("<script defer src=\"{js}\"></script>")));

        for path in [paste.as_str(), "/000000"] {
            let body = client.get(path).send().await?.text().await?;
            assert_eq!(custom_route(&body, "css").as_ref(), Some(&css), "{path}");
            assert_eq!(custom_route(&body, "js").as_ref(), Some(&js), "{path}");
        }

        for (route, content, mime) in [
            (&css, CUSTOM_CSS, "text/css"),
            (&js, CUSTOM_JS, "text/javascript"),
        ] {
            let res = client.get(route).send().await?;
            assert_eq!(res.status(), StatusCode::OK);
            assert_eq!(res.headers().get("content-type").unwrap(), mime);

            // Same strict policy as without custom assets.
            let csp = res
                .headers()
                .get("content-security-policy")
                .unwrap()
                .to_str()?;
            assert!(csp.contains("script-src 'self';"));
            assert!(!csp.contains("unsafe-inline"));

            assert_eq!(res.text().await?, content);
        }

        Ok(())
    }
}
