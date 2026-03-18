use std::borrow::Cow;
#[cfg(not(feature = "embed"))]
use std::path::Path;

#[cfg(feature = "embed")]
#[derive(rust_embed::RustEmbed)]
#[folder = "../../packages/web/dist"]
#[prefix = ""]
struct Assets;

#[cfg(not(feature = "embed"))]
const DEV_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/web/dist");

#[cfg(feature = "embed")]
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    Assets::get(path).map(|f| f.data)
}

#[cfg(not(feature = "embed"))]
pub fn get(path: &str) -> Option<Cow<'static, [u8]>> {
    let full_path = Path::new(DEV_DIR).join(path);
    std::fs::read(&full_path).ok().map(Cow::Owned)
}

pub fn mime_for(path: &str) -> String {
    mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_for_known_types() {
        assert_eq!(mime_for("style.css"), "text/css");
        assert_eq!(mime_for("app.js"), "text/javascript");
        assert_eq!(mime_for("index.html"), "text/html");
    }

    #[test]
    fn test_get_nonexistent_asset() {
        assert!(get("nonexistent_file.txt").is_none());
    }
}
