//! Search Layer: finding layers by name and by kind, in the Layer palette.
//!
//! Clip Studio Paint keeps this in a palette of its own. Sue puts it at the
//! top of the Layer palette, behind the search button, so the list people
//! search is the list they act on: a click on a result picks that layer
//! in the very same rows, with every button above them. The kinds are the
//! ones of Clip Studio Paint's filter list, in its order.

use egui::{Rect, Ui, pos2};
use sue_core::{Arena, EntityId, Value};
use sue_model::class;
use sue_wm::Theme;
use sue_wm::ui::tokens::{Density, Space};

use crate::doc;

/// What the search looks for: words in the name and a kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SearchLayer {
    /// Words the name has (any case); empty for any name.
    pub text: String,
    /// The kind, an index of [`KINDS`] (0: all).
    pub kind: usize,
}

/// The kinds the filter list offers, in order: the message of each.
pub(crate) const KINDS: [&str; 15] = [
    "search-kind-all",
    "search-kind-raster",
    "search-kind-raster-color",
    "search-kind-raster-gray",
    "search-kind-raster-mono",
    "search-kind-vector",
    "search-kind-vector-color",
    "search-kind-vector-gray",
    "search-kind-vector-mono",
    "search-kind-correction",
    "search-kind-gradient",
    "search-kind-fill",
    "search-kind-text",
    "search-kind-image",
    "search-kind-lines",
];

/// The 3D kind goes last, after the kinds above.
pub(crate) const KIND_3D: &str = "search-kind-3d";

/// Every kind's name as people read it, in the filter list's order.
pub(crate) fn kind_names() -> Vec<String> {
    KINDS
        .iter()
        .chain(std::iter::once(&KIND_3D))
        .map(|key| sue_lang::text("paint", key))
        .collect()
}

/// The expression color of `entity`: 0 color, 1 gray, 2 monochrome.
fn expression(entity: &sue_core::Entity) -> i64 {
    entity
        .property(crate::effects::EXPRESSION)
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

impl SearchLayer {
    /// `true` when it looks for anything at all.
    pub(crate) fn is_active(&self) -> bool {
        !self.text.trim().is_empty() || self.kind != 0
    }

    /// `true` if `layer` is what the search looks for.
    pub(crate) fn matches(&self, arena: &Arena, layer: EntityId) -> bool {
        let Ok(entity) = arena.get_by_id(layer) else {
            return false;
        };
        let words = self.text.trim().to_lowercase();
        if !words.is_empty()
            && !words
                .split_whitespace()
                .all(|word| entity.name().to_lowercase().contains(word))
        {
            return false;
        }
        let kind = entity.class();
        let vector = crate::vector::is_vector(arena, layer);
        let raster = kind == class::LAYER
            && !vector
            && !doc::is_folder(arena, layer)
            && doc::raster(arena, layer).is_some();
        match self.kind {
            0 => true,
            1 => raster,
            2..=4 => raster && expression(entity) == self.kind as i64 - 2,
            5 => vector,
            6..=8 => vector && expression(entity) == self.kind as i64 - 6,
            9 => kind == doc::CORRECTION,
            10 => kind == doc::GRADIENT_LAYER,
            11 => kind == doc::FILL_LAYER,
            12 => kind == doc::TEXT || kind == crate::manga::BALLOON,
            13 => kind == doc::MATERIAL_LAYER,
            14 => kind == crate::manga::EFFECT,
            _ => crate::three::is_3d(arena, layer),
        }
    }
}

impl crate::studio::SuePaint {
    /// The search bar over the layer list, in `area`: the words and the
    /// kind. Returns how much of `area` it took from the top.
    pub(super) fn search_layer_bar(&mut self, ui: &mut Ui, area: Rect, theme: &Theme) -> f32 {
        let Some(search) = self.layer_palette.search.as_mut() else {
            return 0.0;
        };
        let density = Density::Compact;
        let gap = theme.space(Space::S2);
        let height = density.control_height();
        let kinds = kind_names();
        let split = area.left() + area.width() * 0.55;
        let words = Rect::from_min_max(
            pos2(area.left() + gap, area.top() + gap),
            pos2(split - gap, area.top() + gap + height),
        );
        let kind = Rect::from_min_max(
            pos2(split, area.top() + gap),
            pos2(area.right() - gap, area.top() + gap + height),
        );
        density.scope(ui, |ui| {
            let hint = tr!("search-layer-hint");
            sue_wm::ui::search_field("sue-paint-search-layer", &mut search.text)
                .hint(&hint)
                .label(&hint)
                .show_at(ui, words, theme);
            let label = tr!("search-layer-kind");
            sue_wm::ui::dropdown("sue-paint-search-kind", &mut search.kind, &kinds)
                .label(&label)
                .show_at(ui, kind, theme);
        });
        height + gap * 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::NewPicture;
    use sue_core::ReversibleTransaction;

    #[test]
    fn the_search_finds_layers_by_words_and_by_kind() {
        let mut arena = Arena::new();
        let (mut made, picture) = doc::create(&NewPicture {
            width: 4,
            height: 4,
            ..NewPicture::default()
        })
        .unwrap();
        made.apply(&mut arena).unwrap();
        let layer = doc::layers(&arena, picture).last().unwrap().id;
        let (mut grouped, folder) = doc::new_folder(&arena, picture, Some(layer), false);
        grouped.apply(&mut arena).unwrap();
        let name = arena.get_by_id(layer).unwrap().name().to_owned();
        let find = |text: &str, kind: usize| SearchLayer {
            text: text.to_owned(),
            kind,
        };
        assert!(!SearchLayer::default().is_active());
        assert!(find(&name.to_uppercase(), 0).matches(&arena, layer));
        assert!(!find("no such words", 0).matches(&arena, layer));
        assert!(find("", 1).matches(&arena, layer), "a raster layer");
        assert!(find("", 2).matches(&arena, layer), "in color");
        assert!(!find("", 3).matches(&arena, layer), "not gray");
        assert!(
            !find("", 1).matches(&arena, folder),
            "a folder is no raster"
        );
        assert!(!find("", 5).matches(&arena, layer), "no vector");
        assert_eq!(kind_names().len(), KINDS.len() + 1);
    }
}
