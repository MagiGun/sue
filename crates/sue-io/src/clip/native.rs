//! Clip Studio's own storage of the layers that are more than pixels:
//! fill and gradient layers (`GradationFillInfo`), text layers
//! (`TextLayerString`, `TextLayerAttributes`), vector layers (a
//! `VectorObjectList` row whose strokes sit in an `Exta` chunk, drawn with
//! a brush written into the document), palette color and the sub color of
//! the layer color effect.
//!
//! Every layout here was read from files made in Clip Studio Paint on
//! this machine (the survey of 2026-11-16, `clip-format.md` section 9);
//! nothing is copied from anyone's description of the format. Numbers
//! whose meaning is not known are written as Clip Studio wrote them.
//! What the survey did not reach (correction layers, rulers, layer comps
//! made by hand, the timeline, circle and ellipse gradients) stays in
//! Sue's own tables (`docs/clip-sue-tables.md`).

use super::container::{be32, put_string};
use super::kinds::{
    ClipAlign, ClipContent, ClipFill, ClipGradient, ClipGradientRepeat, ClipGradientShape,
    ClipLine, ClipPoint, ClipStop, ClipText, ClipTextRun, ClipVector,
};
use super::model::{ClipTable, Columns, column};
use crate::sqlite::Value;

/// `LayerType` of a fill or gradient layer, as Clip Studio writes it.
pub const GENERATED: i64 = 2;
/// `LayerType` of a text or vector layer, as Clip Studio writes it (a
/// folder is told by `LayerFolder`, not by this).
pub const DRAWN: i64 = 0;

/// The `Layer` columns this module writes that Sue's own column list
/// lacks, declared as Clip Studio declares them.
pub(super) const NATIVE_LAYER_COLUMNS: &str = "GradationFillColorMixingInfo NULL DEFAULT NULL, DrawToRenderOffscreenType INTEGER DEFAULT NULL, DrawToRenderMipmapType INTEGER DEFAULT NULL, MoveOffsetAndExpandType INTEGER DEFAULT NULL, FixOffsetAndExpandType INTEGER DEFAULT NULL, RenderBoundForLayerMoveType INTEGER DEFAULT NULL, SetRenderThumbnailInfoType INTEGER DEFAULT NULL, DrawRenderThumbnailType INTEGER DEFAULT NULL, VectorNormalStrokeIndex INTEGER DEFAULT NULL, VectorNormalFillIndex INTEGER DEFAULT NULL, VectorNormalBalloonIndex INTEGER DEFAULT NULL, VectorNormalType INTEGER DEFAULT NULL, MixSubColorForEveryPlot INTEGER DEFAULT NULL, MaterialContentType INTEGER DEFAULT NULL, TextLayerType INTEGER DEFAULT NULL, TextLayerString BLOB DEFAULT NULL, TextLayerAttributes BLOB DEFAULT NULL, TextLayerAddAttributesV01 BLOB DEFAULT NULL, TextLayerAttributesVersion INTEGER DEFAULT NULL, TextLayerNameAutoChangeEnabled INTEGER DEFAULT NULL, GradationFillInfo BLOB DEFAULT NULL, DrawColorSubRed INTEGER DEFAULT NULL, DrawColorSubGreen INTEGER DEFAULT NULL, DrawColorSubBlue INTEGER DEFAULT NULL";

/// How much of `content` Clip Studio's own storage holds, as far as the
/// survey shows it: `None` when it has no place for it yet (it stays in
/// Sue's own tables and Clip Studio sees its pixels), `Some(false)` when
/// all of it, `Some(true)` when something is lost on the way (Sue keeps
/// that in its own tables).
#[must_use]
pub fn clip_studio_fit(content: &ClipContent) -> Option<bool> {
    match content {
        ClipContent::Fill(fill) => Some(fill.color[3] != 255),
        ClipContent::Gradient(gradient) => (gradient.shape == ClipGradientShape::Linear
            && gradient.repeat == ClipGradientRepeat::Clamp
            && !gradient.stops.is_empty())
        .then_some(false),
        ClipContent::Text(text) => {
            if text.plain().is_empty() {
                return None;
            }
            let first = text.runs.first();
            Some(
                text.runs.iter().any(|run| {
                    run.bold
                        || run.italic
                        || run.color[3] != 255
                        || first.is_some_and(|first| {
                            run.font != first.font
                                || (run.size - first.size).abs() > 1e-3
                                || run.color != first.color
                        })
                }) || text.vertical
                    || text.align != ClipAlign::Start
                    || text.frame.is_some()
                    || (text.line_spacing - 1.0).abs() > 1e-3,
            )
        }
        ClipContent::Vector(vector) => {
            if !vector.lines.iter().any(|line| !line.points.is_empty()) {
                return None;
            }
            Some(vector.lines.iter().any(|line| {
                line.closed
                    || line
                        .points
                        .iter()
                        .any(|point| point.corner || (point.opacity - 1.0).abs() > 1e-3)
            }))
        }
        _ => None,
    }
}

// ----- Small pieces ----------------------------------------------------------------

/// An 8-bit channel as Clip Studio writes it: the byte four times over.
fn quad(byte: u8) -> u32 {
    u32::from(byte) * 0x0101_0101
}

/// The 8-bit channel of a value written four times over (its top byte).
fn top(value: u32) -> u8 {
    (value >> 24) as u8
}

/// An 8-bit channel as a `Layer` column holds it.
#[must_use]
pub(super) fn quad_column(byte: u8) -> i64 {
    i64::from(quad(byte))
}

/// The 8-bit channel of a `Layer` color column: its top byte, or the
/// value itself when it is no more than 255.
#[must_use]
pub(super) fn column_byte(value: i64) -> u8 {
    if (0..=255).contains(&value) {
        value as u8
    } else {
        ((value >> 24) & 0xff) as u8
    }
}

fn be(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn be_real(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_be_bytes());
}

fn be_real_at(bytes: &[u8], at: usize) -> Option<f64> {
    Some(f64::from_be_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

fn be_f32_at(bytes: &[u8], at: usize) -> Option<f32> {
    Some(f32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A UTF-16 name (a count, then big-endian letters) at `at`, and where
/// what follows it starts.
fn name_at(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let count = usize::try_from(be32(bytes, at)?).ok()?;
    let end = at.checked_add(4)?.checked_add(count.checked_mul(2)?)?;
    let units: Vec<u16> = bytes
        .get(at + 4..end)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_be_bytes(*pair))
        .collect();
    Some((String::from_utf16(&units).ok()?, end))
}

fn rounded(value: f64) -> i32 {
    value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

// ----- Fill and gradient layers: GradationFillInfo ---------------------------------

/// The numbers after the size of a stop and their count in the header of
/// `GradationData`, as Clip Studio writes them (their meaning is not
/// known).
const GRADATION_HEAD: [u32; 3] = [16, 1, 3];

/// What a gradient stop takes its color from, as Clip Studio writes it:
/// 1 the main drawing color, 2 the sub drawing color. The color is
/// written out beside it either way. Sue writes what Clip Studio's
/// default gradient has (the first stop 1, the others 2); the number for
/// "a color of its own" is not yet observed.
fn stop_source(index: usize) -> u32 {
    if index == 0 { 1 } else { 2 }
}

/// The `GradationFillInfo` blob of a fill or a linear gradient that
/// repeats nothing; `None` for any other content.
#[must_use]
pub fn gradation_info(content: &ClipContent) -> Option<Vec<u8>> {
    match content {
        ClipContent::Fill(ClipFill { color }) => {
            let stop = |at: f32| ClipStop { at, color: *color };
            Some(gradation_blob(
                &[stop(0.0), stop(1.0)],
                [[0.0, 0.0], [0.0, 0.0]],
                Some([color[0], color[1], color[2]]),
            ))
        }
        ClipContent::Gradient(gradient) if clip_studio_fit(content).is_some() => Some(
            gradation_blob(&gradient.stops, [gradient.start, gradient.end], None),
        ),
        _ => None,
    }
}

fn gradation_blob(stops: &[ClipStop], line: [[f32; 2]; 2], plain: Option<[u8; 3]>) -> Vec<u8> {
    let mut body = Vec::new();
    be(&mut body, 2);
    put_string(&mut body, "GradationData");
    let mut data = Vec::new();
    let count = u32::try_from(stops.len()).unwrap_or(u32::MAX);
    for value in [24, 28, count] {
        be(&mut data, value);
    }
    for value in GRADATION_HEAD {
        be(&mut data, value);
    }
    for (index, stop) in stops.iter().enumerate() {
        for channel in stop.color {
            be(&mut data, quad(channel));
        }
        be(&mut data, stop_source(index));
        be(
            &mut data,
            (f64::from(stop.at.clamp(0.0, 1.0)) * 32768.0).round() as u32,
        );
        be(&mut data, 0);
    }
    be(&mut body, u32::try_from(data.len()).unwrap_or(u32::MAX));
    body.extend_from_slice(&data);
    put_string(&mut body, "GradationSetting");
    for value in [0, 0, 1] {
        be(&mut body, value);
    }
    for value in [
        100.0,
        100.0,
        0.0,
        f64::from(line[0][0]),
        f64::from(line[0][1]),
        f64::from(line[1][0]),
        f64::from(line[1][1]),
    ] {
        be_real(&mut body, value);
    }
    put_string(&mut body, "GradationSettingAdd0001");
    be(&mut body, 20);
    be(&mut body, u32::from(plain.is_some()));
    for channel in plain.unwrap_or_default() {
        be(&mut body, quad(channel));
    }
    be(&mut body, 1);
    let mut out = Vec::with_capacity(body.len() + 4);
    be(&mut out, u32::try_from(body.len() + 4).unwrap_or(u32::MAX));
    out.extend_from_slice(&body);
    out
}

/// The fill or gradient a `GradationFillInfo` blob describes, when it is
/// laid out as the survey found it and its shape is one Sue has seen
/// (a linear gradient; a fill).
#[must_use]
pub fn gradation_of(blob: &[u8]) -> Option<ClipContent> {
    let total = usize::try_from(be32(blob, 0)?).ok()?;
    if total != blob.len() {
        return None;
    }
    let mut at = 8;
    let (mut stops, mut line, mut setting_ok, mut plain) = (None, None, false, None);
    while at < blob.len() {
        let (name, after) = name_at(blob, at)?;
        match name.as_str() {
            "GradationData" => {
                let len = usize::try_from(be32(blob, after)?).ok()?;
                let data = blob.get(after + 4..after + 4 + len)?;
                stops = Some(gradation_stops(data)?);
                at = after + 4 + len;
            }
            "GradationSetting" => {
                let words = [
                    be32(blob, after)?,
                    be32(blob, after + 4)?,
                    be32(blob, after + 8)?,
                ];
                setting_ok = words == [0, 0, 1];
                let real = |i: usize| be_real_at(blob, after + 12 + 8 * i);
                let point =
                    |i: usize| -> Option<[f32; 2]> { Some([real(i)? as f32, real(i + 1)? as f32]) };
                line = Some([point(3)?, point(5)?]);
                at = after + 12 + 56;
            }
            "GradationSettingAdd0001" => {
                let len = usize::try_from(be32(blob, after)?).ok()?;
                let data = blob.get(after + 4..after + 4 + len)?;
                if be32(data, 0)? != 0 {
                    plain = Some([
                        top(be32(data, 4)?),
                        top(be32(data, 8)?),
                        top(be32(data, 12)?),
                    ]);
                }
                at = after + 4 + len;
            }
            _ => return None,
        }
    }
    if let Some([r, g, b]) = plain {
        return Some(ClipContent::Fill(ClipFill {
            color: [r, g, b, 255],
        }));
    }
    let (stops, [start, end]) = (stops?, line?);
    (setting_ok && !stops.is_empty()).then_some(ClipContent::Gradient(ClipGradient {
        shape: ClipGradientShape::Linear,
        start,
        end,
        ratio: 1.0,
        repeat: ClipGradientRepeat::Clamp,
        stops,
    }))
}

fn gradation_stops(data: &[u8]) -> Option<Vec<ClipStop>> {
    let head = usize::try_from(be32(data, 0)?).ok()?;
    let size = usize::try_from(be32(data, 4)?).ok()?;
    let count = usize::try_from(be32(data, 8)?).ok()?;
    if size < 24 || count > 4096 {
        return None;
    }
    (0..count)
        .map(|index| {
            let at = head.checked_add(index.checked_mul(size)?)?;
            let channel = |i: usize| be32(data, at + 4 * i).map(top);
            let position = be32(data, at + 20)? as f32 / 32768.0;
            Some(ClipStop {
                at: position.clamp(0.0, 1.0),
                color: [channel(0)?, channel(1)?, channel(2)?, channel(3)?],
            })
        })
        .collect()
}

// ----- Text layers: TextLayerString and TextLayerAttributes ------------------------

/// What a text layer's three blobs hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextBlobs {
    /// `TextLayerString`: the text, UTF-8.
    pub string: Vec<u8>,
    /// `TextLayerAttributes`.
    pub attributes: Vec<u8>,
    /// `TextLayerAddAttributesV01`.
    pub add: Vec<u8>,
}

/// One record of the attribute list: its id and its bytes.
type Record = (u32, Vec<u8>);

fn le(value: u32) -> Vec<u8> {
    value.to_le_bytes().to_vec()
}

fn le_list(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// A style run list of one run over `count` characters holding `data`.
fn run(count: u32, data: &[u8]) -> Vec<u8> {
    let mut out = le_list(&[1, 0]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(data);
    out
}

/// A text with a 16-bit length in front.
fn short_text(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let len = u16::try_from(bytes.len()).unwrap_or(u16::MAX);
    let mut out = len.to_le_bytes().to_vec();
    out.extend_from_slice(&bytes[..usize::from(len)]);
    out
}

/// The name of a font's face, as Clip Studio names one (`Tahoma Regular`).
fn face_name(run: &ClipTextRun) -> String {
    let style = match (run.bold, run.italic) {
        (false, false) => "Regular",
        (true, false) => "Bold",
        (false, true) => "Italic",
        (true, true) => "Bold Italic",
    };
    format!("{} {style}", run.font)
}

/// The box a text covers on the canvas, in pixels: left, top, right,
/// bottom. Clip Studio lays the words out again itself; the box only has
/// to be near.
fn text_box(text: &ClipText, dpi: f64) -> [i32; 4] {
    let size = text.runs.first().map_or(12.0, |run| f64::from(run.size)) * dpi.max(1.0) / 72.0;
    let plain = text.plain();
    let lines = plain.split('\n').count().max(1);
    let longest = plain
        .split('\n')
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);
    let (width, height) = match text.frame {
        Some([w, h]) => (f64::from(w), f64::from(h)),
        None => (longest as f64 * size * 0.55, lines as f64 * size * 1.2),
    };
    let left = rounded(f64::from(text.at[0]));
    let top = rounded(f64::from(text.at[1]));
    [
        left,
        top,
        left.saturating_add(rounded(width).max(1)),
        top.saturating_add(rounded(height).max(1)),
    ]
}

/// The records of a text, in the order Clip Studio writes them. `full`
/// gives the style runs their data (`TextLayerAttributes`); otherwise
/// they are written short, as in `TextLayerAddAttributesV01`.
#[expect(
    clippy::too_many_lines,
    reason = "one pass from top to bottom; split up, its steps would be read out of order"
)]
fn text_records(text: &ClipText, dpi: f64, full: bool) -> Vec<Record> {
    let first = text.runs.first();
    let count = u32::try_from(text.plain().encode_utf16().count()).unwrap_or(u32::MAX);
    let face = first.map_or_else(|| "Regular".to_owned(), face_name);
    let family = first.map_or("", |run| run.font.as_str());
    let [r, g, b, _] = first.map_or([0, 0, 0, 255], |run| run.color);
    let size = first.map_or(1200, |run| (f64::from(run.size) * 100.0).round() as i32);
    let [left, top_edge, right, bottom] = text_box(text, dpi);
    let (w, h) = (right - left, bottom - top_edge);
    let style = |data: &[u8]| {
        if full {
            run(count, data)
        } else {
            le_list(&[1, 4, 0])
        }
    };
    let mut line_spacing = le_list(&[1]);
    line_spacing.extend_from_slice(&[0; 6]);
    line_spacing.extend_from_slice(&120.0_f64.to_le_bytes());
    let mut scale = 100.0_f64.to_le_bytes().to_vec();
    scale.extend_from_slice(&100.0_f64.to_le_bytes());
    // The first list is laid out unlike the others: its fourth number
    // (26) is not the size of the 18 bytes after it.
    let first_run = if full {
        let mut out = le_list(&[1, 0]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&le(26));
        out.extend_from_slice(&[0; 18]);
        out
    } else {
        let mut out = le_list(&[1, 14]);
        out.extend_from_slice(&[0; 14]);
        out
    };
    let mut font_47 = le_list(&[0x0032_0004]);
    font_47.extend_from_slice(&[0; 6]);
    font_47.extend_from_slice(&short_text(&face));
    let font_57 = if full {
        let mut out = 1_u16.to_le_bytes().to_vec();
        out.extend_from_slice(&short_text(&face));
        out.extend_from_slice(&short_text(family));
        out
    } else {
        Vec::new()
    };
    let corners = le_list(&[0, 0, w * 100, 0, w * 100, h * 100, 0, h * 100]);
    vec![
        (11, first_run),
        (26, style(&scale)),
        (17, Vec::new()),
        (12, style(&[0, 0])),
        (13, style(&line_spacing)),
        (14, Vec::new()),
        (27, Vec::new()),
        (28, Vec::new()),
        (29, style(&[1, 0])),
        (65, style(&[1, 0])),
        (30, style(&[0; 4])),
        (15, Vec::new()),
        (19, Vec::new()),
        (16, Vec::new()),
        (20, Vec::new()),
        (18, style(&[0, 0])),
        (21, Vec::new()),
        (22, Vec::new()),
        (31, face.clone().into_bytes()),
        (32, le_list(&[size])),
        (33, le(0)),
        (
            34,
            [quad(r), quad(g), quad(b)]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect(),
        ),
        (35, le(0)),
        (37, le(0)),
        (44, le(1000)),
        (38, le(2)),
        (43, le(50)),
        (39, le_list(&[i32::try_from(count).unwrap_or(i32::MAX), 0])),
        (40, Vec::new()),
        (41, Vec::new()),
        (42, le_list(&[left, top_edge, right, bottom])),
        (45, le(0)),
        (46, le(0)),
        (47, font_47),
        (48, le(1)),
        (53, le(0)),
        (55, le(0)),
        (52, le(1)),
        (54, le_list(&[0, 0, 0, 0, -1])),
        (56, le_list(&[0, 1000, 0, -1, -1, -1])),
        (49, le(1003)),
        (50, le(39)),
        (51, le(0)),
        (57, font_57),
        (58, le(1800)),
        (59, le(900)),
        (60, le(900)),
        (61, le(0)),
        (62, le(0)),
        (63, le_list(&[w, h])),
        (64, corners),
        (66, le(0)),
        (67, le(1000)),
        (68, le(0)),
        (69, le(0)),
        (70, vec![0; 8]),
        // The real 360 written big-endian inside this little-endian list,
        // as Clip Studio writes it.
        (71, 360.0_f64.to_be_bytes().to_vec()),
        (72, vec![0; 8]),
        (73, le(1)),
        (74, le(0)),
    ]
}

fn record_bytes(records: &[Record]) -> Vec<u8> {
    let mut out = Vec::new();
    for (id, data) in records {
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_le_bytes());
        out.extend_from_slice(data);
    }
    out
}

/// The three blobs of a text layer; `dpi` is the canvas's.
#[must_use]
pub fn text_blobs(text: &ClipText, dpi: f64) -> TextBlobs {
    let attributes = record_bytes(&text_records(text, dpi, true));
    let short = record_bytes(&text_records(text, dpi, false));
    let mut add = u32::try_from(short.len())
        .unwrap_or(0)
        .to_le_bytes()
        .to_vec();
    add.extend_from_slice(&short);
    TextBlobs {
        string: text.plain().into_bytes(),
        attributes,
        add,
    }
}

/// The records of an attribute list, when it is laid out as one.
fn records_of(bytes: &[u8]) -> Option<Vec<(u32, &[u8])>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let id = le32_at(bytes, at)?;
        let len = usize::try_from(le32_at(bytes, at + 4)?).ok()?;
        let end = at.checked_add(8)?.checked_add(len)?;
        out.push((id, bytes.get(at + 8..end)?));
        at = end;
    }
    Some(out)
}

/// A text with a 16-bit length in front at `at`, and where it ends.
fn short_text_at(bytes: &[u8], at: usize) -> Option<(String, usize)> {
    let len = usize::from(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let text = bytes.get(at + 2..at + 2 + len)?;
    Some((String::from_utf8_lossy(text).into_owned(), at + 2 + len))
}

/// The text a text layer's `TextLayerString` and `TextLayerAttributes`
/// hold: its words, font family, size, color and where it starts. `dpi`
/// is the canvas's.
#[must_use]
pub fn text_of(string: &[u8], attributes: &[u8]) -> Option<ClipText> {
    let text = String::from_utf8(string.to_vec()).ok()?;
    let records = records_of(attributes)?;
    let find = |id: u32| {
        records
            .iter()
            .find(|(found, _)| *found == id)
            .map(|(_, data)| *data)
    };
    let family = find(57)
        .and_then(|data| {
            let (_, after) = short_text_at(data, 2)?;
            let (family, _) = short_text_at(data, after)?;
            (!family.is_empty()).then_some(family)
        })
        .or_else(|| {
            let face = String::from_utf8_lossy(find(31)?).into_owned();
            Some(
                ["Bold Italic", "Regular", "Bold", "Italic"]
                    .iter()
                    .find_map(|style| face.strip_suffix(style).map(|f| f.trim_end().to_owned()))
                    .unwrap_or(face),
            )
        })
        .unwrap_or_default();
    let size = find(32)
        .and_then(|data| le32_at(data, 0))
        .map_or(12.0, |value| value as f32 / 100.0);
    let color = find(34).map_or([0, 0, 0, 255], |data| {
        let channel = |i: usize| le32_at(data, 4 * i).map_or(0, top);
        [channel(0), channel(1), channel(2), 255]
    });
    let at = find(42).map_or([0.0, 0.0], |data| {
        let value = |i: usize| le32_at(data, 4 * i).map_or(0.0, |v| v as i32 as f32);
        [value(0), value(1)]
    });
    Some(ClipText {
        runs: vec![ClipTextRun {
            text,
            font: family,
            size,
            color,
            bold: false,
            italic: false,
        }],
        at,
        frame: None,
        vertical: false,
        align: ClipAlign::Start,
        line_spacing: 1.0,
    })
}

// ----- Vector layers: strokes in an Exta chunk --------------------------------------

/// The size of a stroke's header and of one point, as Clip Studio writes
/// them.
const STROKE_HEAD: usize = 92;
const POINT: usize = 88;
/// Numbers of a stroke whose meaning is not known, as Clip Studio wrote
/// them: the header's second number, its flags and its last number
/// (which the first point repeats).
const STROKE_SECOND: u32 = 76;
const STROKE_FLAGS: u32 = 0x2081;
const STROKE_LAST: u32 = 0x0180_09cc;

fn floor(value: f64) -> u32 {
    value.floor().clamp(0.0, f64::from(u32::MAX)) as u32
}

fn ceil(value: f64) -> u32 {
    value.ceil().clamp(0.0, f64::from(u32::MAX)) as u32
}

/// The strokes of a vector layer as the chunk Clip Studio stores them in,
/// each drawn with the brush whose `MainId` is `brush`.
#[must_use]
pub fn vector_bytes(vector: &ClipVector, brush: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for line in vector.lines.iter().filter(|line| !line.points.is_empty()) {
        let radius = f64::from(line.width.max(0.0)) / 2.0;
        let reach = radius + 1.0;
        let points: Vec<[f64; 2]> = line
            .points
            .iter()
            .map(|point| point.at.map(f64::from))
            .collect();
        // Each point's box covers its piece of line, to the next point.
        let boxes: Vec<[u32; 4]> = points
            .iter()
            .enumerate()
            .map(|(i, at)| {
                let next = points.get(i + 1).unwrap_or(at);
                [
                    floor(at[0].min(next[0]) - reach),
                    floor(at[1].min(next[1]) - reach),
                    ceil(at[0].max(next[0]) + reach),
                    ceil(at[1].max(next[1]) + reach),
                ]
            })
            .collect();
        let whole = boxes.iter().fold([u32::MAX, u32::MAX, 0, 0], |a, b| {
            [
                a[0].min(b[0]),
                a[1].min(b[1]),
                a[2].max(b[2]),
                a[3].max(b[3]),
            ]
        });
        let count = u32::try_from(points.len()).unwrap_or(u32::MAX);
        for value in [
            STROKE_HEAD as u32,
            STROKE_SECOND,
            POINT as u32,
            POINT as u32,
            count,
            STROKE_FLAGS,
        ] {
            be(&mut out, value);
        }
        for value in whole {
            be(&mut out, value);
        }
        let [r, g, b, a] = line.color;
        for channel in [r, g, b, 255, 255, 255] {
            be(&mut out, quad(channel));
        }
        be_real(&mut out, f64::from(a) / 255.0);
        be(&mut out, 0);
        be(&mut out, brush);
        be_real(&mut out, radius);
        be(&mut out, STROKE_LAST);
        for (i, (point, bounds)) in line.points.iter().zip(&boxes).enumerate() {
            be_real(&mut out, points[i][0]);
            be_real(&mut out, points[i][1]);
            for value in bounds {
                be(&mut out, *value);
            }
            be(&mut out, 0);
            for value in [1.0_f32, 1.0, point.width.max(0.0)] {
                be(&mut out, value.to_bits());
            }
            be(&mut out, 0);
            for value in [1.0_f32, 1.0, 1.0] {
                be(&mut out, value.to_bits());
            }
            for value in [0, 0, 0, 0, STROKE_LAST, 0] {
                be(&mut out, value);
            }
        }
    }
    out
}

/// The strokes in a vector layer's chunk, when they are laid out as the
/// survey found them.
#[must_use]
pub fn vector_of(bytes: &[u8]) -> Option<ClipVector> {
    let mut lines = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let head = usize::try_from(be32(bytes, at)?).ok()?;
        let point_size = usize::try_from(be32(bytes, at + 8)?).ok()?;
        let count = usize::try_from(be32(bytes, at + 16)?).ok()?;
        if head < STROKE_HEAD || point_size < 48 {
            return None;
        }
        let channel = |i: usize| be32(bytes, at + 40 + 4 * i).map(top);
        let opacity = be_real_at(bytes, at + 64)?;
        let radius = be_real_at(bytes, at + 80)?;
        let alpha = (opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let start = at.checked_add(head)?;
        // The whole stroke is there, or none of it is read.
        bytes.get(start..start.checked_add(count.checked_mul(point_size)?)?)?;
        let points = (0..count)
            .map(|i| {
                let p = start.checked_add(i.checked_mul(point_size)?)?;
                Some(ClipPoint {
                    at: [
                        be_real_at(bytes, p)? as f32,
                        be_real_at(bytes, p + 8)? as f32,
                    ],
                    pressure: 1.0,
                    width: be_f32_at(bytes, p + 44)?,
                    opacity: 1.0,
                    corner: false,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        lines.push(ClipLine {
            points,
            width: (radius * 2.0) as f32,
            color: [channel(0)?, channel(1)?, channel(2)?, alpha],
            brush: String::new(),
            closed: false,
        });
        at = start.checked_add(count.checked_mul(point_size)?)?;
    }
    Some(ClipVector { lines })
}

/// The stroke data of a vector layer whose `VectorNormalStrokeIndex` is
/// `index`, found through the kept `VectorObjectList` table and the kept
/// chunks: its row's place in the table and the chunk's id.
#[must_use]
pub(super) fn stroke_row(table: &ClipTable, index: i64) -> Option<(usize, String)> {
    let main = table.columns.iter().position(|c| c == "MainId")?;
    let data = table.columns.iter().position(|c| c == "VectorData")?;
    table.rows.iter().enumerate().find_map(|(i, (_, values))| {
        (values.get(main)?.int()? == index).then(|| {
            values
                .get(data)
                .and_then(Value::bytes)
                .map(|id| (i, String::from_utf8_lossy(id).into_owned()))
        })?
    })
}

// ----- The brush a vector layer is drawn with ---------------------------------------

/// `CREATE TABLE` of `VectorObjectList`, as Clip Studio declares it.
pub(super) const VECTOR_OBJECT_LIST_SQL: &str = "CREATE TABLE VectorObjectList(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, LayerId INTEGER DEFAULT NULL, VectorData BLOB DEFAULT NULL)";

/// `CREATE TABLE` of `BrushStyleManager`.
pub(super) const BRUSH_MANAGER_SQL: &str = "CREATE TABLE BrushStyleManager(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, FirstGraphData INTEGER DEFAULT NULL, FirstPatternImage INTEGER DEFAULT NULL, FirstPattern INTEGER DEFAULT NULL, FirstFixedSpray INTEGER DEFAULT NULL, FirstBrushStyle INTEGER DEFAULT NULL, FirstFillStyle INTEGER DEFAULT NULL)";

/// `CREATE TABLE` of `BrushEffectorGraphData`.
pub(super) const BRUSH_GRAPH_SQL: &str = "CREATE TABLE BrushEffectorGraphData(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, NextIndex INTEGER DEFAULT NULL, ControlNumber INTEGER DEFAULT NULL, ControlDataSize INTEGER DEFAULT NULL, ControlPoints BLOB DEFAULT NULL)";

/// `CREATE TABLE` of `BrushStyle`.
pub(super) const BRUSH_STYLE_SQL: &str = "CREATE TABLE BrushStyle(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, MainId INTEGER DEFAULT NULL, CanvasId INTEGER DEFAULT NULL, NextIndex INTEGER DEFAULT NULL, StyleFlag INTEGER DEFAULT NULL, PenRadius REAL DEFAULT NULL, SizeEffector BLOB DEFAULT NULL, OpacityEffector BLOB DEFAULT NULL, FlowBase REAL DEFAULT NULL, FlowEffector BLOB DEFAULT NULL, AntiAlias INTEGER DEFAULT NULL, MipmapIndexToPlot INTEGER DEFAULT NULL, Hardness REAL DEFAULT NULL, IntervalBase REAL DEFAULT NULL, IntervalEffector BLOB DEFAULT NULL, AutoIntervalType INTEGER DEFAULT NULL, ThicknessBase REAL DEFAULT NULL, ThicknessEffector BLOB DEFAULT NULL, RotationBase REAL DEFAULT NULL, RotationInSprayBase REAL DEFAULT NULL, RotationEffector INTEGER DEFAULT NULL, RotationRandom REAL DEFAULT NULL, RotationEffectorInSpray INTEGER DEFAULT NULL, RotationRandomInSpray REAL DEFAULT NULL, PatternStyle INTEGER DEFAULT NULL, CompositeMode INTEGER DEFAULT NULL, DualCompositeMode INTEGER DEFAULT NULL, TexturePattern INTEGER DEFAULT NULL, TextureFlag INTEGER DEFAULT NULL, TextureComposite INTEGER DEFAULT NULL, TextureScale REAL DEFAULT NULL, TextureRotate REAL DEFAULT NULL, TextureOffsetX REAL DEFAULT NULL, TextureOffsetY REAL DEFAULT NULL, TextureBrightness REAL DEFAULT NULL, TextureContrast REAL DEFAULT NULL, TextureDensityBase REAL DEFAULT NULL, TextureDensityEffector BLOB DEFAULT NULL, UseWaterColor INTEGER DEFAULT NULL, WaterColorType INTEGER DEFAULT NULL, MixColorBase REAL DEFAULT NULL, MixColorEffector BLOB DEFAULT NULL, MixAlphaBase REAL DEFAULT NULL, MixAlphaEffector BLOB DEFAULT NULL, ColorExtension REAL DEFAULT NULL, BrushColorMixingMode INTEGER DEFAULT NULL, BrushLMSLinearity INTEGER DEFAULT NULL, BlurKind INTEGER DEFAULT NULL, BlurBase REAL DEFAULT NULL, BlurEffector BLOB DEFAULT NULL, SubColorBase REAL DEFAULT NULL, SubColorEffector BLOB DEFAULT NULL, HueChangeBase REAL DEFAULT NULL, HueChangeEffector BLOB DEFAULT NULL, SaturationChangeBase REAL DEFAULT NULL, SaturationChangeEffector BLOB DEFAULT NULL, ValueChangeBase REAL DEFAULT NULL, ValueChangeEffector BLOB DEFAULT NULL, ChangeDrawColorTarget INTEGER DEFAULT NULL, SprayFlag INTEGER DEFAULT NULL, SpraySizeBase REAL DEFAULT NULL, SpraySizeEffector BLOB DEFAULT NULL, SprayDensityBase REAL DEFAULT NULL, SprayDensityEffector BLOB DEFAULT NULL, SprayBias REAL DEFAULT NULL, FixedSpray INTEGER DEFAULT NULL, WaterEdgeFlag INTEGER DEFAULT NULL, WaterEdgeRadius REAL DEFAULT NULL, WaterEdgeAlphaPower REAL DEFAULT NULL, WaterEdgeValuePower REAL DEFAULT NULL, WaterEdgeBlur REAL DEFAULT NULL)";

/// The two pressure curves of the plain pen, as `BrushEffectorGraphData`
/// holds them: `ControlNumber`, `ControlDataSize`, `ControlPoints`.
#[must_use]
pub(super) fn pen_graphs() -> [(i64, i64, Vec<u8>); 2] {
    let points = |values: [f64; 4]| -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    };
    [
        (2, 16, points([0.0, 0.0, 1.0, 1.0])),
        (2, 16, points([0.0, 1.0, 1.0, 0.0])),
    ]
}

/// The plain pen a Sue vector line is drawn with in Clip Studio (its
/// reals written bit for bit as Clip Studio wrote them, so a file's own
/// pen is found again): a
/// `BrushStyle` row after its id columns (`MainId`, `CanvasId`,
/// `NextIndex`), with the ids of its two pressure curves (`graphs`)
/// written into the effectors that point at them.
#[must_use]
pub(super) fn pen_style(graphs: [u32; 2]) -> Vec<(&'static str, Value)> {
    let blob = |bytes: Vec<u8>| Value::Blob(bytes);
    let words = |values: &[u32]| -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    };
    let (int, real) = (Value::Integer, Value::Real);
    vec![
        ("StyleFlag", int(115_200)),
        ("PenRadius", real(0.0)),
        (
            "SizeEffector",
            blob(words(&[
                0xd1,
                0x3f33_3333,
                graphs[0],
                0x3f33_3333,
                0x3f66_6666,
                graphs[1],
            ])),
        ),
        ("OpacityEffector", blob(words(&[1]))),
        ("FlowBase", real(1.0)),
        ("FlowEffector", blob(words(&[0x41, 0x3e38_51eb, graphs[1]]))),
        ("AntiAlias", int(1)),
        ("MipmapIndexToPlot", int(0)),
        ("Hardness", real(1.0)),
        ("IntervalBase", real(1.0)),
        ("IntervalEffector", blob(words(&[0]))),
        ("AutoIntervalType", int(3)),
        ("ThicknessBase", real(0.8)),
        ("ThicknessEffector", blob(words(&[1]))),
        ("RotationBase", real(90.0)),
        ("RotationInSprayBase", real(0.0)),
        ("RotationEffector", int(3)),
        ("RotationRandom", real(1.0)),
        ("RotationEffectorInSpray", int(3)),
        ("RotationRandomInSpray", real(1.0)),
        ("PatternStyle", int(0)),
        ("CompositeMode", int(0)),
        ("DualCompositeMode", int(1)),
        ("TexturePattern", int(0)),
        ("TextureFlag", int(0)),
        ("TextureComposite", int(0)),
        ("TextureScale", real(1.0)),
        ("TextureRotate", real(0.0)),
        ("TextureOffsetX", real(0.0)),
        ("TextureOffsetY", real(0.0)),
        ("TextureBrightness", real(0.0)),
        ("TextureContrast", real(0.0)),
        ("TextureDensityBase", real(1.0)),
        ("TextureDensityEffector", blob(words(&[0]))),
        ("UseWaterColor", int(0)),
        ("WaterColorType", int(0)),
        ("MixColorBase", real(1.0)),
        ("MixColorEffector", blob(words(&[0]))),
        ("MixAlphaBase", real(1.0)),
        ("MixAlphaEffector", blob(words(&[0]))),
        ("ColorExtension", real(0.0)),
        ("BrushColorMixingMode", int(0)),
        ("BrushLMSLinearity", int(0)),
        ("BlurKind", int(0)),
        ("BlurBase", real(0.0)),
        ("BlurEffector", blob(words(&[0]))),
        ("SubColorBase", real(0.0)),
        ("SubColorEffector", blob(words(&[0]))),
        ("HueChangeBase", real(0.0)),
        ("HueChangeEffector", blob(words(&[0]))),
        ("SaturationChangeBase", real(0.0)),
        ("SaturationChangeEffector", blob(words(&[0]))),
        ("ValueChangeBase", real(0.0)),
        ("ValueChangeEffector", blob(words(&[0]))),
        ("ChangeDrawColorTarget", int(1)),
        ("SprayFlag", int(0)),
        ("SpraySizeBase", real(1.0)),
        ("SpraySizeEffector", blob(words(&[0]))),
        ("SprayDensityBase", real(1.0)),
        ("SprayDensityEffector", blob(words(&[0]))),
        ("SprayBias", real(1.0)),
        ("FixedSpray", int(0)),
        ("WaterEdgeFlag", int(0)),
        ("WaterEdgeRadius", real(1.0)),
        ("WaterEdgeAlphaPower", real(0.0)),
        ("WaterEdgeValuePower", real(0.0)),
        ("WaterEdgeBlur", real(0.0)),
    ]
}

// ----- Reading a layer's native data -----------------------------------------------

/// The content Clip Studio's own columns of a layer describe (fill,
/// gradient or text; a vector layer's strokes are found by the reader,
/// which has the chunks).
#[must_use]
pub(super) fn content_of_columns(columns: &Columns) -> Option<ClipContent> {
    let blob = |name: &str| column(columns, name).and_then(Value::bytes);
    if let Some(info) = blob("GradationFillInfo") {
        return gradation_of(info);
    }
    if let (Some(string), Some(attributes)) = (blob("TextLayerString"), blob("TextLayerAttributes"))
    {
        return text_of(string, attributes).map(ClipContent::Text);
    }
    None
}

/// `content` as Clip Studio's own storage gives it back: two contents
/// that agree here write the same thing for Clip Studio.
#[must_use]
pub fn as_clip_studio_reads(content: &ClipContent, dpi: f64) -> Option<ClipContent> {
    match content {
        ClipContent::Fill(_) | ClipContent::Gradient(_) => gradation_of(&gradation_info(content)?),
        ClipContent::Text(text) => {
            clip_studio_fit(content)?;
            let blobs = text_blobs(text, dpi);
            text_of(&blobs.string, &blobs.attributes).map(ClipContent::Text)
        }
        ClipContent::Vector(vector) => {
            clip_studio_fit(content)?;
            vector_of(&vector_bytes(vector, 1)).map(ClipContent::Vector)
        }
        _ => None,
    }
}

/// A color as the three `Layer` columns of a palette color or a layer
/// color hold it.
#[must_use]
pub(super) fn color_columns(columns: &Columns, names: [&str; 3]) -> Option<[u8; 3]> {
    let channel = |name: &str| column(columns, name).and_then(Value::int).map(column_byte);
    Some([channel(names[0])?, channel(names[1])?, channel(names[2])?])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(text: &str) -> Vec<u8> {
        let text: String = text.split_whitespace().collect();
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    /// The default gradient of the survey's `05-gradient.clip`: FF3264
    /// to white, top to bottom of a 1600 by 1200 canvas.
    const SURVEY_GRADIENT: &str = "0000012c 00000002 0000000d 0047007200610064006100740069006f006e0044006100740061 00000050 00000018 0000001c 00000002 00000010 00000001 00000003 ffffffff 32323232 64646464 ffffffff 00000001 00000000 00000000 ffffffff ffffffff ffffffff ffffffff 00000002 00008000 00000000 00000010 0047007200610064006100740069006f006e00530065007400740069006e0067 00000000 00000000 00000001 4059000000000000 4059000000000000 0000000000000000 4089000000000000 0000000000000000 4089000000000000 4092c00000000000 00000017 0047007200610064006100740069006f006e00530065007400740069006e006700410064006400300030003000310000 0014 00000000 00000000 00000000 00000000 00000001";

    fn survey_gradient() -> ClipContent {
        ClipContent::Gradient(ClipGradient {
            shape: ClipGradientShape::Linear,
            start: [800.0, 0.0],
            end: [800.0, 1200.0],
            ratio: 1.0,
            repeat: ClipGradientRepeat::Clamp,
            stops: vec![
                ClipStop {
                    at: 0.0,
                    color: [255, 0x32, 0x64, 255],
                },
                ClipStop {
                    at: 1.0,
                    color: [255; 4],
                },
            ],
        })
    }

    #[test]
    fn a_gradient_is_written_byte_for_byte_as_clip_studio_wrote_it() {
        let blob = gradation_info(&survey_gradient()).unwrap();
        assert_eq!(blob, hex(SURVEY_GRADIENT));
        assert_eq!(gradation_of(&blob), Some(survey_gradient()));
    }

    #[test]
    fn a_fill_is_a_gradient_with_its_plain_color_on() {
        let fill = ClipContent::Fill(ClipFill {
            color: [255, 0x32, 0x64, 255],
        });
        let blob = gradation_info(&fill).unwrap();
        assert_eq!(blob.len(), 300);
        // The plain color part, as in the survey's `04-fill.clip`.
        assert!(blob.ends_with(&hex(
            "00000014 00000001 ffffffff 32323232 64646464 00000001"
        )));
        assert_eq!(gradation_of(&blob), Some(fill));
        // The survey's own blob reads as the gradient it is.
        let mut survey = hex(SURVEY_GRADIENT);
        let at = survey.len() - 20;
        survey[at..at + 4].copy_from_slice(&1_u32.to_be_bytes());
        survey[at + 4..at + 16].copy_from_slice(&hex("ffffffff 32323232 64646464"));
        assert_eq!(
            gradation_of(&survey),
            Some(ClipContent::Fill(ClipFill {
                color: [255, 0x32, 0x64, 255]
            }))
        );
    }

    #[test]
    fn gradients_clip_studio_has_no_place_for_are_not_written() {
        let ClipContent::Gradient(mut gradient) = survey_gradient() else {
            unreachable!()
        };
        gradient.shape = ClipGradientShape::Circle;
        let circle = ClipContent::Gradient(gradient.clone());
        assert_eq!(clip_studio_fit(&circle), None);
        assert_eq!(gradation_info(&circle), None);
        gradient.shape = ClipGradientShape::Linear;
        gradient.repeat = ClipGradientRepeat::Repeat;
        assert_eq!(gradation_info(&ClipContent::Gradient(gradient)), None);
        // A blob with another shape switch, or broken, is not read.
        let mut other = hex(SURVEY_GRADIENT);
        let at = 4 + 4 + 30 + 4 + 80 + 4 + 32;
        other[at + 11] = 2;
        assert_eq!(gradation_of(&other), None);
        let blob = hex(SURVEY_GRADIENT);
        assert_eq!(gradation_of(&blob[..blob.len() - 1]), None);
        assert_eq!(gradation_of(&[]), None);
    }

    fn hello() -> ClipText {
        ClipText {
            runs: vec![ClipTextRun {
                text: "Hello".to_owned(),
                font: "Tahoma".to_owned(),
                size: 30.0,
                color: [255, 0x32, 0x64, 255],
                bold: false,
                italic: false,
            }],
            at: [1057.0, 354.0],
            frame: Some([66.0, 36.0]),
            vertical: false,
            align: ClipAlign::Start,
            line_spacing: 1.0,
        }
    }

    #[test]
    fn a_text_is_written_record_for_record_as_clip_studio_wrote_it() {
        let blobs = text_blobs(&hello(), 72.0);
        assert_eq!(blobs.string, b"Hello");
        // The survey's text layer: 990 bytes of records, 882 of the short
        // list; the records Sue fills in from the text are the ones
        // checked here, the rest are Clip Studio's own defaults.
        assert_eq!((blobs.attributes.len(), blobs.add.len()), (990, 882));
        let records = records_of(&blobs.attributes).unwrap();
        let find = |id: u32| records.iter().find(|(found, _)| *found == id).unwrap().1;
        assert_eq!(
            find(11),
            hex("01000000 00000000 05000000 1a000000")
                .iter()
                .copied()
                .chain([0; 18])
                .collect::<Vec<u8>>()
        );
        assert_eq!(
            find(26),
            hex("01000000 00000000 05000000 10000000 0000000000005940 0000000000005940")
        );
        assert_eq!(
            find(13),
            hex("01000000 00000000 05000000 12000000 01000000 000000000000 0000000000005e40")
        );
        assert_eq!(find(31), b"Tahoma Regular");
        assert_eq!(find(32), hex("b80b0000"));
        assert_eq!(find(34), hex("ffffffff 32323232 64646464"));
        assert_eq!(find(39), hex("05000000 00000000"));
        assert_eq!(find(42), hex("21040000 62010000 63040000 86010000"));
        assert_eq!(
            find(47),
            hex("0400 3200 000000000000 0e00")
                .iter()
                .copied()
                .chain(*b"Tahoma Regular")
                .collect::<Vec<u8>>()
        );
        assert_eq!(
            find(57),
            hex("0100 0e00")
                .iter()
                .copied()
                .chain(*b"Tahoma Regular")
                .chain(hex("0600"))
                .chain(*b"Tahoma")
                .collect::<Vec<u8>>()
        );
        assert_eq!(find(63), hex("42000000 24000000"));
        assert_eq!(
            find(64),
            hex("00000000 00000000 c8190000 00000000 c8190000 100e0000 00000000 100e0000")
        );
        assert_eq!(find(71), hex("4076800000000000"));
        let add = records_of(&blobs.add[4..]).unwrap();
        assert_eq!(u32::from_le_bytes(blobs.add[..4].try_into().unwrap()), 878);
        let short = |id: u32| add.iter().find(|(found, _)| *found == id).unwrap().1;
        assert_eq!(
            short(11),
            hex("01000000 0e000000 0000000000000000000000000000")
        );
        assert_eq!(short(26), hex("01000000 04000000 00000000"));
        assert!(short(57).is_empty());
        assert_eq!(short(42), find(42));
    }

    #[test]
    fn a_text_reads_back_as_its_words_font_size_color_and_place() {
        let mut text = hello();
        text.frame = None;
        let blobs = text_blobs(&text, 72.0);
        assert_eq!(
            text_of(&blobs.string, &blobs.attributes),
            Some(text.clone())
        );
        assert_eq!(
            clip_studio_fit(&ClipContent::Text(text.clone())),
            Some(false)
        );
        // A box, other runs or bold have no place in Clip Studio's
        // storage as Sue writes it: the text goes, they are lost.
        text.runs[0].bold = true;
        assert_eq!(
            clip_studio_fit(&ClipContent::Text(text.clone())),
            Some(true)
        );
        text.runs[0].text.clear();
        assert_eq!(clip_studio_fit(&ClipContent::Text(text)), None);
        // A face name alone gives the family.
        let records = record_bytes(&[(31, b"Noto Sans Bold".to_vec())]);
        assert_eq!(text_of(b"x", &records).unwrap().runs[0].font, "Noto Sans");
        assert_eq!(text_of(b"x", &[1, 0, 0]), None);
        assert_eq!(text_of(&[0xff], &records), None);
    }

    fn pen_line() -> ClipLine {
        let widths = [0.0, 0.44, 0.56, 1.0, 1.0, 0.84, 0.0];
        ClipLine {
            points: widths
                .iter()
                .enumerate()
                .map(|(i, width)| ClipPoint {
                    at: [270.5 + 20.0 * i as f32, 965.25 - 3.5 * i as f32],
                    pressure: 1.0,
                    width: *width,
                    opacity: 1.0,
                    corner: false,
                })
                .collect(),
            width: 1.5,
            color: [255, 0x32, 0x64, 255],
            brush: String::new(),
            closed: false,
        }
    }

    #[test]
    fn a_vector_stroke_is_laid_out_as_clip_studio_lays_one_out() {
        let vector = ClipVector {
            lines: vec![pen_line()],
        };
        let bytes = vector_bytes(&vector, 1);
        assert_eq!(bytes.len(), 92 + 7 * 88);
        // The header as the survey's `02-vector.clip` has it, but for the
        // count of points and the box.
        assert_eq!(
            &bytes[..24],
            hex("0000005c 0000004c 00000058 00000058 00000007 00002081").as_slice()
        );
        assert_eq!(
            &bytes[40..92],
            hex("ffffffff 32323232 64646464 ffffffff ffffffff ffffffff 3ff0000000000000 00000000 00000001 3fe8000000000000 018009cc").as_slice()
        );
        // The first point: where it is, its box, and the width factor.
        assert_eq!(
            f64::from_be_bytes(bytes[92..100].try_into().unwrap()),
            270.5
        );
        assert_eq!(be32(&bytes, 92 + 16), Some(268));
        assert_eq!(be_f32_at(&bytes, 92 + 88 + 44), Some(0.44));
        assert_eq!(vector_of(&bytes), Some(vector.clone()));
        // Two lines one after the other.
        let mut second = pen_line();
        second.color = [0, 0, 0, 128];
        second.width = 6.0;
        let two = ClipVector {
            lines: vec![pen_line(), second],
        };
        let read = vector_of(&vector_bytes(&two, 3)).unwrap();
        assert_eq!(read.lines.len(), 2);
        assert_eq!(
            (read.lines[1].width, read.lines[1].color),
            (6.0, [0, 0, 0, 128])
        );
        assert_eq!(vector_of(&bytes[..bytes.len() - 1]), None);
        assert_eq!(vector_of(&[]), Some(ClipVector::default()));
    }

    #[test]
    fn what_clip_studio_reads_back_is_what_sue_compares() {
        let fill = ClipContent::Fill(ClipFill {
            color: [1, 2, 3, 255],
        });
        assert_eq!(as_clip_studio_reads(&fill, 72.0), Some(fill));
        let mut line = pen_line();
        line.brush = "size=3".to_owned();
        line.points[2].pressure = 0.3;
        let read =
            as_clip_studio_reads(&ClipContent::Vector(ClipVector { lines: vec![line] }), 72.0);
        let Some(ClipContent::Vector(read)) = read else {
            panic!("a vector reads back")
        };
        assert_eq!(read.lines[0].brush, "");
        assert_eq!(read.lines[0].points[2].pressure, 1.0);
        assert_eq!(column_byte(quad_column(200)), 200);
        assert_eq!(column_byte(200), 200);
    }
}
