//! The Filter menu's commands, and the tonal corrections of Edit.

use sue_wm::Command;

use crate::studio::SuePaint;

/// A command of this group, run and grayed as the studio always did.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "filter", super::run).state(super::state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("filter.brightness", "brightness-contrast-ellipsis"),
    c("filter.levels", "level-correction-ellipsis"),
    c("filter.curve", "tone-curve-ellipsis"),
    c("filter.hue", "hue-saturation-luminosity-ellipsis").keys("Ctrl+U"),
    c("filter.balance", "color-balance-ellipsis"),
    c("filter.invert", "reverse-gradient").keys("Ctrl+I"),
    c("filter.posterize", "posterization-ellipsis"),
    c("filter.binarize", "binarization-ellipsis"),
    c("filter.gradient_map", "gradient-map-ellipsis"),
    c("filter.lines_tones", "convert-to-lines-and-tones-ellipsis"),
    c("filter.quick.layer", "apply-to-current-layer-ellipsis"),
    c("filter.quick.all", "apply-to-all-merged-ellipsis"),
    c("filter.effects.layer", "apply-to-current-layer-ellipsis"),
    c("filter.effects.all", "apply-to-all-merged-ellipsis"),
    c("filter.blur", "blur"),
    c("filter.blur_strong", "blur-strong"),
    c("filter.gaussian", "gaussian-blur-ellipsis"),
    c("filter.motion_blur", "motion-blur-ellipsis"),
    c("filter.radial_blur", "radial-blur-ellipsis"),
    c("filter.zoom_blur", "zoom-blur-ellipsis"),
    c("filter.lens_blur", "lens-blur-ellipsis"),
    c("filter.smooth", "smooth"),
    c("filter.dust", "remove-dust-ellipsis"),
    c("filter.jpeg_noise", "remove-jpeg-noise-ellipsis"),
    c("filter.line_width", "line-width-ellipsis"),
    c("filter.wave", "wave-ellipsis"),
    c("filter.pinch", "pinch-ellipsis"),
    c("filter.polar", "polar-coordinates-ellipsis"),
    c("filter.ripple", "ripple-ellipsis"),
    c("filter.fisheye", "fisheye-ellipsis"),
    c("filter.twirl", "twirl-ellipsis"),
    c("filter.zigzag", "zigzag-ellipsis"),
    c("filter.mosaic", "mosaic-ellipsis"),
    c("filter.crystallize", "crystallize-ellipsis"),
    c("filter.noise", "add-noise-ellipsis"),
    c("filter.line_extraction", "line-extraction-ellipsis"),
    c("filter.aberration", "chromatic-aberration-ellipsis"),
    c("filter.glow", "glow-ellipsis"),
    c("filter.inflate", "inflate-ellipsis"),
    c("filter.drop_shadow", "drop-shadow-ellipsis"),
    c("filter.retro", "retro-ellipsis"),
    c("filter.focus_lines", "focus-lines-ellipsis"),
    c("filter.speed_lines", "speed-lines-ellipsis"),
    c("filter.tone", "screentone-ellipsis"),
    c("filter.perlin", "perlin-noise-ellipsis"),
    c("filter.clouds", "clouds"),
    c("filter.sharpen", "sharpen"),
    c("filter.sharpen_strong", "sharpen-strong"),
    c("filter.unsharp", "unsharp-mask-ellipsis"),
];
