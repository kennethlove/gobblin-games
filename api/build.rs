use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = PathBuf::from(&manifest_dir).join("assets").join("icons");
    fs::create_dir_all(&out_dir).unwrap();

    // No icon sources ship with this crate, so sprites generate empty.
    // Add icon sources under api/assets/icons/src/ if icons are needed.
    let manifest_path = PathBuf::from(&manifest_dir);
    let icons_dir = manifest_path
        .parent()
        .unwrap()
        .join("api")
        .join("assets")
        .join("icons")
        .join("src");

    generate_sprite(
        &icons_dir.join("ui"),
        &out_dir.join("sprite-ui.svg"),
        "ui",
        "0 0 24 24",
    );
    generate_sprite(
        &icons_dir.join("narrative"),
        &out_dir.join("sprite-narrative.svg"),
        "narrative",
        "0 0 512 512",
    );

    println!("cargo:rerun-if-changed=assets/icons/src/ui/");
    println!("cargo:rerun-if-changed=assets/icons/src/narrative/");
}

fn generate_sprite(input_dir: &Path, output_path: &Path, category: &str, default_view_box: &str) {
    let mut symbols = Vec::new();

    if input_dir.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(input_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "svg"))
            .collect();
        entries.sort_by_key(|e| e.file_name());

        for entry in entries {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap();
            let file_name = path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .replace('_', "-");
            let id = format!("icon_{}_{}", category, file_name);
            // Keep the source viewBox: tiers mix 24px UI glyphs with 512px
            // game-icons, and templates reference hyphenated ids
            // ("arrow-left"), so underscores stay as-is in the id.
            let view_box =
                extract_view_box(&content).unwrap_or_else(|| default_view_box.to_string());

            // Extract inner content from <svg>...</svg>
            let inner = extract_svg_inner(&content);
            symbols.push(format!(
                r#"<symbol id="{}" viewBox="{}" fill="currentColor">{}</symbol>"#,
                id, view_box, inner
            ));
        }
    }

    let sprite = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" style="display:none">
{}
</svg>"#,
        symbols.join("\n")
    );

    fs::write(output_path, &sprite).unwrap();
    println!(
        "cargo:warning=Generated {} sprite with {} icons",
        category,
        symbols.len()
    );
}

fn extract_svg_inner(svg: &str) -> String {
    // Find content between first > and last </svg>
    if let Some(start) = svg.find('>') {
        let rest = &svg[start + 1..];
        if let Some(end) = rest.rfind("</svg>") {
            return rest[..end].trim().to_string();
        }
    }
    svg.to_string()
}

fn extract_view_box(svg: &str) -> Option<String> {
    let start = svg.find("viewBox=\"")? + "viewBox=\"".len();
    let end = svg[start..].find('"')? + start;
    Some(svg[start..end].to_string())
}
