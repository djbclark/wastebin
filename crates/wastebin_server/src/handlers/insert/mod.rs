use crate::cache::valid_extension;
use wastebin_core::db::write;

pub mod api;
pub mod form;

/// Fill in a missing extension from the first line of the paste, see
/// [`wastebin_highlight::Highlighter::detect_extension`].
///
/// An extension chosen by the user, including the plain text one, is never replaced. Encrypted
/// pastes are left alone because the extension is stored and shown in the clear.
fn detect_extension(highlighter: &wastebin_highlight::Highlighter, entry: &mut write::Entry) {
    if entry.extension.is_some() || entry.password.is_some() {
        return;
    }

    entry.extension = highlighter
        .detect_extension(&entry.text)
        .filter(|extension| valid_extension(extension))
        .map(ToOwned::to_owned);
}
