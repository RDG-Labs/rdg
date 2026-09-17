use client::{RDG_URL_SCHEME, ZED_URL_SCHEME};
use gpui::{AsyncApp, actions};

actions!(
    cli,
    [
        /// Registers the rdg:// and legacy zed:// URL scheme handlers.
        RegisterZedScheme
    ]
);

pub async fn register_zed_scheme(cx: &AsyncApp) -> anyhow::Result<()> {
    for scheme in [RDG_URL_SCHEME, ZED_URL_SCHEME] {
        cx.update(|cx| cx.register_url_scheme(scheme)).await?;
    }
    Ok(())
}
