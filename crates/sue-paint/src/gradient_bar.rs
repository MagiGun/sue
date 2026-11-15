//! Gradients as people edit them, shared by every place that offers one
//! (the Gradient Map dialog, the Gradient tool's Tool Property, a
//! gradient layer): the gradient sets kept as files
//! (`docs/gradient-sets.md`), the bar of stops with its swatches, and the
//! settings of the stop being edited.

use std::path::PathBuf;

use egui::{Rect, Sense, Ui, pos2, vec2};
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Radius, Space, TextRole, Tip};

use crate::gradient_map::{GradientMap, GradientSet, NamedGradient, SET_EXTENSION, StopColor};

/// The gradient sets: Sue's own, then each file in the data folder's
/// `gradient_sets` folder; which one is shown, the gradient picked in it
/// and the stop being edited.
#[derive(Clone, Debug, Default)]
pub(crate) struct GradientLibrary {
    loaded: bool,
    folder: Option<PathBuf>,
    /// Each set and the file it is kept in (none for Sue's own).
    pub sets: Vec<(Option<PathBuf>, GradientSet)>,
    pub set: usize,
    pub picked: Option<usize>,
    pub stop: usize,
    /// What went wrong writing a set, shown in the dialog.
    pub note: Option<String>,
}

/// The folder in Sue Paint's data folder that holds the gradient sets.
pub(crate) const SETS_FOLDER: &str = "gradient_sets";

impl GradientLibrary {
    /// Reads the sets once, from `data` (Sue Paint's data folder).
    pub(crate) fn load(&mut self, data: Option<PathBuf>) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        self.folder = data.map(|folder| folder.join(SETS_FOLDER));
        self.sets = vec![(None, GradientSet::builtin())];
        if let Some(folder) = &self.folder
            && let Ok(entries) = sue_platform::list_dir(folder, false)
        {
            let mut files: Vec<PathBuf> = entries
                .into_iter()
                .map(|entry| entry.path)
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == SET_EXTENSION)
                })
                .collect();
            files.sort();
            for path in files {
                if let Some(set) = sue_platform::fs::read_to_string(&path)
                    .ok()
                    .and_then(|text| GradientSet::from_text(&text))
                {
                    self.sets.push((Some(path), set));
                }
            }
        }
    }

    /// The set shown now.
    pub(crate) fn current(&self) -> Option<&(Option<PathBuf>, GradientSet)> {
        self.sets.get(self.set)
    }

    /// A set of people's own to add gradients to: the one shown, or (when
    /// Sue's own is shown) "My Gradients", made when it is missing.
    fn own_set(&mut self) -> Option<usize> {
        if self.current().is_some_and(|(file, _)| file.is_some()) {
            return Some(self.set);
        }
        if let Some(index) = self.sets.iter().position(|(file, _)| file.is_some()) {
            return Some(index);
        }
        let folder = self.folder.clone()?;
        let set = GradientSet {
            name: tr!("gradient-set-mine"),
            gradients: Vec::new(),
        };
        let path = folder.join(format!("my_gradients.{SET_EXTENSION}"));
        self.sets.push((Some(path), set));
        Some(self.sets.len() - 1)
    }

    /// Writes set `index` to its file.
    fn write(&mut self, index: usize) {
        let Some((Some(path), set)) = self.sets.get(index) else {
            return;
        };
        let written = path
            .parent()
            .map_or(Ok(()), sue_platform::fs::create_dir_all)
            .and_then(|()| sue_platform::fs::write(path, set.to_text()));
        self.note = written
            .err()
            .map(|error| tr!("gradient-set-not-written", error = error.to_string()));
    }

    /// Adds `map` as a new gradient named `name` to a set of people's own,
    /// picks it and writes the set.
    pub(crate) fn add(&mut self, name: String, map: GradientMap) {
        let Some(index) = self.own_set() else {
            self.note = Some(tr!("sue-paint-has-no-data-folder"));
            return;
        };
        self.sets[index]
            .1
            .gradients
            .push(NamedGradient { name, map });
        self.set = index;
        self.picked = Some(self.sets[index].1.gradients.len() - 1);
        self.write(index);
    }

    /// Takes the picked gradient out of its set (Sue's own set stays as it
    /// ships).
    pub(crate) fn delete(&mut self) {
        let (Some(picked), Some((Some(_), set))) = (self.picked, self.sets.get_mut(self.set))
        else {
            return;
        };
        if picked < set.gradients.len() {
            set.gradients.remove(picked);
            self.picked = None;
            self.write(self.set);
        }
    }

    /// Keeps `map` as the picked gradient of a set of people's own (on
    /// OK), writing the set when it changed.
    pub(crate) fn keep(&mut self, map: &GradientMap) {
        let (Some(picked), Some((Some(_), set))) = (self.picked, self.sets.get_mut(self.set))
        else {
            return;
        };
        if let Some(gradient) = set.gradients.get_mut(picked)
            && gradient.map != *map
        {
            gradient.map = *map;
            self.write(self.set);
        }
    }
}

/// `true` when `a` and `b` map every brightness to the same color and
/// opacity, wherever their stops' colors come from.
pub(crate) fn looks_alike(a: &GradientMap, b: &GradientMap) -> bool {
    a.mix == b.mix
        && a.stops().len() == b.stops().len()
        && a.stops()
            .iter()
            .zip(b.stops())
            .all(|(a, b)| a.at == b.at && a.rgb == b.rgb && a.alpha == b.alpha)
}

/// `map` painted across `rect`, over a checkerboard where it is see
/// through.
pub(crate) fn paint_gradient(
    painter: &egui::Painter,
    rect: Rect,
    map: &GradientMap,
    theme: &Theme,
) {
    let steps = (rect.width() / 2.0).ceil().max(1.0) as usize;
    let cell = rect.height() / 2.0;
    // The checkerboard under it.
    let (light, dark) = (theme.surface(0), theme.border_strong());
    let mut x = rect.left();
    let mut column = 0;
    while x < rect.right() {
        for row in 0..2 {
            let shade = if (column + row) % 2 == 0 { light } else { dark };
            let square = Rect::from_min_size(
                pos2(x, rect.top() + cell * row as f32),
                vec2(cell.min(rect.right() - x), cell),
            );
            painter.rect_filled(square, 0.0, shade);
        }
        x += cell;
        column += 1;
    }
    let width = rect.width() / steps as f32;
    for step in 0..steps {
        let t = (step as f32 + 0.5) / steps as f32;
        let [r, g, b, a] = map.raw_at(t);
        let byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
        let fill = egui::Color32::from_rgba_unmultiplied(byte(r), byte(g), byte(b), byte(a));
        painter.rect_filled(
            Rect::from_min_size(
                pos2(rect.left() + width * step as f32, rect.top()),
                vec2(width + 0.5, rect.height()),
            ),
            0.0,
            fill,
        );
    }
    painter.rect_stroke(rect, 0.0, theme.hairline(), egui::StrokeKind::Inside);
}

/// The bar of stops: a click on the bar adds a stop there, a stop is
/// picked by a click and moved by a drag.
pub(crate) fn gradient_bar(ui: &mut Ui, map: &mut GradientMap, stop: &mut usize, theme: &Theme) {
    let density = Density::of(ui);
    let height = density.control_height();
    let (rect, response) = ui.allocate_exact_size(
        vec2(ui.available_width(), height + theme.space(Space::S6)),
        Sense::click_and_drag(),
    );
    let bar = Rect::from_min_max(rect.min, pos2(rect.right(), rect.top() + height))
        .shrink2(egui::Vec2::X * theme.space(Space::S3));
    paint_gradient(ui.painter(), bar, map, theme);
    let handle = theme.space(Space::S3);
    let place = |at: f32| pos2(bar.left() + bar.width() * at, bar.bottom() + handle);
    // The stop under the pointer when it went down, if any.
    let near = |pointer: egui::Pos2, map: &GradientMap| {
        map.stops()
            .iter()
            .enumerate()
            .map(|(index, found)| (index, place(found.at).distance(pointer)))
            .filter(|(_, distance)| *distance <= handle * 2.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    };
    let at_of =
        |pointer: egui::Pos2| ((pointer.x - bar.left()) / bar.width().max(1.0)).clamp(0.0, 1.0);
    if response.drag_started()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(index) = near(pointer, map)
    {
        *stop = index;
    }
    if response.dragged()
        && let Some(pointer) = response.interact_pointer_pos()
        && let Some(moving) = map.stop_mut(*stop)
    {
        moving.at = at_of(pointer);
        *stop = map.sort_keeping(*stop);
    }
    if response.clicked()
        && let Some(pointer) = response.interact_pointer_pos()
    {
        match near(pointer, map) {
            Some(index) => *stop = index,
            None => {
                if let Some(index) = map.add_at(at_of(pointer)) {
                    *stop = index;
                }
            }
        }
    }
    // A stop picked, moved or added with the pointer takes the keys
    // next, so Delete right after a click takes that stop away.
    if response.clicked() || response.drag_started() {
        response.request_focus();
    }
    // Keys: Left and Right move the stop being edited, Delete (or
    // Backspace) takes it away while more than two are left.
    if response.has_focus() {
        let (left, right, delete) = ui.input(|input| {
            (
                input.key_pressed(egui::Key::ArrowLeft),
                input.key_pressed(egui::Key::ArrowRight),
                input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace),
            )
        });
        if delete && map.remove(*stop) {
            *stop = stop.saturating_sub(1);
        }
        let shift = if right {
            0.01
        } else if left {
            -0.01
        } else {
            0.0
        };
        if shift != 0.0
            && let Some(moving) = map.stop_mut(*stop)
        {
            moving.at = (moving.at + shift).clamp(0.0, 1.0);
            *stop = map.sort_keeping(*stop);
        }
    }
    // The stops: small swatches under the bar, the edited one ringed.
    for (index, found) in map.stops().iter().enumerate() {
        let center = place(found.at);
        let swatch = Rect::from_center_size(center, egui::Vec2::splat(theme.space(Space::S5)));
        sue_wm::swatch(
            ui,
            swatch,
            [found.rgb[0], found.rgb[1], found.rgb[2], found.alpha],
            theme,
        );
        let edge = if index == *stop {
            theme.selected_stroke()
        } else {
            theme.hairline()
        };
        ui.painter().rect_stroke(
            swatch,
            theme.radius(Radius::Xs),
            edge,
            egui::StrokeKind::Outside,
        );
        ui.painter().line_segment(
            [pos2(center.x, bar.bottom()), pos2(center.x, swatch.top())],
            theme.hairline(),
        );
    }
    let name = tr!("gradient-stops");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Slider, true, &name));
    ui::focus_ring(ui, &response, theme.radius(Radius::Xs), theme);
    ui::tooltip(
        response,
        &Tip::new(&name).help(&tr!("gradient-stops-help")),
        theme,
    );
}

/// The stop being edited: its place, where its color comes from, its
/// color and opacity, and Delete. `colors` are the main, sub and drawing
/// colors a stop takes when it follows one.
pub(crate) fn stop_ui(
    ui: &mut Ui,
    map: &mut GradientMap,
    stop: &mut usize,
    colors: [[u8; 3]; 3],
    theme: &Theme,
) {
    let count = map.stops().len();
    let Some(edited) = map.stop_mut(*stop) else {
        return;
    };
    let mut place = edited.at * 100.0;
    let moved = number(
        ui,
        "stop-place",
        &tr!("stop-position"),
        &mut place,
        0.0..=100.0,
        0,
        theme,
    );
    if moved {
        edited.at = place / 100.0;
    }
    let sources = [
        tr!("stop-own-color"),
        tr!("stop-main-color"),
        tr!("stop-sub-color"),
        tr!("stop-drawing-color"),
    ];
    let mut source = StopColor::ALL
        .iter()
        .position(|kind| *kind == edited.color)
        .unwrap_or(0);
    let from = tr!("stop-color-from");
    let picked = ui::input::dropdown("sue-paint-stop-source", &mut source, &sources)
        .label(&from)
        .width(ui.available_width())
        .show(ui, theme);
    let changed = picked.changed();
    ui::tooltip(picked, &Tip::new(&from), theme);
    if changed {
        edited.color = StopColor::ALL[source.min(StopColor::ALL.len() - 1)];
        match edited.color {
            StopColor::Own => {}
            StopColor::Main => edited.rgb = colors[0],
            StopColor::Sub => edited.rgb = colors[1],
            StopColor::Drawing => edited.rgb = colors[2],
        }
    }
    ui.add_enabled_ui(edited.color == StopColor::Own, |ui| {
        color(
            ui,
            "stop-color",
            &tr!("filter-color"),
            &mut edited.rgb,
            theme,
        );
    });
    let mut opacity = f32::from(edited.alpha) / 255.0 * 100.0;
    if number(
        ui,
        "stop-opacity",
        &tr!("opacity"),
        &mut opacity,
        0.0..=100.0,
        0,
        theme,
    ) {
        edited.alpha = (opacity / 100.0 * 255.0).round() as u8;
    }
    if moved {
        *stop = map.sort_keeping(*stop);
    }
    let delete = tr!("stop-delete");
    let response = ui::button("sue-paint-stop-delete", &delete)
        .ghost()
        .icon(Icon::Delete)
        .enabled(count > 2)
        .show(ui, theme);
    if response.clicked() && map.remove(*stop) {
        *stop = stop.saturating_sub(1);
    }
    ui::tooltip(response, &Tip::new(&delete).shortcut("Delete"), theme);
}

// ----- The stop's controls --------------------------------------------------------

/// The room a slider's name takes, so the tracks line up.
fn label_room(theme: &Theme) -> f32 {
    theme.space(Space::S9) * 3.0
}

/// A slider for `value` with its name before it and in its tooltip.
fn number(
    ui: &mut Ui,
    salt: &str,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    decimals: usize,
    theme: &Theme,
) -> bool {
    let mut wide = f64::from(*value);
    let response = ui::slider(
        ("sue-paint-filter", salt),
        &mut wide,
        f64::from(*range.start())..=f64::from(*range.end()),
    )
    .label(label)
    .label_width(label_room(theme))
    .decimals(decimals)
    .show(ui, theme);
    let changed = response.changed();
    ui::tooltip(response, &Tip::new(label), theme);
    if changed {
        *value = wide as f32;
    }
    changed
}

/// A color swatch that opens the color wheel, with its name beside it.
fn color(ui: &mut Ui, salt: &str, label: &str, rgb: &mut [u8; 3], theme: &Theme) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut rgba = [rgb[0], rgb[1], rgb[2], 255];
        let pick = sue_wm::color_button(ui, ("sue-paint-filter", salt), &mut rgba, false, theme);
        if pick.changed {
            *rgb = [rgba[0], rgba[1], rgba[2]];
            changed = true;
        }
        ui.label(
            egui::RichText::new(label)
                .font(theme.text_font(TextRole::Body))
                .color(theme.text),
        );
    });
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gradient_map::Stop;

    #[test]
    fn gradient_sets_are_files_people_add_to_and_take_from() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let data = std::env::temp_dir().join(format!("sue-gradient-sets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut library = GradientLibrary::default();
        library.load(Some(data.clone()));
        assert_eq!(library.sets.len(), 1, "Sue's own set only");
        // Adding on Sue's own set makes My Gradients and writes it.
        let map = GradientMap::of(&[
            Stop::new(0.0, [1, 2, 3]),
            Stop::new(0.3, [200, 10, 10]),
            Stop::new(1.0, [250, 250, 250]),
        ])
        .unwrap();
        library.add("Mine".to_owned(), map);
        assert_eq!(library.note, None);
        assert_eq!(library.sets.len(), 2);
        assert_eq!((library.set, library.picked), (1, Some(0)));
        let file = data
            .join(SETS_FOLDER)
            .join(format!("my_gradients.{SET_EXTENSION}"));
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("gradient = Mine = "), "{text}");
        // Read again by a fresh library: the same set.
        let mut again = GradientLibrary::default();
        again.load(Some(data.clone()));
        assert_eq!(again.sets.len(), 2);
        assert_eq!(again.sets[1].1.gradients[0].map, map);
        // OK on an edited gradient keeps it; Delete takes it out.
        let mut edited = map;
        edited.stop_mut(1).unwrap().rgb = [0, 255, 0];
        library.keep(&edited);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("00ff00ff"), "{text}");
        library.delete();
        assert!(library.sets[1].1.gradients.is_empty());
        assert_eq!(library.picked, None);
        // Sue's own set is never changed.
        library.set = 0;
        library.picked = Some(0);
        let shipped = library.sets[0].1.clone();
        library.delete();
        library.keep(&map);
        assert_eq!(library.sets[0].1, shipped);
        let _ = std::fs::remove_dir_all(&data);
    }
}
