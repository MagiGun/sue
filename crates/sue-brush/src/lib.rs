//! # sue-brush: the brush engine
//!
//! One engine for every studio that paints. It takes pen samples and a
//! brush ([`sue_model::BrushPreset`], the component `sue.brush`) and
//! lays paint into the rasters it is handed. It has no interface, no
//! platform code and no document: it depends on `sue-core` and
//! `sue-model` only, so a stroke replays the same in a test without a
//! window, on a worker thread and in the browser.
//!
//! The engine is built module by module; each package of the 0.0.8 plan
//! adds its own file and its line to the list below.
