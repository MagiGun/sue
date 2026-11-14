//! Icons written out as SVG files, from the same path data the screen is
//! drawn from: for review outside Sue and for whoever needs an icon in a
//! document or on a web page.

use std::fmt::Write as _;

use super::{CLEAR, Icon, IconStyle, Part, WEIGHT};

/// `icon` in `style` as an SVG document: the 24 unit canvas, the pen of
/// the set with round caps and joins, and `currentColor` as the one
/// color, so that the file takes the color of the text around it.
#[must_use]
pub fn svg(icon: Icon, style: IconStyle) -> String {
    document(icon.name(), icon.art().parts(style))
}

/// The SVG document of `parts`, titled `name`.
fn document(name: &str, parts: &[Part]) -> String {
    let mut body = String::new();
    let mut masks = 0;
    for part in parts {
        match part {
            Part::Stroke(data) => {
                let _ = writeln!(body, "  <path d=\"{data}\"/>");
            }
            Part::Fill(data) => {
                let _ = writeln!(
                    body,
                    "  <path d=\"{data}\" fill=\"currentColor\" stroke=\"none\"/>"
                );
            }
            Part::Clear(data) => {
                // Everything so far is drawn through a mask that is
                // black where the gap is.
                masks += 1;
                let gap = WEIGHT * CLEAR;
                let mut masked = String::new();
                let _ = writeln!(
                    masked,
                    "  <mask id=\"gap{masks}\" maskUnits=\"userSpaceOnUse\" x=\"0\" y=\"0\" \
                     width=\"24\" height=\"24\">"
                );
                let _ = writeln!(
                    masked,
                    "    <rect width=\"24\" height=\"24\" fill=\"#fff\" stroke=\"none\"/>"
                );
                let _ = writeln!(
                    masked,
                    "    <path d=\"{data}\" stroke=\"#000\" stroke-width=\"{gap}\"/>"
                );
                let _ = writeln!(masked, "  </mask>");
                let _ = writeln!(masked, "  <g mask=\"url(#gap{masks})\">");
                masked.push_str(&body);
                let _ = writeln!(masked, "  </g>");
                body = masked;
            }
        }
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" viewBox=\"0 0 24 \
         24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"{WEIGHT}\" \
         stroke-linecap=\"round\" stroke-linejoin=\"round\">\n  <title>{name}</title>\n{body}</svg>\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_is_a_whole_svg_document_with_its_path_data() {
        for icon in Icon::ALL {
            for style in IconStyle::ALL {
                let text = svg(*icon, style);
                assert!(text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""));
                assert!(text.ends_with("</svg>\n"));
                assert!(text.contains("viewBox=\"0 0 24 24\""));
                assert!(text.contains("stroke-width=\"1.5\""));
                assert!(text.contains("stroke-linecap=\"round\""));
                assert!(text.contains("stroke-linejoin=\"round\""));
                assert!(text.contains(&format!("<title>{}</title>", icon.name())));
                let parts = icon.art().parts(style);
                assert_eq!(text.matches("<path ").count(), parts.len());
                for part in parts {
                    assert!(text.contains(&format!("d=\"{}\"", part.data())));
                }
                // Every tag that opens closes.
                assert_eq!(text.matches("<g ").count(), text.matches("</g>").count());
                assert_eq!(
                    text.matches("<mask ").count(),
                    text.matches("</mask>").count()
                );
            }
        }
    }

    #[test]
    fn a_gap_masks_what_was_drawn_before_it_and_nothing_after() {
        let text = document(
            "crossed",
            &[
                Part::Stroke("M4 12 H20"),
                Part::Clear("M12 4 V20"),
                Part::Stroke("M12 4 V20"),
            ],
        );
        let mask = text.find("<mask id=\"gap1\"").unwrap();
        let group = text.find("<g mask=\"url(#gap1)\">").unwrap();
        let first = text.find("<path d=\"M4 12 H20\"/>").unwrap();
        let close = text.find("</g>").unwrap();
        let last = text.rfind("<path d=\"M12 4 V20\"/>").unwrap();
        assert!(mask < group && group < first && first < close && close < last);
        assert!(text.contains("<path d=\"M12 4 V20\" stroke=\"#000\" stroke-width=\"4.5\"/>"));
        assert!(text.contains("<title>crossed</title>"));
    }

    #[test]
    fn fills_take_the_color_and_no_pen_and_the_solid_twin_differs() {
        let line = svg(Icon::Heart, IconStyle::Line);
        let solid = svg(Icon::Heart, IconStyle::Solid);
        assert!(!line.contains("fill=\"currentColor\""));
        assert!(solid.contains("fill=\"currentColor\" stroke=\"none\""));
        assert_ne!(line, solid);
        // No twin: the same file for both styles.
        assert_eq!(
            svg(Icon::Search, IconStyle::Line),
            svg(Icon::Search, IconStyle::Solid)
        );
    }
}
