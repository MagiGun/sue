//! # sue-model: the shared data models
//!
//! The kernel ([`sue_core`]) stores entities, relations and buffers. This
//! crate defines the data shapes that many features share, as extension
//! components any feature can read and write:
//!
//! | Model | Shared by |
//! | --- | --- |
//! | [`TextMarks`] | word processing, notes, markdown, novels, LaTeX, layout |
//! | [`manuscript`] | novels, scripts, theses, manuals, e-books, outlines |
//! | [`contents`] | tables of contents in any text, made from its headings |
//! | [`Table`] | spreadsheets, databases, CSV, schedules, bills of materials |
//! | [`Animation`] / [`Curve`] | motion graphics, 2D and 3D animation, automation, grading |
//! | [`Sequence`] / [`Notes`] | video editing, DAWs, slideshows, subtitles, singing synthesis |
//! | [`NodeGraph`] / [`NodeRegistry`] | compositing, shaders, audio patches, procedural geometry, game logic |
//! | [`Geometry`] / [`Transform`] | 3D, CAD, BIM, vector art, rigging, game levels |
//! | [`PageLayout`] | desktop publishing, books, slides, PDF pages, drawing sheets |
//! | [`picture`] / [`BlendMode`] | painting, photo editing, compositing, textures, thumbnails |
//! | [`Encoded`] | any file Sue cannot decode yet, kept byte for byte |
//!
//! Features that need something new add their own component type the same
//! way (see `HUMANZ.md` §8), and register it with [`register`].

mod blend;
pub mod contents;
mod curve;
mod encoded;
mod geometry;
mod graph;
mod layout;
pub mod manuscript;
pub mod picture;
mod sequence;
mod table;
mod text;

pub use blend::BlendMode;
pub use curve::{Animation, Curve, Interpolation, Key, Response};
pub use encoded::Encoded;
pub use geometry::{AttributeData, Domain, Geometry, Prim, PrimKind, Transform};
pub use graph::{GraphEdit, GraphOp, Link, Node, NodeGraph, NodeKey, NodeRegistry, NodeType, Port};
pub use layout::{
    FIELDS, Frame, PAGE_SIZES, Page, PageFields, PageLayout, Rect, Running, RunningPiece,
    page_size, points, running_pieces,
};
pub use sequence::{Clip, Note, Notes, Sequence, Track};
pub use table::{
    Axis, Cell, Column, HEADER_ROWS, Reshape, SetCell, TEXT_TABLE, Table, WIDTHS, column_name,
    column_shares, parse_a1,
};
pub use text::{
    CharFormat, Mark, OBJECT, PARAGRAPH_KINDS, TextMarks, char_format, ensure_final_newline,
    is_paragraph_kind, kind as mark_kind, marks_by_paragraph, paragraph_at, paragraphs, points_of,
    rgb_of, runs, word_count,
};

use sue_core::{ComponentRegistry, PropertyDef, PropertySchema, Quantity, ValueType};

/// Well-known entity classes, so features agree on what an entity is.
pub mod class {
    /// A folder or collection of entities (children by `sue.child`).
    pub const FOLDER: &str = "sue.folder";
    /// A rich text document (`StringBuffer` plus [`crate::TextMarks`]).
    pub const DOCUMENT: &str = "sue.document";
    /// A plain note.
    pub const NOTE: &str = "sue.note";
    /// Source code.
    pub const CODE: &str = "sue.code";
    /// A spreadsheet or database ([`crate::Table`]).
    pub const SHEET: &str = "sue.sheet";
    /// A raster image, or a layered picture (see [`crate::picture`]).
    pub const IMAGE: &str = "sue.image";
    /// A raster layer of a picture.
    pub const LAYER: &str = "sue.layer";
    /// A paper layer: one color under a picture's other layers.
    pub const PAPER: &str = "sue.layer.paper";
    /// A vector drawing ([`crate::Geometry`] in 2D).
    pub const VECTOR: &str = "sue.vector";
    /// Audio.
    pub const AUDIO: &str = "sue.audio";
    /// Video (a linked source decoded on demand).
    pub const VIDEO: &str = "sue.video";
    /// A timeline ([`crate::Sequence`]).
    pub const SEQUENCE: &str = "sue.sequence";
    /// A node graph ([`crate::NodeGraph`]).
    pub const GRAPH: &str = "sue.graph";
    /// A 3D model or CAD part ([`crate::Geometry`]).
    pub const MODEL: &str = "sue.model";
    /// A scene: placed children with [`crate::Transform`]s.
    pub const SCENE: &str = "sue.scene";
    /// A laid-out publication ([`crate::PageLayout`]).
    pub const PUBLICATION: &str = "sue.publication";
    /// A slideshow ([`crate::PageLayout`] with slide-sized pages).
    pub const SLIDES: &str = "sue.slides";
    /// A long piece of writing: a binder of folders and documents (see
    /// [`crate::manuscript`]).
    pub const MANUSCRIPT: &str = "sue.manuscript";
    /// A link to a file Sue does not hold in memory.
    pub const LINKED_FILE: &str = "sue.linked-file";
}

/// Registers every model component type.
pub fn register(registry: &mut ComponentRegistry) {
    registry
        .register::<TextMarks>()
        .register::<Table>()
        .register::<Animation>()
        .register::<Sequence>()
        .register::<Notes>()
        .register::<NodeGraph>()
        .register::<Geometry>()
        .register::<Transform>()
        .register::<PageLayout>()
        .register::<Encoded>();
}

/// A registry with every model component type.
#[must_use]
pub fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    register(&mut registry);
    registry
}

/// Registers the properties many features share (placement, style, time),
/// so the Inspector can edit them on any entity.
pub fn register_properties(schema: &mut PropertySchema) {
    let length = ValueType::Quantity(sue_core::Dimension::Length);
    let layer_flag = |key: &str, label: &str| {
        PropertyDef::new(key, label, false)
            .typed(ValueType::Bool)
            .group("Layer")
    };
    let defs = [
        PropertyDef::new("doc.title", "Title", "").group("Document"),
        PropertyDef::new("doc.author", "Author", "").group("Document"),
        PropertyDef::new("doc.language", "Language", "en").group("Document"),
        PropertyDef::new(manuscript::SYNOPSIS, "Synopsis", "").group("Document"),
        PropertyDef::new(manuscript::NOTES, "Notes", "").group("Document"),
        PropertyDef::new(manuscript::STATUS, "Status", "").group("Document"),
        PropertyDef::new(manuscript::LABEL, "Label", "").group("Document"),
        PropertyDef::new(manuscript::KEYWORDS, "Keywords", "").group("Document"),
        PropertyDef::new(manuscript::INCLUDE, "Include when publishing", true)
            .typed(ValueType::Bool)
            .group("Document"),
        PropertyDef::new(manuscript::TARGET, "Word target", 0_i64)
            .range(0.0, 10_000_000.0)
            .group("Document"),
        PropertyDef::new("style.opacity", "Opacity", 1.0)
            .range(0.0, 1.0)
            .group("Style"),
        PropertyDef::new("style.blend", "Blend mode", "normal")
            .typed(ValueType::Choice(
                BlendMode::ALL
                    .iter()
                    .map(|mode| mode.name().to_owned())
                    .collect(),
            ))
            .group("Style"),
        PropertyDef::new("style.fill", "Fill", sue_core::Color::srgb8(0, 0, 0)).group("Style"),
        PropertyDef::new("style.stroke", "Stroke", sue_core::Color::srgb8(0, 0, 0)).group("Style"),
        PropertyDef::new(
            "style.stroke_width",
            "Stroke width",
            Quantity::new(1.0, "pt"),
        )
        .typed(length.clone())
        .group("Style"),
        PropertyDef::new("text.font", "Font", "Inter").group("Text"),
        PropertyDef::new("text.size", "Size", Quantity::new(12.0, "pt"))
            .typed(length)
            .group("Text"),
        PropertyDef::new("audio.gain", "Gain (dB)", 0.0)
            .range(-96.0, 24.0)
            .group("Audio"),
        PropertyDef::new("audio.pan", "Pan", 0.0)
            .range(-1.0, 1.0)
            .group("Audio"),
        PropertyDef::new("time.start", "Start", sue_core::Time::ZERO).group("Time"),
        PropertyDef::new("cad.material", "Material", "").group("Engineering"),
        PropertyDef::new("cad.layer", "Layer", "0").group("Engineering"),
        PropertyDef::new(picture::WIDTH, "Width (px)", 1_i64)
            .range(1.0, 65_535.0)
            .group("Image"),
        PropertyDef::new(picture::HEIGHT, "Height (px)", 1_i64)
            .range(1.0, 65_535.0)
            .group("Image"),
        PropertyDef::new(picture::DPI, "Resolution (dpi)", 72.0)
            .range(1.0, 10_000.0)
            .group("Image"),
        PropertyDef::new(picture::VISIBLE, "Visible", true)
            .typed(ValueType::Bool)
            .group("Layer"),
        layer_flag(picture::LOCKED, "Locked"),
        layer_flag(picture::ALPHA_LOCKED, "Lock transparent pixels"),
        layer_flag(picture::CLIP, "Clip to layer below"),
        layer_flag(picture::DRAFT, "Draft layer"),
        PropertyDef::new(
            picture::COLOR,
            "Paper color",
            sue_core::Color::srgb8(255, 255, 255),
        )
        .group("Layer"),
        PropertyDef::new(picture::X, "X offset (px)", 0_i64).group("Layer"),
        PropertyDef::new(picture::Y, "Y offset (px)", 0_i64).group("Layer"),
    ];
    for def in defs {
        schema.register(def);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{Arena, CHILD, Entity, Relation, StringBuffer, SueFile, Time, Value};

    #[test]
    fn every_model_saves_and_loads_inside_one_document() {
        let mut arena = Arena::new();
        let folder = arena.insert(Entity::empty("Project", class::FOLDER));
        let folder = arena.get(folder).unwrap().id();

        let mut marks = TextMarks::new();
        marks.add(Mark::new(0..5, mark_kind::BOLD, true));
        let doc = Entity::new("Script", StringBuffer::from("Hello world"))
            .with_class(class::DOCUMENT)
            .with_ext(marks);
        let sheet = Entity::empty("Budget", class::SHEET)
            .with_ext(Table::from_rows(vec![vec!["Total".into(), 12.5.into()]]));
        let doc_id = doc.id();
        let mut sequence = Sequence::default();
        let mut track = Track::new("V1", "video");
        track.insert(Clip::new(doc_id, Time::ZERO, Time::from_seconds(3.0)));
        sequence.tracks.push(track);
        let edit = Entity::empty("Edit", class::SEQUENCE).with_ext(sequence);
        let mut geometry = Geometry::new();
        let a = geometry.add_point([0.0; 3]);
        let b = geometry.add_point([1.0, 0.0, 0.0]);
        geometry.add_prim(PrimKind::Polyline, &[a, b]).unwrap();
        let part = Entity::empty("Bracket", class::MODEL)
            .with_ext(geometry)
            .with_ext(Transform::translate(0.0, 0.0, 5.0));
        let mut layout = PageLayout::with_pages(page_size("Slide 16:9").unwrap(), 1);
        layout.pages[0]
            .frames
            .push(Frame::new(doc_id, [0.0, 0.0, 100.0, 100.0]));
        let deck = Entity::empty("Pitch", class::SLIDES).with_ext(layout);
        let graph = Entity::empty("Comp", class::GRAPH).with_ext(NodeGraph::new());
        let anim = Entity::empty("Move", "sue.motion").with_ext(Animation::new());
        let notes = Entity::empty("Melody", "sue.melody").with_ext(Notes::new());
        for entity in [doc, sheet, edit, part, deck, graph, anim, notes] {
            let id = entity.id();
            arena.insert_keep_id(entity).unwrap();
            let order = arena.relations().next_order(folder, CHILD);
            arena
                .relate(Relation::new(folder, CHILD, id).ordered(order))
                .unwrap();
        }

        let mut file = SueFile::new();
        file.set_arena(&arena);
        let bytes = file.to_bytes();
        let loaded = SueFile::from_bytes(&bytes)
            .unwrap()
            .arena(&registry())
            .unwrap();
        assert_eq!(loaded.len(), arena.len());
        for (_, entity) in arena.iter() {
            assert_eq!(loaded.get_by_id(entity.id()).unwrap(), entity);
        }
        let sheet = loaded.with_class(class::SHEET).next().unwrap();
        let table = loaded.get(sheet).unwrap().ext::<Table>().unwrap();
        assert_eq!(table.value(0, 1), Some(&Value::Float(12.5)));
        assert_eq!(loaded.relations().children(folder).len(), 8);

        // Without the model types registered, everything survives as opaque.
        let blind = SueFile::from_bytes(&bytes)
            .unwrap()
            .arena(&sue_core::ComponentRegistry::new())
            .unwrap();
        let mut again = SueFile::new();
        again.set_arena(&blind);
        assert_eq!(again.to_bytes(), bytes);
    }

    #[test]
    fn shared_properties_validate() {
        let mut schema = PropertySchema::new();
        register_properties(&mut schema);
        assert!(schema.validate("style.opacity", &Value::Float(0.5)).is_ok());
        assert!(
            schema
                .validate("style.opacity", &Value::Float(2.0))
                .is_err()
        );
        assert!(
            schema
                .validate("text.size", &Quantity::new(4.0, "mm").into())
                .is_ok()
        );
        assert!(schema.validate("style.blend", &"screen".into()).is_ok());
        assert!(schema.validate("style.blend", &"color-burn".into()).is_ok());
        assert!(schema.validate("layer.visible", &false.into()).is_ok());
        assert!(schema.validate("image.dpi", &0.0.into()).is_err());
        assert_eq!(schema.with_prefix("audio.").count(), 2);
    }
}
