//! Layer comps (Clip Studio Paint EX): named ways a picture's layers can
//! show, each layer's visibility, opacity and blend mode, kept on the
//! picture so one picture holds several versions (with and without the
//! background, day and night) and each can be shown again or exported.
//!
//! A picture keeps its comps as text in [`COMPS`]: one comp a line, its
//! name, a tab, then each layer as `id,visible,opacity,blend` apart by `;`.
//! Layers are named by their `EntityId`, so a comp still knows them after
//! they move; a layer deleted since is passed over, and a layer added
//! since stays as it is.

use sue_core::txn::{Batch, SetProperty};
use sue_core::{Arena, EntityId, Value};
use sue_model::picture;

/// A picture's layer comps (`Text`).
pub const COMPS: &str = "picture.layer_comps";

type Edit = Batch<Arena>;

/// How one layer shows in a comp.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerState {
    /// The layer.
    pub layer: EntityId,
    /// It shows.
    pub visible: bool,
    /// 0 to 1.
    pub opacity: f32,
    /// Its blend mode's name.
    pub blend: String,
}

/// A layer comp: a name and how each layer shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Comp {
    /// What people call it.
    pub name: String,
    /// Every layer as it was when the comp was made or updated.
    pub states: Vec<LayerState>,
}

/// `picture`'s comps, in the order they were made.
#[must_use]
pub fn read(arena: &Arena, picture: EntityId) -> Vec<Comp> {
    arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(COMPS))
        .and_then(Value::as_str)
        .map(decode)
        .unwrap_or_default()
}

/// The comps as text (see the module docs).
#[must_use]
pub fn encode(comps: &[Comp]) -> String {
    comps
        .iter()
        .map(|comp| {
            let name: String = comp
                .name
                .chars()
                .map(|ch| {
                    if matches!(ch, '\t' | '\n' | '\r') {
                        ' '
                    } else {
                        ch
                    }
                })
                .collect();
            let states: Vec<String> = comp
                .states
                .iter()
                .map(|state| {
                    format!(
                        "{},{},{},{}",
                        state.layer,
                        u8::from(state.visible),
                        state.opacity,
                        state.blend
                    )
                })
                .collect();
            format!("{name}\t{}", states.join(";"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The comps read back from [`encode`]; parts it cannot read are passed
/// over.
#[must_use]
pub fn decode(text: &str) -> Vec<Comp> {
    text.lines()
        .filter_map(|line| {
            let (name, states) = line.split_once('\t')?;
            let states = states
                .split(';')
                .filter_map(|state| {
                    let mut parts = state.split(',');
                    let layer = parts.next()?.trim().parse().ok()?;
                    let visible = parts.next()?.trim() == "1";
                    let opacity = parts.next()?.trim().parse().ok()?;
                    let blend = parts.next()?.trim().to_owned();
                    Some(LayerState {
                        layer,
                        visible,
                        opacity,
                        blend,
                    })
                })
                .collect();
            Some(Comp {
                name: name.to_owned(),
                states,
            })
        })
        .collect()
}

/// A comp named `name` of how `picture`'s layers show now.
#[must_use]
pub fn capture(arena: &Arena, picture: EntityId, name: &str) -> Comp {
    Comp {
        name: name.to_owned(),
        states: crate::doc::layers(arena, picture)
            .into_iter()
            .map(|info| LayerState {
                layer: info.id,
                visible: info.style.visible,
                opacity: info.style.opacity,
                blend: info.style.blend.name().to_owned(),
            })
            .collect(),
    }
}

/// The edit that shows `comp`: every layer it knows that still exists set
/// as it was, each property only where it differs.
#[must_use]
pub fn apply(arena: &Arena, picture: EntityId, comp: &Comp, label: &str) -> Edit {
    let now = crate::doc::layers(arena, picture);
    let mut edit = Edit::new(label);
    for state in &comp.states {
        let Some(info) = now.iter().find(|info| info.id == state.layer) else {
            continue;
        };
        if info.style.visible != state.visible {
            edit = edit.then(SetProperty::new(
                state.layer,
                picture::VISIBLE,
                state.visible,
            ));
        }
        if (info.style.opacity - state.opacity).abs() > 1e-4 {
            edit = edit.then(SetProperty::new(
                state.layer,
                picture::OPACITY,
                f64::from(state.opacity),
            ));
        }
        if info.style.blend.name() != state.blend {
            edit = edit.then(SetProperty::new(
                state.layer,
                picture::BLEND,
                state.blend.as_str(),
            ));
        }
    }
    edit
}

/// The edit that keeps `comps` on `picture`.
#[must_use]
pub fn store(picture: EntityId, comps: &[Comp], label: &str) -> Edit {
    Edit::new(label).then(SetProperty::new(picture, COMPS, encode(comps)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{self, NewPicture};
    use sue_core::ReversibleTransaction;

    #[test]
    fn comps_show_their_layers_again_and_keep_as_text() {
        let mut arena = Arena::new();
        let (mut made, picture) = doc::create(&NewPicture {
            width: 8,
            height: 8,
            ..NewPicture::default()
        })
        .unwrap();
        made.apply(&mut arena).unwrap();
        let layers: Vec<EntityId> = doc::layers(&arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect();
        let top = *layers.last().unwrap();
        let day = capture(&arena, picture, "Day");
        // Night: the top layer hidden, at half opacity, multiplied.
        for (key, value) in [
            (picture::VISIBLE, Value::from(false)),
            (picture::OPACITY, Value::from(0.5)),
            (picture::BLEND, Value::from("multiply")),
        ] {
            arena
                .get_by_id_mut(top)
                .unwrap()
                .properties_mut()
                .set(key, value);
        }
        let night = capture(&arena, picture, "Night\twith a tab");
        let comps = vec![day.clone(), night];
        let text = encode(&comps);
        let back = decode(&text);
        assert_eq!(back.len(), 2);
        assert_eq!(back[0], day);
        assert_eq!(back[1].name, "Night with a tab");
        assert_eq!(back[1].states.last().unwrap().blend, "multiply");
        // Back to day, as one edit that undoes exactly.
        let mut edit = apply(&arena, picture, &day, "Day");
        edit.apply(&mut arena).unwrap();
        let style = doc::layers(&arena, picture).last().unwrap().style;
        assert!(style.visible && (style.opacity - 1.0).abs() < 1e-6);
        assert_eq!(style.blend, sue_model::BlendMode::Normal);
        edit.revert(&mut arena).unwrap();
        assert!(!doc::layers(&arena, picture).last().unwrap().style.visible);
        // Kept on the picture.
        let mut keep = store(picture, &back, "Keep");
        keep.apply(&mut arena).unwrap();
        assert_eq!(read(&arena, picture), back);
        // A layer gone since is passed over.
        let gone = Comp {
            name: "Gone".to_owned(),
            states: vec![LayerState {
                layer: EntityId::new(),
                visible: false,
                opacity: 1.0,
                blend: "normal".to_owned(),
            }],
        };
        let mut nothing = apply(&arena, picture, &gone, "Gone");
        nothing.apply(&mut arena).unwrap();
    }
}
