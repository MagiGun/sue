//! Each frame: the language, the layout for the device, the body, and
//! keeping the canvases in step with the arena.

use std::path::PathBuf;

use egui::{Rect, Ui};
use sue_core::txn::{Batch, SetProperty};
use sue_core::{Arena, EntityId};
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, AppletId, ArenaEdit};

use super::{Doc, Gesture, ID, KEYS_SECTION, SuePaint, shortcut_table};
use crate::dialogs::NewDialog;
use crate::doc::{self, NewPicture};

impl SuePaint {
    /// Opens a tab for every picture in the arena that has none (the newest
    /// becomes current), drops tabs whose picture is gone, and keeps every
    /// tab's current layer valid.
    pub(crate) fn sync_docs(&mut self, arena: &Arena) {
        let pictures: Vec<EntityId> = arena
            .iter()
            .filter(|(_, entity)| picture::is_picture(entity))
            .map(|(_, entity)| entity.id())
            .collect();
        let current = self.picture();
        let shown = self.docs.get(self.active).map(|doc| doc.picture);
        let before = self.docs.len();
        self.docs.retain(|doc| pictures.contains(&doc.picture));
        let kept_index = (self.docs.len() == before && shown == current).then_some(self.active);
        let mut added = None;
        for id in pictures {
            if !self.closed.contains(&id) && !self.docs.iter().any(|doc| doc.picture == id) {
                self.docs.push(Doc::new(id));
                added = Some(self.docs.len() - 1);
            }
        }
        self.active = added
            .or(kept_index)
            .or_else(|| current.and_then(|id| self.docs.iter().position(|doc| doc.picture == id)))
            .unwrap_or(0)
            .min(self.docs.len().saturating_sub(1));
        for doc in &mut self.docs {
            let stack = doc::layers(arena, doc.picture);
            if doc
                .layer
                .is_none_or(|layer| !stack.iter().any(|info| info.id == layer))
            {
                doc.layer = stack
                    .iter()
                    .rev()
                    .find(|info| info.kind == LayerKind::Raster)
                    .or(stack.last())
                    .map(|info| info.id);
            }
            if let Some(size) = arena.get_by_id(doc.picture).ok().and_then(picture::size)
                && doc
                    .selection
                    .as_ref()
                    .is_some_and(|selection| selection.size() != size)
            {
                doc.selection = None;
                doc.edges = None;
            }
        }
    }

    /// Creates a picture from the New dialog and makes it current.
    pub(crate) fn make_picture(&mut self, cx: &mut AppletContext<'_>, spec: &NewPicture) {
        match doc::create(spec) {
            Ok((edit, picture)) => {
                if Self::apply(cx, edit) {
                    self.activate(&cx.world.arena, picture);
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Makes `picture` the current tab, opening one if it has none.
    pub(crate) fn activate(&mut self, arena: &Arena, picture: EntityId) {
        self.closed.remove(&picture);
        self.sync_docs(arena);
        if let Some(index) = self.docs.iter().position(|doc| doc.picture == picture) {
            self.active = index;
        }
    }

    /// A New dialog filled from the rice.
    pub(crate) fn new_dialog(cx: &AppletContext<'_>) -> NewDialog {
        let rice = cx.rice();
        let mut spec = NewPicture::default();
        if let Some(width) = rice
            .get_i64("sue_paint.width")
            .and_then(|v| u32::try_from(v).ok())
        {
            spec.width = width;
        }
        if let Some(height) = rice
            .get_i64("sue_paint.height")
            .and_then(|v| u32::try_from(v).ok())
        {
            spec.height = height;
        }
        if let Some(dpi) = rice.get_i64("sue_paint.dpi") {
            spec.dpi = dpi as f64;
        }
        NewDialog::new(&spec)
    }

    /// Records what an unfinished gesture or slider already changed in
    /// place, so nothing escapes the history when the studio closes.
    pub(super) fn settle(&mut self, cx: &mut AppletContext<'_>) {
        match self.gesture.take() {
            Some(Gesture::Stroke { stroke, label, .. }) => {
                if let Some(raster) = doc::raster(&cx.world.arena, stroke.layer()) {
                    let edit = stroke.finish(raster, &label);
                    if !edit.is_empty() {
                        cx.record(ArenaEdit(edit));
                    }
                }
            }
            Some(Gesture::Move { layer, old, .. }) => {
                let mut steps = Batch::new(tr!("move-layer"));
                for (key, before) in [(picture::X, old[0].clone()), (picture::Y, old[1].clone())] {
                    steps = steps.then(match before {
                        Some(value) => SetProperty::new(layer, key, value),
                        None => SetProperty::remove(layer, key),
                    });
                }
                cx.record(ArenaEdit(steps));
            }
            _ => {}
        }
        if let Some(live) = self.live.take() {
            let label = cx
                .world
                .arena
                .get_by_id(live.id)
                .ok()
                .and_then(|entity| entity.property(&live.key).cloned())
                .map_or_else(
                    || tr!("set-2", key = live.key),
                    |value| doc::label(&live.key, &value),
                );
            let step = match live.old {
                Some(old) => SetProperty::new(live.id, live.key, old),
                None => SetProperty::remove(live.id, live.key),
            };
            cx.record(ArenaEdit(Batch::new(label).then(step)));
        }
    }

    /// Builds the menus again when the language changed (with the recent
    /// files and workspaces in them), and keeps their keys in step with the
    /// settings.
    pub(super) fn follow_language(&mut self, cx: &AppletContext<'_>) {
        if self.language.changed() {
            self.menus = crate::menus::menus();
            crate::menus::set_recent(
                &mut self.menus,
                crate::menus::recent_items(&self.recent_files),
            );
            self.refresh_workspace_menu();
            self.bound.invalidate();
        }
        self.bound
            .refresh(&mut self.menus, cx.rice(), KEYS_SECTION, shortcut_table());
    }

    pub(super) fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        self.pen_curve = cx
            .rice()
            .get_str(crate::dialogs::PEN_PRESSURE)
            .and_then(|text| text.parse().ok())
            .unwrap_or_default();
        let present = cx.view.page.as_ref().map(AppletId::as_str) == Some(ID);
        let reveal = cx.view.reveal.clamp(0.0, 1.0);
        self.hide = 1.0 - reveal;
        // The view's ground and picture turn opaque within the first third
        // of the way in.
        let cover = (reveal / 0.35).clamp(0.0, 1.0);
        self.cover = cover * cover * (3.0 - 2.0 * cover);
        self.ready = present && reveal >= 0.999;
        self.present = present;
        if present {
            self.shown = true;
        }
        self.sync_docs(&cx.world.arena);
        self.hub
            .forget_old_cache(cx.catalog.folders.get("cache").map(PathBuf::as_path));
        if present {
            // A material the Hub's Materials place put on the canvas.
            self.paste_from_hub(cx);
        }
        self.poll_scans(cx);
        self.animate(cx, &ui.ctx().clone());
        self.record_timelapse(cx, &ui.ctx().clone());
        if let Some(doc) = self.docs.get_mut(self.active) {
            doc.compositor.sync(&cx.world.arena);
        }
        let ctx = ui.ctx().clone();
        if self.ready {
            self.keys(&ctx, cx);
        }
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        self.interface = cx
            .rice()
            .get_str("display.interface")
            .unwrap_or("auto")
            .to_owned();
        let form = cx.view.form_factor;
        if self.laid_for != Some(form) {
            self.lay_out_for(form);
        }
        // Simple Mode, unless Studio Mode was chosen.
        self.bare = form != sue_platform::FormFactor::Desktop && !crate::simple::studio_mode(cx);
        if form != sue_platform::FormFactor::Desktop {
            if self.bare {
                self.simple_frame(ui, full, cx, &theme);
            } else {
                // Studio Mode on phones and tablets.
                self.studio_frame(ui, full, cx, &theme);
            }
            // New on a touch screen: the New canvas page over it all.
            self.new_canvas_ui(ui, full, cx, &theme);
            if self.ready {
                self.dialog_ui(&ctx, cx, &theme);
            }
            return;
        }
        // The New canvas page is a touch screen's; the desktop has the
        // New dialog.
        self.new_canvas.category = None;

        // The menus are in Sue's top bar (`bar_ui`); the command bar is a
        // palette, in the dock's top row until people move it. While the
        // studio comes in or goes out, every palette slides in from or out
        // to its edge of the window.
        self.body(ui, full, cx, &theme);
        if self.ready {
            self.dialog_ui(&ctx, cx, &theme);
        }
    }

    /// Puts the palettes where `form`'s layout wants them, once, when the
    /// studio first shows in it or the interface changes: a tablet keeps the
    /// tool strip and the layers docked and the rest as icons beside the
    /// canvas, ready to pop out over it.
    pub(super) fn lay_out_for(&mut self, form: sue_platform::FormFactor) {
        use crate::dock::Palette;
        self.laid_for = Some(form);
        self.sheet = None;
        match form {
            sue_platform::FormFactor::Tablet => {
                if self.desk_dock.is_none() {
                    self.desk_dock = Some(self.dock.clone());
                }
                self.dock = crate::dock::Dock::default();
                for palette in [
                    Palette::SubTool,
                    Palette::BrushSize,
                    Palette::Color,
                    Palette::ColorSet,
                    Palette::Navigator,
                ] {
                    self.dock.iconize(palette);
                }
            }
            // Back from a tablet: the desktop's layout as it was.
            sue_platform::FormFactor::Desktop => {
                if let Some(dock) = self.desk_dock.take() {
                    self.dock = dock;
                }
            }
            sue_platform::FormFactor::Mobile => {}
        }
    }

    /// Lays out the palettes around the canvas: docked, tabbed, collapsed
    /// or floating (`dock`), the timeline and other palettes under the
    /// canvas, and the canvas; and keeps the layout for the next run.
    pub(super) fn body(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &sue_wm::Theme,
    ) {
        let center = self.dock_ui(ui, body, cx, theme);
        self.canvas_area(ui, center, cx, theme);
        if self.ready {
            self.keep_layout(ui.ctx(), cx);
        }
    }
}
