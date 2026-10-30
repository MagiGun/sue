// Generated from the spec in prefs_gen.py: Clip Studio Paint's Preferences,
// page by page. Each label is an id in the language database (paint).

use super::{Group, Item, Page, Wire};

pub(super) const PAGES: &[Page] = &[
    Page {
        title: "pref-tools",
        groups: &[
            Group {
                title: "pref-switch-tool-temporarily",
                items: &[
                    Item::Check { label: "pref-switch-tool-temporarily-by-pressing-and-holding", on: true, wire: Wire::None },
                    Item::Number { label: "pref-length-of-keypress-to-switch-tools", value: 500.0, min: 0.0, max: 5000.0, decimals: 0, unit: "ms", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-options",
                items: &[
                    Item::Number { label: "pref-minimum-drag-distance", value: 3.0, min: 0.0, max: 100.0, decimals: 0, unit: "", wire: Wire::None },
                    Item::Choice { label: "pref-auto-scroll-at-edge-of-canvas", choices: &[("drag", "pref-drag"), ("off", "pref-disable")], value: "drag", wire: Wire::None },
                    Item::Choice { label: "pref-handle-size", choices: &[("small", "pref-small"), ("medium", "pref-medium"), ("large", "pref-large")], value: "medium", wire: Wire::None },
                    Item::Check { label: "pref-use-fast-view-while-navigating-canvas", on: false, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-brush-line",
                items: &[
                    Item::Check { label: "pref-start-from-current-size-when-changing-brush-size", on: true, wire: Wire::None },
                    Item::Check { label: "pref-display-brush-size-adjustment-on-canvas", on: true, wire: Wire::None },
                    Item::Check { label: "pref-reduce-pen-pressure-when-drawing-with-finger-mou", on: false, wire: Wire::None },
                    Item::Check { label: "pref-preview-lines-by-pressing-modifier-key-shift", on: true, wire: Wire::Bool("sue_paint.shift_line") },
                ],
            },
            Group {
                title: "pref-stroke-preview",
                items: &[
                    Item::Check { label: "pref-show-stroke-preview-to-reduce-brush-lag", on: true, wire: Wire::None },
                    Item::Choice { label: "pref-display-settings", choices: &[("auto", "pref-automatic")], value: "auto", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-figure",
                items: &[
                    Item::Choice { label: "pref-figure-tool-guidelines", choices: &[("both", "pref-line-preview-and-control-points")], value: "both", wire: Wire::None },
                    Item::Check { label: "pref-confirm-shape-with-double-click-or-double-tap", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-color",
                items: &[
                    Item::Check { label: "pref-disable-drawing-with-transparency-on-transparenc", on: false, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-smart-shape",
                items: &[
                    Item::Check { label: "pref-hold-to-create-figures", on: true, wire: Wire::None },
                    Item::Number { label: "pref-length-of-long-press", value: 0.7, min: 0.1, max: 5.0, decimals: 2, unit: "seconds", wire: Wire::None },
                    Item::Check { label: "pref-show-smart-shape-launcher", on: true, wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-tablet",
        groups: &[
            Group {
                title: "pref-using-tablet-service",
                items: &[
                    Item::Radio { label: "", choices: &[("wintab", "pref-wintab"), ("tablet_pc", "pref-tablet-pc")], value: "wintab", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-coordinate-detection-mode",
                items: &[
                    Item::Check { label: "pref-use-mouse-mode-in-tablet-driver-settings", on: false, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-tablet-operation-area",
                items: &[
                    Item::Check { label: "pref-set-tablet-operation-area-with-application", on: false, wire: Wire::None },
                    Item::Radio { label: "pref-operation-area-on-monitor", choices: &[("all", "pref-run-on-all-monitors"), ("one", "pref-operate-specified-monitor")], value: "all", wire: Wire::None },
                    Item::Radio { label: "pref-operation-area-on-tablet", choices: &[("aspect", "pref-operate-while-keeping-aspect-ratio-of-monitor"), ("whole", "pref-operate-with-whole-tablet")], value: "aspect", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-tablet-settings",
                items: &[
                    Item::Choice { label: "pref-input-frequency", choices: &[("quality", "pref-prefer-quality"), ("speed", "pref-prefer-speed")], value: "quality", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-touch-gestures",
        groups: &[
            Group {
                title: "pref-canvas-operation",
                items: &[
                    Item::Check { label: "pref-use-tools-with-finger", on: true, wire: Wire::None },
                    Item::Choice { label: "pref-single-swipe", choices: &[("same", "pref-use-the-same-tool-with-fingers-and-pen")], value: "same", wire: Wire::None },
                    Item::Note { text: "pref-you-can-change-tools-assigned-to-touch-gestures" },
                    Item::Choice { label: "pref-two-finger-swipe", choices: &[("move", "pref-move-canvas")], value: "move", wire: Wire::None },
                    Item::Choice { label: "pref-two-finger-pinch", choices: &[("scale", "pref-scale-canvas-up-down")], value: "scale", wire: Wire::None },
                    Item::Choice { label: "pref-two-finger-rotation", choices: &[("rotate", "pref-rotate-canvas")], value: "rotate", wire: Wire::None },
                    Item::Choice { label: "pref-two-finger-tap", choices: &[("undo", "pref-undo"), ("none", "pref-disable")], value: "undo", wire: Wire::Text("sue_paint.two_finger_tap") },
                    Item::Choice { label: "pref-three-finger-tap", choices: &[("redo", "pref-redo"), ("none", "pref-disable")], value: "redo", wire: Wire::Text("sue_paint.three_finger_tap") },
                ],
            },
            Group {
                title: "pref-palette",
                items: &[
                    Item::Choice { label: "pref-two-finger-tap-on-a-palette", choices: &[("none", "pref-disable")], value: "none", wire: Wire::None },
                    Item::Choice { label: "pref-three-finger-tap-the-navigator", choices: &[("reset", "pref-reset-canvas-display"), ("none", "pref-disable")], value: "reset", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-limit-touch-operation",
                items: &[
                    Item::Check { label: "pref-disable-palette-operation-with-finger-or-mouse", on: false, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-touch-gesture-settings",
                items: &[
                    Item::Radio { label: "", choices: &[("windows", "pref-windows-gestures"), ("sue", "pref-sue-paint-gestures")], value: "sue", wire: Wire::None },
                    Item::Note { text: "pref-with-sue-paint-gestures-you-can-use-a-three-fing" },
                ],
            },
        ],
    },
    Page {
        title: "pref-command",
        groups: &[
            Group {
                title: "pref-transform",
                items: &[
                    Item::Check { label: "pref-only-target-raster-layers-vector-layers-selectio", on: false, wire: Wire::None },
                    Item::Note { text: "pref-even-if-this-is-turned-off-the-process-will-only" },
                    Item::Check { label: "pref-confirm-transformations-with-double-click-or-dou", on: false, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-paste",
                items: &[
                    Item::Choice { label: "pref-when-svg-code-is-saved-to-clipboard", choices: &[("vector", "pref-convert-to-vector-lines")], value: "vector", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-interface",
        groups: &[
            Group {
                title: "pref-ime-control",
                items: &[
                    Item::Check { label: "pref-automatically-activate-ime-when-editing-text-lay", on: true, wire: Wire::None },
                    Item::Check { label: "pref-automatically-switch-ime-when-typing-text-into-p", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-color",
                items: &[
                    Item::Choice { label: "pref-interface-color", choices: &[("gray", "pref-gray"), ("sue-theme", "pref-sue-theme"), ("void", "pref-void"), ("graphite", "pref-graphite"), ("paper", "pref-paper"), ("custom", "pref-custom")], value: "gray", wire: Wire::Text("sue_paint.theme") },
                    Item::Number { label: "pref-adjust-density", value: 50.0, min: 0.0, max: 100.0, decimals: 0, unit: "", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-touch-operations",
                items: &[
                    Item::Check { label: "pref-adjust-interface-for-touch-operations", on: false, wire: Wire::None },
                    Item::Radio { label: "pref-interface-scaling", choices: &[("small", "pref-small"), ("default", "pref-default"), ("large", "pref-large")], value: "default", wire: Wire::None },
                    Item::Note { text: "pref-when-you-change-the-settings-the-position-and-si" },
                ],
            },
            Group {
                title: "pref-layout",
                items: &[
                    Item::Check { label: "pref-switch-to-tablet-interface", on: false, wire: Wire::Flag("display.interface", "tablet", "auto") },
                    Item::Note { text: "pref-when-changing-the-settings-the-palette-layout-wi" },
                ],
            },
        ],
    },
    Page {
        title: "pref-performance",
        groups: &[
            Group {
                title: "pref-memory",
                items: &[
                    Item::Number { label: "pref-allocate-to-application", value: 70.0, min: 10.0, max: 90.0, decimals: 0, unit: "%", wire: Wire::None },
                    Item::Note { text: "pref-the-changes-will-take-effect-after-restarting-th" },
                ],
            },
            Group {
                title: "pref-undo",
                items: &[
                    Item::Number { label: "pref-undo-levels", value: 200.0, min: 1.0, max: 10000.0, decimals: 0, unit: "", wire: Wire::None },
                    Item::Number { label: "pref-undo-interval-between-actions", value: 0.0, min: 0.0, max: 10000.0, decimals: 0, unit: "ms", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-cursor",
        groups: &[
            Group {
                title: "pref-shape-of-cursor",
                items: &[
                    Item::Choice { label: "pref-brush-type-cursor", choices: &[("brush", "pref-brush-shape"), ("crosshair", "pref-crosshair"), ("dot", "pref-dot")], value: "brush", wire: Wire::Text("sue_paint.brush_cursor") },
                    Item::Choice { label: "pref-pen-type-cursor", choices: &[("brush", "pref-brush-shape")], value: "brush", wire: Wire::None },
                    Item::Choice { label: "pref-eraser-type-cursor", choices: &[("brush", "pref-brush-shape")], value: "brush", wire: Wire::None },
                    Item::Choice { label: "pref-figure-type-cursor", choices: &[("tool", "pref-tool-specific")], value: "tool", wire: Wire::None },
                    Item::Choice { label: "pref-fill-type-cursor", choices: &[("tool", "pref-tool-specific")], value: "tool", wire: Wire::None },
                    Item::Choice { label: "pref-view-type-cursor", choices: &[("tool", "pref-tool-specific")], value: "tool", wire: Wire::None },
                    Item::Choice { label: "pref-line-correction-type-cursor", choices: &[("size", "pref-brush-size")], value: "size", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-additional-display-when-brush-size-cursor-is-sma",
                items: &[
                    Item::Radio { label: "", choices: &[("none", "pref-none"), ("dot", "pref-dot"), ("line", "pref-line")], value: "line", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-display-position-of-reversed-cursor-brush-size-s",
                items: &[
                    Item::Radio { label: "", choices: &[("now", "pref-no-delay"), ("delay", "pref-delay-for-stabilization")], value: "delay", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-cursor-options",
                items: &[
                    Item::Choice { label: "pref-change-brush-cursor", choices: &[("direction", "pref-change-direction-only")], value: "direction", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-layer-frame",
        groups: &[
            Group {
                title: "pref-layer",
                items: &[
                    Item::Choice { label: "pref-name-of-copies", choices: &[("copy", "pref-name-copy")], value: "copy", wire: Wire::None },
                    Item::Check { label: "pref-use-through-blending-mode-for-new-layer-folders", on: false, wire: Wire::None },
                    Item::Number { label: "pref-opacity-of-selection-layers", value: 50.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::Int("sue_paint.selection_layer_opacity", 1) },
                    Item::Number { label: "pref-vector-path-opacity", value: 70.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                    Item::Choice { label: "pref-how-to-scale-down-screentones", choices: &[("tone", "pref-show-tone")], value: "tone", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-mask",
                items: &[
                    Item::Color { label: "pref-area-color", rgb: [0, 0, 255] },
                    Item::Number { label: "pref-opacity-of-mask", value: 30.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                    Item::Check { label: "pref-display-mask-area-when-creating-mask", on: false, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-frame-border",
                items: &[
                    Item::Number { label: "pref-vertical-gutter", value: 20.0, min: 0.0, max: 1000.0, decimals: 2, unit: "px", wire: Wire::None },
                    Item::Number { label: "pref-horizontal-gutter", value: 50.0, min: 0.0, max: 1000.0, decimals: 2, unit: "px", wire: Wire::None },
                    Item::Check { label: "pref-display-mask-area-when-creating-frame", on: true, wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-animation",
        groups: &[
            Group {
                title: "pref-animation-folder",
                items: &[
                    Item::Choice { label: "pref-how-to-paste-to-cels", choices: &[("selected", "pref-paste-to-selected-layer")], value: "selected", wire: Wire::None },
                    Item::Note { text: "pref-this-does-not-apply-to-pasting-an-object-selecte" },
                    Item::Check { label: "pref-add-cel-to-frame-when-adding-layer", on: true, wire: Wire::None },
                    Item::Check { label: "pref-remove-cel-when-deleting-moving-layer", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-timeline",
                items: &[
                    Item::Check { label: "pref-automatically-extend-clip-length", on: true, wire: Wire::None },
                    Item::Note { text: "pref-extends-the-clip-according-to-changes-to-the-fir" },
                    Item::Check { label: "pref-link-clip-operations-with-track-labels", on: true, wire: Wire::None },
                    Item::Note { text: "pref-when-moving-deleting-copying-cutting-or-pasting" },
                ],
            },
            Group {
                title: "pref-when-registering-light-table",
                items: &[
                    Item::Number { label: "pref-opacity", value: 100.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                    Item::Choice { label: "pref-color-mode", choices: &[("color", "pref-color")], value: "color", wire: Wire::None },
                    Item::Color { label: "pref-layer-color", rgb: [40, 110, 230] },
                    Item::Color { label: "pref-sub-color", rgb: [255, 255, 255] },
                ],
            },
            Group {
                title: "pref-overall-light-table-opacity",
                items: &[
                    Item::Number { label: "pref-default-opacity", value: 50.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-camera-path",
                items: &[
                    Item::Choice { label: "pref-show-these-layers", choices: &[("camera", "pref-2d-camera")], value: "camera", wire: Wire::None },
                    Item::Color { label: "pref-color-2", rgb: [0, 0, 0] },
                    Item::Number { label: "pref-opacity", value: 20.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                    Item::Check { label: "pref-show-field-guides", on: true, wire: Wire::None },
                    Item::Check { label: "pref-display-in-different-colors", on: false, wire: Wire::None },
                    Item::Color { label: "pref-first-color", rgb: [0, 255, 0] },
                    Item::Color { label: "pref-final-color", rgb: [255, 0, 0] },
                    Item::Check { label: "pref-show-lines", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-2d-camera",
                items: &[
                    Item::Color { label: "pref-color-2", rgb: [0, 0, 255] },
                    Item::Number { label: "pref-opacity", value: 80.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-show-all-field-guides",
                items: &[
                    Item::Color { label: "pref-color-2", rgb: [0, 128, 0] },
                    Item::Number { label: "pref-opacity", value: 30.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-ruler-unit",
        groups: &[
            Group {
                title: "pref-ruler-grid-crop-marks",
                items: &[
                    Item::Color { label: "pref-color-of-snap-enabled-rulers", rgb: [140, 0, 255] },
                    Item::Color { label: "pref-color-of-rulers-with-snapping-disabled", rgb: [0, 128, 0] },
                    Item::Color { label: "pref-color-of-grid-line", rgb: [128, 128, 128] },
                    Item::Color { label: "pref-color-of-grid-dividing-line", rgb: [210, 210, 210] },
                    Item::Color { label: "pref-color-of-crop-marks-inner-border", rgb: [0, 0, 128] },
                    Item::Color { label: "pref-safety-margin-color", rgb: [0, 192, 64] },
                    Item::Number { label: "pref-opacity-of-ruler-grid-crop-marks", value: 40.0, min: 0.0, max: 100.0, decimals: 0, unit: "%", wire: Wire::None },
                    Item::Check { label: "pref-when-snapping-to-perspective-ruler-reset-drawing", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-unit",
                items: &[
                    Item::Choice { label: "pref-unit-of-length", choices: &[("px", "pref-px"), ("mm", "pref-mm"), ("cm", "pref-cm"), ("in", "pref-in"), ("pt", "pref-pt")], value: "px", wire: Wire::None },
                    Item::Choice { label: "pref-text-unit", choices: &[("pt", "pref-pt"), ("px", "pref-px"), ("mm", "pref-mm")], value: "pt", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-canvas",
        groups: &[
            Group {
                title: "pref-view",
                items: &[
                    Item::Choice { label: "pref-display-quality", choices: &[("default", "pref-default"), ("smooth", "pref-smooth")], value: "default", wire: Wire::None },
                    Item::Color { label: "pref-transparent-color-1", rgb: [255, 255, 255] },
                    Item::Color { label: "pref-transparent-color-2", rgb: [204, 204, 204] },
                ],
            },
            Group {
                title: "pref-angle",
                items: &[
                    Item::Number { label: "pref-step", value: 15.0, min: 1.0, max: 90.0, decimals: 0, unit: "degrees", wire: Wire::Int("sue_paint.rotate_step", 1) },
                ],
            },
            Group {
                title: "pref-display-resolution",
                items: &[
                    Item::Note { text: "pref-not-set" },
                ],
            },
        ],
    },
    Page {
        title: "pref-file",
        groups: &[
            Group {
                title: "pref-auto-recovery",
                items: &[
                    Item::Number { label: "pref-save-recovery-data-every", value: 1.0, min: 0.0, max: 60.0, decimals: 0, unit: "minutes", wire: Wire::Int("general.autosave_seconds", 60) },
                    Item::Check { label: "pref-save-recovery-data-for-every-operation", on: false, wire: Wire::None },
                    Item::Note { text: "pref-sue-keeps-unsaved-work-this-often-and-the-hub-of" },
                ],
            },
            Group {
                title: "pref-save",
                items: &[
                    Item::Choice { label: "pref-save-format", choices: &[("clip", "pref-clip-studio-paint-clip"), ("sue", "pref-sue-project-sue")], value: "clip", wire: Wire::Text("sue_paint.save_format") },
                    Item::Check { label: "pref-save-files-as-a-background-process", on: true, wire: Wire::None },
                    Item::Check { label: "pref-show-save-status-as-pop-up", on: false, wire: Wire::None },
                    Item::Check { label: "pref-use-layer-cache-when-exporting", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-page-management",
                items: &[
                    Item::Check { label: "pref-auto-save-when-switching-page", on: false, wire: Wire::None },
                    Item::Check { label: "pref-always-open-in-a-new-tab-when-double-clicking-or", on: true, wire: Wire::None },
                ],
            },
            Group {
                title: "pref-png",
                items: &[
                    Item::Choice { label: "pref-png-compression-level", choices: &[("mid", "pref-mid-recommended")], value: "mid", wire: Wire::None },
                    Item::Note { text: "pref-the-compression-level-affects-the-file-size-and" },
                ],
            },
        ],
    },
    Page {
        title: "pref-color-conversion",
        groups: &[
            Group {
                title: "pref-color-conversion-settings",
                items: &[
                    Item::Choice { label: "pref-rgb-profile", choices: &[("srgb", "pref-srgb-iec61966-2-1")], value: "srgb", wire: Wire::None },
                    Item::Choice { label: "pref-cmyk-profile", choices: &[("japan", "pref-japan-color-2001-coated")], value: "japan", wire: Wire::None },
                    Item::Choice { label: "pref-rendering-intent", choices: &[("perceptual", "pref-perceptual")], value: "perceptual", wire: Wire::None },
                    Item::Choice { label: "pref-existing-profile-at-time-of-saving", choices: &[("prefs", "pref-preferences-rgb-profile")], value: "prefs", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-story-editor",
        groups: &[
            Group {
                title: "pref-new-text",
                items: &[
                    Item::Choice { label: "pref-how-to-insert-new-text", choices: &[("current", "pref-add-to-current-layer")], value: "current", wire: Wire::None },
                    Item::Choice { label: "pref-new-text-properties", choices: &[("copy", "pref-copy-from-current-text")], value: "copy", wire: Wire::None },
                    Item::Note { text: "pref-when-typing-with-the-story-editor-this-will-appl" },
                ],
            },
            Group {
                title: "pref-line-breaks-spacing",
                items: &[
                    Item::Choice { label: "pref-how-to-show-line-break", choices: &[("show", "pref-show-line-break")], value: "show", wire: Wire::None },
                    Item::Choice { label: "pref-paragraph-breaks", choices: &[("double", "pref-double-line-space")], value: "double", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-view",
                items: &[
                    Item::Radio { label: "pref-direction", choices: &[("horizontal", "pref-horizontal"), ("vertical", "pref-vertical")], value: "horizontal", wire: Wire::None },
                    Item::Number { label: "pref-body-page-font-size", value: 12.0, min: 4.0, max: 72.0, decimals: 2, unit: "pt", wire: Wire::None },
                    Item::Number { label: "pref-reading-font-size", value: 8.0, min: 4.0, max: 72.0, decimals: 2, unit: "pt", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-3d",
        groups: &[
            Group {
                title: "pref-model-settings",
                items: &[
                    Item::Choice { label: "pref-default-3d-figure", choices: &[("male", "pref-3d-drawing-figure-male")], value: "male", wire: Wire::None },
                    Item::Choice { label: "pref-hand-model-for-pose-materials", choices: &[("left", "pref-left-hand")], value: "left", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-rendering-settings",
                items: &[
                    Item::Number { label: "pref-threshold-for-semi-transparency", value: 80.0, min: 0.0, max: 100.0, decimals: 0, unit: "", wire: Wire::None },
                    Item::Check { label: "pref-use-multi-sampling", on: true, wire: Wire::None },
                    Item::Note { text: "pref-the-changes-will-take-effect-after-restarting-th" },
                ],
            },
            Group {
                title: "pref-new-3d-layer",
                items: &[
                    Item::Choice { label: "pref-shadow-appearance", choices: &[("ground", "pref-cast-on-ground-only")], value: "ground", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-import-settings-for-wavefront-obj-files",
                items: &[
                    Item::Choice { label: "pref-upper-axis", choices: &[("y", "pref-y-axis"), ("z", "pref-z-axis")], value: "y", wire: Wire::None },
                    Item::Choice { label: "pref-origin-point", choices: &[("keep", "pref-do-not-change")], value: "keep", wire: Wire::None },
                ],
            },
            Group {
                title: "pref-import-settings-for-3d-files",
                items: &[
                    Item::Choice { label: "pref-texture-import-settings", choices: &[("ask", "pref-check-import-settings-each-time")], value: "ask", wire: Wire::None },
                ],
            },
        ],
    },
    Page {
        title: "pref-smartphone-connection",
        groups: &[
            Group {
                title: "pref-main-device-settings",
                items: &[
                    Item::Number { label: "pref-port-no", value: 32035.0, min: 1024.0, max: 65535.0, decimals: 0, unit: "", wire: Wire::None },
                    Item::Note { text: "pref-after-changing-the-port-number-connect-to-the-sm" },
                ],
            },
        ],
    },
    Page {
        title: "pref-tabmate-2",
        groups: &[
            Group {
                title: "pref-scan-settings",
                items: &[
                    Item::Check { label: "pref-start-scanning-when-app-launches-or-is-active", on: true, wire: Wire::None },
                ],
            },
        ],
    },
];
