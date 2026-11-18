//! The names the Layer palette still gives the icons of its four tabs.
//!
//! Sue Paint once drew its own icons here with the painter. Every
//! surface now draws from Sue's one icon set ([`sue_wm::icons`]), which
//! an icon pack can replace by name; what is left is this short list,
//! turned into the palette's icon of that set where the tab is drawn.
//! It goes when the Layer palette names its palettes' icons itself
//! ([`crate::dock::Palette::symbol`]), as every other palette does.

use crate::dock::Palette;

/// A tab of the Layer palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Icon {
    /// The layers.
    Layers,
    /// A layer's properties.
    LayerProperty,
    /// The history.
    History,
    /// The animation cels.
    Cel,
}

impl From<Icon> for sue_wm::icons::Icon {
    /// The icon of the palette the tab shows, from Sue's icon set.
    fn from(icon: Icon) -> Self {
        match icon {
            Icon::Layers => Palette::Layer,
            Icon::LayerProperty => Palette::LayerProperty,
            Icon::History => Palette::History,
            Icon::Cel => Palette::Cels,
        }
        .symbol()
    }
}
