//! Canvases in windows of their own, and pictures dropped on the canvas.
//!
//! A canvas tab dragged out of the tab strip (or Window, Canvas, Canvas
//! in Its Own Window) opens in a window of its own, which the system lets
//! people move to any screen; where the system gives Sue one window it
//! floats over the page instead (`ShellRequest::OpenView`). The canvas
//! stays one of Sue Paint's open canvases: saved, closed and listed in
//! the Window menu as before, with its own view. A press in its window
//! makes it the current canvas, so the palettes work on it; a press in
//! Sue Paint's column makes the canvas there current again. Closing the
//! window puts the canvas back in the tabs.
//!
//! A picture file dropped on the canvas becomes a new layer of that
//! canvas; dropped on the tab strip, or anywhere while no canvas is open,
//! it opens as a canvas of its own.

use std::path::PathBuf;

use egui::{Rect, Ui};
use sue_wm::{AppletContext, ShellRequest, Theme};

use super::Column;
use crate::studio::{SuePaint, me_id};

/// What the canvas chrome keeps between frames.
#[derive(Debug, Default)]
pub(crate) struct Chrome {
    /// The number the next canvas window is named with.
    next_window: u64,
    /// The canvas windows that are open, by number, with where their
    /// view was drawn last.
    windows: Vec<(u64, Rect)>,
    /// The canvas the column shows while the current one is in a window
    /// of its own.
    pub(super) main: usize,
    /// Picture files dropped on a canvas this frame, with that canvas:
    /// when they come back as files handed to Sue Paint they become its
    /// layers.
    drops: Vec<(PathBuf, usize)>,
    /// How many pictures the drop noted in `drops` held.
    dropped: usize,
    /// The right-click menu of a tab: which canvas it is for, where it
    /// opened, and the menu.
    pub(super) tab_menu: Option<(usize, egui::Pos2)>,
    pub(super) menu: sue_wm::ui::menu::MenuState,
}

/// What the view of canvas window `id` is called: `canvas:<id>`.
fn view_name(id: u64) -> String {
    format!("canvas:{id}")
}

/// The number of the canvas window whose view is called `view`.
fn window_of(view: &str) -> Option<u64> {
    view.strip_prefix("canvas:")?.parse().ok()
}

/// `true` when a button went down inside `area` this frame.
fn pressed_in(ui: &Ui, area: Rect) -> bool {
    ui.input(|input| {
        input.pointer.any_pressed()
            && input
                .pointer
                .press_origin()
                .is_some_and(|at| area.contains(at))
    })
}

impl SuePaint {
    /// The canvases shown in the column's tabs (not in windows of their
    /// own), by their place among all the open canvases.
    pub(super) fn main_docs(&self) -> Vec<usize> {
        (0..self.docs.len())
            .filter(|&index| self.docs[index].window.is_none())
            .collect()
    }

    /// The canvas the column shows: the current one, or, while that is in
    /// a window of its own, the one the column showed last (`None` when
    /// every open canvas is in a window).
    pub(super) fn column_doc(&mut self) -> Option<usize> {
        let in_column = |paint: &Self, index: usize| {
            paint
                .docs
                .get(index)
                .is_some_and(|doc| doc.window.is_none())
        };
        if in_column(self, self.active) {
            self.chrome.main = self.active;
            return Some(self.active);
        }
        if in_column(self, self.chrome.main) {
            return Some(self.chrome.main);
        }
        let first = self.main_docs().first().copied()?;
        self.chrome.main = first;
        Some(first)
    }

    /// Runs `f` with canvas `index` current, then makes the canvas that
    /// was current current again, unless a button went down in `area`
    /// (the person works on this canvas now) or `f` changed the canvases.
    fn with_doc(
        &mut self,
        index: usize,
        area: Rect,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        f: impl FnOnce(&mut Self, &mut Ui, &mut AppletContext<'_>),
    ) {
        let (active, count) = (self.active, self.docs.len());
        self.active = index;
        if let Some(doc) = self.docs.get_mut(index) {
            doc.compositor.sync(&cx.world.arena);
        }
        f(self, ui, cx);
        let took = pressed_in(ui, area);
        if !took && self.active == index && self.docs.len() == count {
            self.active = active;
        }
    }

    /// Draws the column for canvas `index`, which is not the current one.
    pub(super) fn column_for(
        &mut self,
        index: usize,
        ui: &mut Ui,
        area: Rect,
        slide: f32,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        self.with_doc(index, area, ui, cx, |paint, ui, cx| {
            paint.canvas_column(ui, area, slide, Column::Main, cx, theme);
        });
    }

    /// While the current canvas is in a window of its own, the view the
    /// commands of the menus and the palettes work in is that window's.
    pub(super) fn keep_window_viewport(&mut self) {
        let Some(id) = self.doc().and_then(|doc| doc.window) else {
            return;
        };
        if let Some((_, viewport)) = self
            .chrome
            .windows
            .iter()
            .find(|(open, _)| *open == id)
            .filter(|(_, viewport)| viewport.is_positive())
        {
            self.viewport = *viewport;
        }
    }

    /// Closes the windows of canvases that were closed, and forgets a
    /// window whose canvas is gone.
    pub(super) fn keep_windows(&mut self, cx: &mut AppletContext<'_>) {
        let open: Vec<u64> = self.docs.iter().filter_map(|doc| doc.window).collect();
        let gone: Vec<u64> = self
            .chrome
            .windows
            .iter()
            .map(|(id, _)| *id)
            .filter(|id| !open.contains(id))
            .collect();
        for id in gone {
            self.chrome.windows.retain(|(open, _)| *open != id);
            cx.request(ShellRequest::CloseView {
                applet: me_id(),
                view: view_name(id),
            });
        }
        for doc in &mut self.docs {
            if doc
                .window
                .is_some_and(|id| !self.chrome.windows.iter().any(|(open, _)| *open == id))
            {
                doc.window = None;
            }
        }
    }

    /// The name of canvas `index`, as its tab and its window read it.
    pub(super) fn doc_name(&self, index: usize, cx: &AppletContext<'_>) -> String {
        self.docs
            .get(index)
            .and_then(|doc| cx.world.arena.get_by_id(doc.picture).ok())
            .map_or_else(|| tr!("picture"), |entity| entity.name().to_owned())
    }

    /// Moves canvas `index` into a window of its own and makes it current;
    /// the column shows the canvas beside it. A canvas in a window already
    /// has its window brought to the front.
    pub(crate) fn float_canvas(&mut self, index: usize, cx: &mut AppletContext<'_>) {
        let Some(doc) = self.docs.get(index) else {
            return;
        };
        let title = self.doc_name(index, cx);
        let id = match doc.window {
            Some(id) => id,
            None => {
                let id = self.chrome.next_window;
                self.chrome.next_window += 1;
                self.chrome.windows.push((id, Rect::NOTHING));
                // The column shows the nearest canvas left in the tabs.
                let tabs = self.main_docs();
                if let Some(next) = tabs
                    .iter()
                    .copied()
                    .filter(|&other| other != index)
                    .min_by_key(|&other| other.abs_diff(index))
                {
                    self.chrome.main = next;
                }
                self.docs[index].window = Some(id);
                id
            }
        };
        self.active = index;
        cx.request(ShellRequest::OpenView {
            applet: me_id(),
            view: view_name(id),
            title,
        });
    }

    /// Puts the canvas of window `id` back in the tabs and makes it
    /// current; `close` closes its window too (when the window did not
    /// close by itself).
    pub(crate) fn dock_canvas(&mut self, id: u64, close: bool, cx: &mut AppletContext<'_>) {
        if let Some(index) = self.docs.iter().position(|doc| doc.window == Some(id)) {
            self.docs[index].window = None;
            self.active = index;
        }
        self.chrome.windows.retain(|(open, _)| *open != id);
        if close {
            cx.request(ShellRequest::CloseView {
                applet: me_id(),
                view: view_name(id),
            });
        }
    }

    /// Window, Canvas, Canvas in Its Own Window: the current canvas goes
    /// into a window of its own, or back into the tabs from one.
    pub(crate) fn toggle_canvas_window(&mut self, cx: &mut AppletContext<'_>) {
        match self.doc().map(|doc| doc.window) {
            Some(Some(id)) => self.dock_canvas(id, true, cx),
            Some(None) => self.float_canvas(self.active, cx),
            None => {}
        }
    }

    /// The view `view` of Sue Paint in a window of its own: a canvas
    /// window's canvas, with its status strip and scroll bars.
    pub(crate) fn canvas_window_ui(&mut self, view: &str, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let Some(id) = window_of(view) else {
            return;
        };
        let Some(index) = self.docs.iter().position(|doc| doc.window == Some(id)) else {
            // Its canvas closed: so does the window.
            self.dock_canvas(id, true, cx);
            return;
        };
        let theme = cx.theme.clone();
        let area = ui.max_rect();
        // The window shows its canvas whole whatever Sue Paint's page
        // does: on the Hub, or coming in.
        let kept = (self.hide, self.cover, self.ready, self.viewport);
        (self.hide, self.cover, self.ready) = (0.0, 1.0, true);
        let dropped = dropped_files(ui);
        // A window that floats over the page is drawn in Sue's main window,
        // which hands what is dropped on it to Sue Paint by itself: the
        // window only says the drop was on its canvas, when it was.
        let floating = ui.ctx().viewport_id() == egui::ViewportId::ROOT;
        if floating {
            let over = ui
                .input(|input| input.pointer.hover_pos())
                .is_some_and(|at| area.contains(at));
            if over {
                for path in dropped.iter().filter(|path| crate::studio::opens(path)) {
                    self.chrome.drops.retain(|(seen, _)| seen != path);
                    self.chrome.drops.push((path.clone(), index));
                }
                self.chrome.dropped = self.chrome.drops.len();
            }
        }
        let dropped = if floating { Vec::new() } else { dropped };
        self.with_doc(index, area, ui, cx, |paint, ui, cx| {
            paint.canvas_column(ui, area, 0.0, Column::Window, cx, &theme);
            let viewport = paint.viewport;
            if let Some(entry) = paint
                .chrome
                .windows
                .iter_mut()
                .find(|(open, _)| *open == id)
            {
                entry.1 = viewport;
            }
            // Pictures dropped on the window become its canvas's layers;
            // anything else goes where a drop on Sue goes.
            for path in &dropped {
                if crate::studio::opens(path) {
                    paint.open_picked(cx, path, true);
                } else {
                    cx.request(ShellRequest::OpenFile(path.clone()));
                }
            }
        });
        if !dropped.is_empty()
            && self
                .docs
                .get(index)
                .is_some_and(|doc| doc.window == Some(id))
        {
            self.active = index;
        }
        (self.hide, self.cover, self.ready) = (kept.0, kept.1, kept.2);
        if self.doc().is_none_or(|doc| doc.window != Some(id)) {
            self.viewport = kept.3;
        }
    }

    /// The window of view `view` closed: its canvas goes back in the tabs.
    pub(crate) fn canvas_window_closed(&mut self, view: &str, cx: &mut AppletContext<'_>) {
        if let Some(id) = window_of(view) {
            self.dock_canvas(id, false, cx);
        }
    }

    /// What the column shows while every open canvas is in a window of its
    /// own: where they went, and a button that brings them back.
    pub(super) fn all_in_windows(
        &mut self,
        ui: &mut Ui,
        viewport: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut inner = crate::panels::child(ui, viewport, "all-in-windows");
        inner.multiply_opacity(self.cover);
        if !self.ready {
            inner.disable();
        }
        let title = tr!("canvases-in-windows");
        let detail = tr!("canvases-in-windows-detail");
        let back = tr!("bring-canvases-back");
        let mut bring = false;
        sue_wm::ui::empty_state("sue-paint-all-in-windows", &title)
            .icon(sue_wm::icons::Icon::External)
            .detail(&detail)
            .show_actions(&mut inner, theme, |ui| {
                bring = sue_wm::ui::button("sue-paint-bring-back", &back)
                    .primary()
                    .show(ui, theme)
                    .clicked();
            });
        if bring {
            let ids: Vec<u64> = self.chrome.windows.iter().map(|(id, _)| *id).collect();
            for id in ids {
                self.dock_canvas(id, true, cx);
            }
        }
    }

    /// Notes the picture files dropped on Sue Paint's column this frame,
    /// unless they were dropped on the tab strip `tabs` or no canvas is
    /// open: those become layers of the canvas the column shows, which
    /// becomes current.
    pub(super) fn take_drops(&mut self, ui: &Ui, tabs: Rect, cx: &AppletContext<'_>) {
        self.chrome.drops.clear();
        let dropped = dropped_files(ui);
        if dropped.is_empty() || !self.present {
            return;
        }
        let over_tabs = ui
            .input(|input| input.pointer.hover_pos())
            .is_some_and(|at| tabs.contains(at));
        let Some(index) = self.column_doc() else {
            return;
        };
        let canvas = self
            .docs
            .get(index)
            .is_some_and(|doc| cx.world.arena.get_by_id(doc.picture).is_ok());
        if over_tabs || !canvas {
            return;
        }
        let pictures: Vec<PathBuf> = dropped
            .into_iter()
            .filter(|path| crate::studio::opens(path))
            .collect();
        self.chrome.drops = pictures.into_iter().map(|path| (path, index)).collect();
        self.chrome.dropped = self.chrome.drops.len();
    }

    /// `true` when the drop a picture just came from held several.
    pub(crate) fn dropped_together(&self) -> bool {
        self.chrome.dropped > 1
    }

    /// `true` once for a file that was dropped on a canvas this frame,
    /// which then becomes current and takes the file as a layer rather
    /// than it opening as a canvas of its own.
    pub(crate) fn take_canvas_drop(&mut self, path: &std::path::Path) -> bool {
        match self
            .chrome
            .drops
            .iter()
            .position(|(dropped, _)| dropped == path)
        {
            Some(at) => {
                let (_, index) = self.chrome.drops.remove(at);
                if index < self.docs.len() {
                    self.active = index;
                }
                true
            }
            None => false,
        }
    }
}

/// The files dropped on the window `ui` is in this frame.
fn dropped_files(ui: &Ui) -> Vec<PathBuf> {
    ui.input(|input| {
        input
            .raw
            .dropped_files
            .iter()
            .map(|file| file.path().to_path_buf())
            .filter(|path| !path.as_os_str().is_empty())
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::{view_name, window_of};

    #[test]
    fn a_canvas_windows_view_is_named_by_its_number() {
        assert_eq!(view_name(7), "canvas:7");
        assert_eq!(window_of(&view_name(7)), Some(7));
        assert_eq!(window_of("canvas:"), None);
        assert_eq!(window_of("navigator"), None);
    }
}
