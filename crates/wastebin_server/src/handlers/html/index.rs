use askama::Template;
use askama_web::WebTemplate;
use axum::extract::State;

use crate::i18n::Lang;
use crate::{Highlighter, Page, handlers::extract::Theme};

/// GET handler for the index page.
pub async fn get(
    State(page): State<Page>,
    State(highlighter): State<Highlighter>,
    theme: Option<Theme>,
    lang: Lang,
) -> Index {
    Index {
        page,
        theme,
        lang,
        highlighter,
    }
}

/// Index page displaying a form for paste insertion and a selection box for languages.
#[derive(Template, WebTemplate)]
#[template(path = "index.html")]
pub(crate) struct Index {
    page: Page,
    theme: Option<Theme>,
    lang: Lang,
    highlighter: Highlighter,
}

#[cfg(test)]
mod tests {
    use crate::cache::valid_extension;

    /// Picker extension coverage.
    ///
    /// The template submits `extensions.first()`.
    #[test]
    fn picker_extensions_pass_validation() {
        let highlighter = wastebin_highlight::Highlighter::default();

        for syntax in highlighter.syntaxes() {
            let Some(extension) = syntax.extensions.first() else {
                continue;
            };

            assert!(
                valid_extension(extension),
                "syntax {:?} submits invalid extension {:?}",
                syntax.name,
                extension
            );
        }
    }

    use crate::handlers::insert::form::Entry;
    use crate::test_helpers::{Client, DraftAutosave, StoreCookies};
    use reqwest::StatusCode;

    /// Route of the draft script as referenced by `body`, if any.
    fn draft_script(body: &str) -> Option<&str> {
        let start = body.find("src=\"/draft.")? + "src=\"".len();
        let end = start + body[start..].find('"')?;
        Some(&body[start..end])
    }

    /// Location a form submission of `entry` redirects to.
    async fn submit(client: &Client, entry: &Entry) -> Result<String, Box<dyn std::error::Error>> {
        let res = client.post_form().form(entry).send().await?;
        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        Ok(res.headers().get("location").unwrap().to_str()?.to_owned())
    }

    #[tokio::test]
    async fn draft_autosave_is_off_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::new(StoreCookies(false)).await;

        let body = client.get("/").send().await?.text().await?;
        assert_eq!(draft_script(&body), None);

        let entry = Entry {
            text: String::from("FooBarBaz"),
            ..Default::default()
        };
        let location = submit(&client, &entry).await?;
        let body = client.get(&location).send().await?.text().await?;
        assert_eq!(draft_script(&body), None);

        Ok(())
    }

    #[tokio::test]
    async fn draft_autosave_loads_script() -> Result<(), Box<dyn std::error::Error>> {
        let client = Client::with_draft_autosave(StoreCookies(false), DraftAutosave(true)).await;

        let body = client.get("/").send().await?.text().await?;
        let route = draft_script(&body).expect("index loads the draft script");

        // Served from the same origin, so the strict `script-src 'self'` policy allows it.
        let res = client.get(route).send().await?;
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(
            res.headers().get("content-type").unwrap(),
            "text/javascript"
        );
        let script = res.text().await?;
        assert!(script.contains("sessionStorage"));
        // The password field is never read, let alone stored.
        assert!(!script.contains("\"password\""));
        assert!(!script.contains("encrypt-toggle"));
        assert!(!script.contains("burn-after-reading"));

        // Every page a successful submission lands on loads it to discard the draft.
        let plain = Entry {
            text: String::from("FooBarBaz"),
            ..Default::default()
        };
        let encrypted = Entry {
            text: String::from("FooBarBaz"),
            password: String::from("hunter2"),
            ..Default::default()
        };
        let burn = Entry {
            text: String::from("FooBarBaz"),
            burn_after_reading: Some(String::from("on")),
            ..Default::default()
        };

        for entry in [plain, encrypted, burn] {
            let location = submit(&client, &entry).await?;
            let body = client.get(&location).send().await?.text().await?;
            assert_eq!(draft_script(&body), Some(route), "{location}");
        }

        // Error pages keep the draft for another try.
        let res = client.get("/000000").send().await?;
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(draft_script(&res.text().await?), None);

        Ok(())
    }
}
