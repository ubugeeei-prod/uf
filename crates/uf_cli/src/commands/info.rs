//! `uf info`: product, distribution, and local toolchain context.

use anyhow::Result;
use camino::Utf8Path;
use uf_term::{KeyValue, Status, Tone};

use crate::brand;
use crate::ui::Ui;

pub(crate) fn info(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let cwd = cwd.as_str();
    let host = format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS);

    ui.render(|renderer, out| {
        brand::render_product_card(renderer, out, "uf info");
        renderer.blank(out);

        renderer.heading(out, 2, "distribution");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::toned("docs", brand::DOCS_URL, Tone::Path),
                KeyValue::new("curl", brand::CURL_INSTALL),
                KeyValue::new("nix run", brand::NIX_RUN),
                KeyValue::new("nix profile", brand::NIX_PROFILE),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "runtime");
        renderer.key_values(
            out,
            4,
            &[
                KeyValue::toned("version", version, Tone::Number),
                KeyValue::new("host", &host),
                KeyValue::toned("cwd", cwd, Tone::Path),
                KeyValue::toned("brand tokens", brand::BRAND_TOKENS, Tone::Path),
            ],
        );
        renderer.blank(out);

        renderer.heading(out, 2, "design tokens");
        renderer.table(out, 4, &brand::palette_table());
        renderer.blank(out);
        renderer.status(out, Status::Success, "brand system loaded");
    });

    Ok(())
}
