//! Where a layer sits in its picture's stack: order keys for new layers,
//! and the edits that move layers.

use sue_core::txn::Relate;
use sue_core::{Arena, CHILD, EntityId, OrderKey, Relation};

use super::{Edit, LayerInfo, layers};

/// The order key for a new layer right below `above` (the bottom when
/// `above` is not in the picture).
#[must_use]
pub fn key_below(arena: &Arena, picture: EntityId, above: EntityId) -> OrderKey {
    let stack = layers(arena, picture);
    let Some(index) = stack.iter().position(|layer| layer.id == above) else {
        return OrderKey::between(None, stack.first().map(|layer| &layer.order));
    };
    let high = &stack[index].order;
    match index.checked_sub(1).map(|below| &stack[below].order) {
        Some(low) if low < high => OrderKey::between(Some(low), Some(high)),
        _ => OrderKey::between(None, Some(high)),
    }
}

/// The edit that moves `block` (layers of `picture`, bottom first) to sit
/// together right above `low` (`None`: at the bottom), in the same order.
/// `None` when `low` is not in the picture or is in the block.
#[must_use]
pub fn move_block(
    arena: &Arena,
    picture: EntityId,
    block: &[EntityId],
    low: Option<EntityId>,
) -> Option<Edit> {
    if block.is_empty() || low.is_some_and(|low| block.contains(&low)) {
        return None;
    }
    let others: Vec<LayerInfo> = layers(arena, picture)
        .into_iter()
        .filter(|info| !block.contains(&info.id))
        .collect();
    let at = match low {
        Some(low) => Some(others.iter().position(|info| info.id == low)?),
        None => None,
    };
    let low_key = at.map(|index| others[index].order.clone());
    let high_key = others
        .get(at.map_or(0, |index| index + 1))
        .map(|info| info.order.clone());
    if let (Some(low), Some(high)) = (&low_key, &high_key)
        && low >= high
    {
        return None;
    }
    let mut edit = Edit::new(tr!("move-layer"));
    let mut last = low_key;
    for id in block {
        let key = OrderKey::between(last.as_ref(), high_key.as_ref());
        edit = edit.then(Relate::new(
            Relation::new(picture, CHILD, *id).ordered(key.clone()),
        ));
        last = Some(key);
    }
    Some(edit)
}

/// The order key for a new layer right above `below` (or at the top).
pub(crate) fn key_above(arena: &Arena, picture: EntityId, below: Option<EntityId>) -> OrderKey {
    let stack = layers(arena, picture);
    let Some(index) = below.and_then(|id| stack.iter().position(|layer| layer.id == id)) else {
        return arena.relations().next_order(picture, CHILD);
    };
    let low = &stack[index].order;
    match stack.get(index + 1) {
        Some(next) if next.order > *low => OrderKey::between(Some(low), Some(&next.order)),
        _ => OrderKey::between(Some(low), None),
    }
}

/// `count` increasing order keys for new layers right above `below` (or
/// at the top), bottom first.
#[must_use]
pub fn keys_above(
    arena: &Arena,
    picture: EntityId,
    below: Option<EntityId>,
    count: usize,
) -> Vec<OrderKey> {
    let stack = layers(arena, picture);
    let at = below.and_then(|id| stack.iter().position(|layer| layer.id == id));
    let mut low = match at {
        Some(index) => Some(stack[index].order.clone()),
        None => stack.last().map(|layer| layer.order.clone()),
    };
    let high = at
        .and_then(|index| stack.get(index + 1))
        .map(|layer| layer.order.clone());
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let order = match (&low, &high) {
            (Some(low), Some(high)) if high > low => OrderKey::between(Some(low), Some(high)),
            (low, _) => OrderKey::between(low.as_ref(), None),
        };
        out.push(order.clone());
        low = Some(order);
    }
    out
}

/// Moves `layer` to position `index` of the stack (0 is the bottom).
#[must_use]
pub fn reorder(arena: &Arena, picture: EntityId, layer: EntityId, index: usize) -> Option<Edit> {
    let stack: Vec<LayerInfo> = layers(arena, picture);
    let from = stack.iter().position(|info| info.id == layer)?;
    let others: Vec<&LayerInfo> = stack.iter().filter(|info| info.id != layer).collect();
    let index = index.min(others.len());
    if index == from {
        return None;
    }
    let low = index
        .checked_sub(1)
        .and_then(|at| others.get(at))
        .map(|info| &info.order);
    let high = others.get(index).map(|info| &info.order);
    let order = match (low, high) {
        (Some(low), Some(high)) if low >= high => return None,
        _ => OrderKey::between(low, high),
    };
    Some(Edit::new(tr!("move-layer")).then(Relate::new(
        Relation::new(picture, CHILD, layer).ordered(order),
    )))
}
