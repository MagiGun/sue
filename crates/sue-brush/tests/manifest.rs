//! The engine stands on the kernel and the shared models only. A stroke
//! must replay without a window, on any thread and in the browser, so
//! the engine never reaches for the platform, the picture engine, the
//! interface or a studio.

/// The names under `[section]` of the manifest.
fn names(manifest: &str, section: &str) -> Vec<String> {
    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != section)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let name = line.split(['=', ' ']).next().unwrap_or(line);
            name.trim_end_matches(".workspace").to_owned()
        })
        .collect()
}

#[test]
fn the_engine_depends_on_the_kernel_and_the_models_only() {
    let manifest = include_str!("../Cargo.toml");
    assert_eq!(
        names(manifest, "[dependencies]"),
        ["sue-core", "sue-model"],
        "sue-brush takes pen samples and rasters, never a platform or a studio"
    );
    for line in manifest.lines().map(str::trim) {
        assert!(
            !line.starts_with("[target.") && line != "[build-dependencies]",
            "{line}: the engine is the same on every system"
        );
    }
    // What its tests may use is held too: nothing that draws or opens a
    // window.
    for name in names(manifest, "[dev-dependencies]") {
        assert!(
            ["sue-core", "sue-model"].contains(&name.as_str()),
            "{name} is not for the engine's tests"
        );
    }
}

#[test]
fn the_engine_reads_the_shared_brush_model() {
    let preset = sue_model::BrushPreset::new("Pen");
    let bytes = preset.to_bytes();
    assert_eq!(sue_model::BrushPreset::from_bytes(&bytes), Ok(preset));
    assert!(sue_model::registry().contains("sue.brush"));
    let registry = sue_core::ComponentRegistry::new();
    assert!(!registry.contains("sue.brush"));
}
