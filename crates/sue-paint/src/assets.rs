//! CLIP STUDIO ASSETS on Sue Paint's start page (Browse Assets), as Clip
//! Studio's own start page shows it: rows of today's top materials, a
//! row's full ranking (See more), and a material's own page. The pages are read from
//! <https://assets.clip-studio.com/en-us/> and laid out by Sue; each page
//! and picture is kept in Sue's cache folder, so when the site cannot be
//! reached the copy from last time shows instead. Materials download on
//! the site itself, with a CLIP STUDIO account.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use egui::{Align2, Color32, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};
use sue_platform::update::Job;
use sue_wm::{AppletContext, ShellRequest, Theme};

/// The site.
pub(crate) const SITE: &str = "https://assets.clip-studio.com";
/// Its English home page.
pub(crate) const HOME: &str = "https://assets.clip-studio.com/en-us/";
/// A page kept this long is fresh enough not to ask again.
const FRESH: Duration = Duration::from_secs(30 * 60);
/// At most this many fetches at once.
const AT_ONCE: usize = 6;

/// A material as a card lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetCard {
    pub id: String,
    pub title: String,
    /// What kind: Material catalog, Brush, 3D object and so on.
    pub kind: String,
    pub author: String,
    pub thumbnail: String,
    pub downloads: String,
    /// Free, or what it costs.
    pub price: String,
}

/// A row of the home page: its title, the full list behind See more, and
/// its cards.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Section {
    pub title: String,
    pub more: String,
    pub cards: Vec<AssetCard>,
}

/// One material's page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct AssetPage {
    pub id: String,
    pub title: String,
    pub thumbnail: String,
    pub downloads: String,
    pub likes: String,
    /// What kind, and which programs it is for.
    pub tags: Vec<String>,
    pub author: String,
    pub price: String,
    /// What the author says first, a line each.
    pub lead: Vec<String>,
    /// The description: words and pictures in order.
    pub description: Vec<Block>,
    /// The materials in the set: each one's picture and name.
    pub contents: Vec<(String, String)>,
}

/// A piece of a description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Text(String),
    Picture(String),
}

/// A link made whole: `/en-us/...` and `//host/...` gain the site.
pub(crate) fn absolute(link: &str) -> String {
    if let Some(rest) = link.strip_prefix("//") {
        format!("https://{rest}")
    } else if link.starts_with('/') {
        format!("{SITE}{link}")
    } else {
        link.to_owned()
    }
}

/// The page of material `id`.
pub(crate) fn detail_url(id: &str) -> String {
    format!("{SITE}/en-us/detail?id={id}")
}

// ----- Reading the pages --------------------------------------------------------

/// HTML's entities as their characters.
fn entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

/// The words of `html`: tags gone (a line break for `<br>` and the ends of
/// blocks), entities read, spaces gathered.
fn words(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            break;
        };
        let tag = rest[open + 1..open + close].to_ascii_lowercase();
        if tag.starts_with("br") || tag.starts_with("/div") || tag.starts_with("/p") {
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    entities(&out)
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned()
}

/// What lies between `from` (after it) and the next `to` in `text`.
fn between<'a>(text: &'a str, from: &str, to: &str) -> Option<&'a str> {
    let start = text.find(from)? + from.len();
    let end = text[start..].find(to)? + start;
    Some(&text[start..end])
}

/// The value of the first `name="..."` in `text`.
fn attribute(text: &str, name: &str) -> Option<String> {
    between(text, &format!("{name}=\""), "\"").map(entities)
}

/// The translated words in `text` (the site shows English and keeps the
/// author's own words beside them).
fn translated(text: &str) -> Option<String> {
    let words = words(between(text, "data-translated-text>", "</")?);
    (!words.is_empty()).then_some(words)
}

/// Every material card in `html`, in order.
pub(crate) fn parse_cards(html: &str) -> Vec<AssetCard> {
    html.split("<li class=\"materialCard\"")
        .skip(1)
        .filter_map(|card| {
            let id = between(card, "detail?id=", "\"")?
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>();
            if id.is_empty() {
                return None;
            }
            let thumbnail = card
                .split("materialCard__thumbmailImage")
                .nth(1)
                .and_then(|after| attribute(after, "src"))
                .unwrap_or_default();
            let kind = between(card, "materialCard__materialTypeLink\"", "</a>")
                .and_then(|link| link.split_once('>'))
                .map(|(_, text)| words(text))
                .unwrap_or_default();
            let title = card
                .split("<h6")
                .nth(1)
                .and_then(|heading| {
                    translated(heading).or_else(|| {
                        let inner = heading.split_once('>')?.1.split("</h6>").next()?;
                        let text = words(inner);
                        (!text.is_empty()).then_some(text)
                    })
                })
                .unwrap_or_default();
            let author = card
                .split("materialCard__cardAuthor")
                .nth(1)
                .and_then(|after| between(after, "<p>", "</p>"))
                .map(words)
                .unwrap_or_default();
            let downloads = between(card, "materialCard__favoriteCount\">", "</div>")
                .map(words)
                .unwrap_or_default();
            let price = between(card, "<ul class=\"materialCard__price\">", "</ul>")
                .map(|list| words(list).split_whitespace().collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            Some(AssetCard {
                id,
                title,
                kind,
                author,
                thumbnail: absolute(&thumbnail),
                downloads,
                price,
            })
        })
        .collect()
}

/// The home page's rows, each with its cards.
pub(crate) fn parse_home(html: &str) -> Vec<Section> {
    let heads: Vec<usize> = html
        .match_indices("<div class=\"materialHeader index\">")
        .map(|(at, _)| at)
        .collect();
    heads
        .iter()
        .enumerate()
        .filter_map(|(index, &at)| {
            let end = heads.get(index + 1).copied().unwrap_or(html.len());
            let part = &html[at..end];
            let title = words(between(part, "<h4>", "<small>")?);
            let more = between(part, "<small>", "</small>")
                .and_then(|small| attribute(small, "href"))
                .map(|link| absolute(&link))
                .unwrap_or_default();
            let cards = parse_cards(part);
            (!cards.is_empty()).then_some(Section { title, more, cards })
        })
        .collect()
}

/// A material's page.
pub(crate) fn parse_page(html: &str, id: &str) -> AssetPage {
    let header = html
        .split("layout__materialHeader\"")
        .nth(1)
        .unwrap_or(html);
    let title = header
        .split("materialHeaderTitle\">")
        .nth(1)
        .and_then(translated)
        .unwrap_or_default();
    let thumbnail = header
        .split("materialHeaderThumbnail--pc")
        .nth(1)
        .and_then(|after| attribute(after, "src"))
        .map(|link| absolute(&link))
        .unwrap_or_default();
    let downloads = between(html, "downloadCount__count\">", "</span>")
        .map(words)
        .unwrap_or_default();
    let likes = between(html, "data-favorite-count-view>", "</span>")
        .map(words)
        .unwrap_or_default();
    let tags = between(html, "<ul class=\"headTags\">", "</ul>")
        .map(|list| {
            list.split("<a ")
                .skip(1)
                .filter_map(|link| link.split_once('>'))
                .map(|(_, text)| words(text.split("</a>").next().unwrap_or("")))
                .filter(|tag| !tag.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let author = between(html, "authorTop__name\">", "</span>")
        .map(words)
        .unwrap_or_default();
    let price = between(html, "<div class=\"price\">", "</div>")
        .or_else(|| between(html, "class=\"price__", "</"))
        .map(|text| words(text.split_once('>').map_or(text, |(_, rest)| rest)))
        .filter(|price| !price.is_empty())
        .unwrap_or_else(|| tr!("assets-free"));
    let lead = html
        .split("<div class=\"materialLead\">")
        .nth(1)
        .and_then(translated)
        .map(|text| text.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    let mut description = Vec::new();
    if let Some(body) = html
        .split("materialDescriptionContent\">")
        .nth(1)
        .and_then(|after| between(after, "data-translated-text>", "<div class=\"hide\""))
    {
        // Words and pictures in their order.
        let mut rest = body;
        while let Some(at) = rest.find("<img") {
            let text = words(&rest[..at]);
            if !text.is_empty() {
                description.push(Block::Text(text));
            }
            let tag_end = rest[at..].find('>').map_or(rest.len(), |end| at + end + 1);
            if let Some(src) = attribute(&rest[at..tag_end], "src") {
                description.push(Block::Picture(absolute(&src)));
            }
            rest = &rest[tag_end..];
        }
        let text = words(rest);
        if !text.is_empty() {
            description.push(Block::Text(text));
        }
    }
    let contents = html
        .split("materialContentItem__item")
        .skip(1)
        .filter_map(|item| {
            let picture = attribute(item.split("<img").nth(1)?, "src")?;
            let name = item
                .split("materialContentItem__name")
                .nth(1)
                .and_then(translated)
                .unwrap_or_default();
            Some((absolute(&picture), name))
        })
        .collect();
    AssetPage {
        id: id.to_owned(),
        title,
        thumbnail,
        downloads,
        likes,
        tags,
        author,
        price,
        lead,
        description,
        contents,
    }
}

// ----- Fetching and keeping them --------------------------------------------------

/// Where a page or picture stands.
enum Fetch {
    /// Waiting for a free fetch.
    Queued,
    /// Being fetched.
    Running(Job<Result<Vec<u8>, String>>),
    /// Fetched, or read from the cache.
    Done,
}

/// A picture ready to show.
pub(crate) struct Picture {
    pub size: [u32; 2],
    pub pixels: Vec<u8>,
}

/// The pages and pictures of CLIP STUDIO ASSETS, fetched in the background
/// and kept in the cache folder.
#[derive(Default)]
pub(crate) struct Assets {
    folder: Option<PathBuf>,
    fetches: HashMap<String, Fetch>,
    /// The urls that are pictures (the rest are pages).
    picture_urls: std::collections::HashSet<String>,
    queue: VecDeque<String>,
    pages: HashMap<String, String>,
    pictures: HashMap<String, Option<Picture>>,
    /// Pages the site did not give this time (shown from the cache).
    stale: std::collections::HashSet<String>,
}

impl std::fmt::Debug for Assets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Assets")
            .field("pages", &self.pages.len())
            .field("pictures", &self.pictures.len())
            .finish_non_exhaustive()
    }
}

/// A file name for `url` in the cache.
fn cache_name(url: &str) -> String {
    format!("{:016x}", crate::start::texture_key(url))
}

/// The biggest side a kept picture has (cards are small; a page's
/// pictures fit the page).
fn fit(tile: sue_core::RasterTile2D, most: u32) -> Option<Picture> {
    let (width, height) = (tile.width(), tile.height());
    let bytes = tile.bytes()?;
    if width.max(height) <= most {
        return Some(Picture {
            size: [width, height],
            pixels: bytes.to_vec(),
        });
    }
    let scale = most as f32 / width.max(height) as f32;
    let (w, h) = (
        ((width as f32 * scale) as u32).max(1),
        ((height as f32 * scale) as u32).max(1),
    );
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let sx = ((x as f32 + 0.5) / scale) as u32;
            let sy = ((y as f32 + 0.5) / scale) as u32;
            let at = ((sy.min(height - 1) * width + sx.min(width - 1)) * 4) as usize;
            pixels.extend_from_slice(&bytes[at..at + 4]);
        }
    }
    Some(Picture {
        size: [w, h],
        pixels,
    })
}

/// A JPEG, PNG, WebP or GIF picture as RGBA.
fn decode(bytes: &[u8]) -> Option<sue_core::RasterTile2D> {
    sue_io::decode_jpeg(bytes)
        .map(|decoded| decoded.tile)
        .or_else(|_| sue_io::decode_png(bytes).map(|decoded| decoded.tile))
        .or_else(|_| sue_io::decode_picture(bytes, None))
        .ok()
}

impl Assets {
    /// Keeps pages and pictures in `folder` (Sue's cache).
    pub(crate) fn use_folder(&mut self, folder: Option<PathBuf>) {
        if self.folder != folder {
            if let Some(folder) = &folder {
                let _ = std::fs::create_dir_all(folder);
            }
            self.folder = folder;
        }
    }

    fn cached(&self, url: &str) -> Option<(Vec<u8>, bool)> {
        let path = self.folder.as_ref()?.join(cache_name(url));
        let bytes = std::fs::read(&path).ok()?;
        let fresh = std::fs::metadata(&path)
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|at| SystemTime::now().duration_since(at).ok())
            .is_some_and(|age| age < FRESH);
        Some((bytes, fresh))
    }

    fn keep(&self, url: &str, bytes: &[u8]) {
        if let Some(folder) = &self.folder {
            let _ = std::fs::write(folder.join(cache_name(url)), bytes);
        }
    }

    /// Asks for `url` if it is not here yet; the cached copy shows
    /// meanwhile, and a fresh one replaces it once fetched.
    fn want(&mut self, url: &str, picture: bool) {
        if self.fetches.contains_key(url) {
            return;
        }
        if picture {
            self.picture_urls.insert(url.to_owned());
        }
        let cached = self.cached(url);
        let fresh = cached.as_ref().is_some_and(|(_, fresh)| *fresh);
        if let Some((bytes, _)) = cached {
            self.store(url, &bytes, picture);
        }
        if fresh || (picture && self.pictures.contains_key(url)) {
            self.fetches.insert(url.to_owned(), Fetch::Done);
        } else {
            self.fetches.insert(url.to_owned(), Fetch::Queued);
            self.queue.push_back(url.to_owned());
        }
    }

    fn store(&mut self, url: &str, bytes: &[u8], picture: bool) {
        if picture {
            let most = if url.contains("/thumbnail") || url.contains("/thumb/") {
                320
            } else {
                1100
            };
            self.pictures.insert(
                url.to_owned(),
                decode(bytes).and_then(|tile| fit(tile, most)),
            );
        } else {
            self.pages
                .insert(url.to_owned(), String::from_utf8_lossy(bytes).into_owned());
        }
    }

    /// Starts queued fetches and takes in finished ones. Returns `true`
    /// while any are going (so the page draws again).
    pub(crate) fn poll(&mut self) -> bool {
        let running = self
            .fetches
            .values()
            .filter(|fetch| matches!(fetch, Fetch::Running(_)))
            .count();
        for _ in running..AT_ONCE {
            let Some(url) = self.queue.pop_front() else {
                break;
            };
            let job = sue_platform::update::fetch(&url, concat!("Sue/", env!("CARGO_PKG_VERSION")));
            self.fetches.insert(url, Fetch::Running(job));
        }
        let mut finished = Vec::new();
        for (url, fetch) in &self.fetches {
            if let Fetch::Running(job) = fetch
                && let Some(answer) = job.take()
            {
                finished.push((url.clone(), answer));
            }
        }
        for (url, answer) in finished {
            let picture = self.picture_urls.contains(&url);
            match answer {
                Ok(bytes) if !bytes.is_empty() => {
                    self.keep(&url, &bytes);
                    self.store(&url, &bytes, picture);
                    self.stale.remove(&url);
                }
                _ => {
                    self.stale.insert(url.clone());
                }
            }
            self.fetches.insert(url, Fetch::Done);
        }
        self.fetches
            .values()
            .any(|fetch| matches!(fetch, Fetch::Queued | Fetch::Running(_)))
    }

    /// The page at `url`: `None` while there is no copy at all.
    pub(crate) fn page(&mut self, url: &str) -> Option<&str> {
        self.want(url, false);
        self.pages.get(url).map(String::as_str)
    }

    /// `true` when the page shown is the copy from last time (the site
    /// could not be reached).
    pub(crate) fn offline(&self, url: &str) -> bool {
        self.stale.contains(url)
    }

    /// `true` while `url` is being fetched with nothing to show yet.
    pub(crate) fn loading(&self, url: &str) -> bool {
        !self.pages.contains_key(url)
            && self
                .fetches
                .get(url)
                .is_some_and(|fetch| matches!(fetch, Fetch::Queued | Fetch::Running(_)))
    }

    /// The picture at `url` as a texture, once there is one.
    pub(crate) fn texture(
        &mut self,
        url: &str,
        cx: &mut AppletContext<'_>,
    ) -> Option<(egui::TextureId, [u32; 2])> {
        if url.is_empty() {
            return None;
        }
        self.want(url, true);
        let picture = self.pictures.get(url)?.as_ref()?;
        let key = crate::start::texture_key(url);
        cx.textures
            .texture(key, 1, picture.size, 4, &picture.pixels, false)
            .map(|texture| (texture, picture.size))
    }

    /// Fetches page `url` again, whatever the cache holds.
    pub(crate) fn reload(&mut self, url: &str) {
        self.fetches.remove(url);
        self.stale.remove(url);
        if let Some(folder) = &self.folder {
            let _ = std::fs::remove_file(folder.join(cache_name(url)));
        }
        self.fetches.insert(url.to_owned(), Fetch::Queued);
        self.queue.push_back(url.to_owned());
    }
}

// ----- Showing them -----------------------------------------------------------------

/// Where the start page's browser is: Home, a ranking, or a material.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Place {
    /// A full list of materials (See more).
    List(String),
    /// A material's page, by id.
    Material(String),
}

/// The start page's browser: where it is, and where it was.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Browser {
    pub back: Vec<Option<Place>>,
    pub forward: Vec<Option<Place>>,
    /// `None` is Home.
    pub place: Option<Place>,
}

impl Browser {
    pub(crate) fn go(&mut self, place: Option<Place>) {
        if place != self.place {
            self.back.push(std::mem::replace(&mut self.place, place));
            self.forward.clear();
        }
    }

    pub(crate) fn back(&mut self) {
        if let Some(place) = self.back.pop() {
            self.forward.push(std::mem::replace(&mut self.place, place));
        }
    }

    pub(crate) fn forward(&mut self) {
        if let Some(place) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.place, place));
        }
    }
}

/// What a click in the assets asks for.
pub(crate) enum Go {
    Place(Place),
    /// The page on the site itself, in the system's browser.
    Site(String),
}

/// A card as Clip Studio's start page draws it: the picture at the left,
/// its kind, name, author, downloads and price at the right. Returns a
/// click.
fn card_ui(
    ui: &mut Ui,
    rect: Rect,
    card: &AssetCard,
    rank: Option<usize>,
    assets: &mut Assets,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> bool {
    let response = ui.interact(
        rect,
        egui::Id::unique((
            "sue-paint-asset-card",
            &card.id,
            rect.min.x as i32,
            rect.min.y as i32,
        )),
        Sense::click(),
    );
    let painter = ui.painter().with_clip_rect(rect);
    if response.hovered() {
        painter.rect_filled(rect, 4.0, theme.hover);
    }
    let side = rect.height() - 8.0;
    let frame = Rect::from_min_size(rect.min + vec2(4.0, 4.0), vec2(side, side));
    painter.rect_filled(frame, 0.0, Color32::WHITE);
    if let Some((texture, size)) = assets.texture(&card.thumbnail, cx) {
        let scale = (side - 6.0) / size[0].max(size[1]) as f32;
        let shown = Rect::from_center_size(
            frame.center(),
            vec2(size[0] as f32 * scale, size[1] as f32 * scale),
        );
        painter.image(
            texture,
            shown,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    painter.rect_stroke(frame, 0.0, theme.hairline(), egui::StrokeKind::Inside);
    if let Some(rank) = rank {
        painter.text(
            frame.left_top() + vec2(6.0, 4.0),
            Align2::LEFT_TOP,
            format!("{rank}"),
            theme.font(theme.ui_size + 4.0),
            Color32::from_rgb(220, 170, 40),
        );
    }
    let x = frame.right() + 8.0;
    let mut y = rect.top() + 6.0;
    let mut line = |text: &str, size: f32, color: Color32, rows: usize| {
        // At most `rows` lines, the last cut with an ellipsis.
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            theme.font(size),
            color,
            (rect.right() - x - 4.0).max(20.0),
        );
        job.wrap.max_rows = rows;
        job.wrap.overflow_character = Some('\u{2026}');
        let galley = painter.layout_job(job);
        let height = galley.size().y;
        painter.galley(pos2(x, y), galley, color);
        y += height + 2.0;
    };
    line(&card.kind, theme.small_size, theme.muted, 1);
    line(&card.title, theme.ui_size, theme.text, 2);
    line(&card.author, theme.small_size, theme.muted, 1);
    let downloads = pos2(x, rect.bottom() - 24.0);
    download_mark(&painter, downloads + vec2(5.0, -7.0), theme.muted);
    painter.text(
        downloads + vec2(13.0, 0.0),
        Align2::LEFT_BOTTOM,
        &card.downloads,
        theme.font(theme.small_size),
        theme.muted,
    );
    painter.text(
        pos2(x, rect.bottom() - 6.0),
        Align2::LEFT_BOTTOM,
        &card.price,
        theme.font(theme.small_size + 0.5),
        if card.price.eq_ignore_ascii_case("free") {
            Color32::from_rgb(110, 200, 90)
        } else {
            Color32::from_rgb(90, 160, 230)
        },
    );
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

/// A small download arrow over a tray, centered on `at`.
fn download_mark(painter: &egui::Painter, at: egui::Pos2, color: Color32) {
    let stroke = Stroke::new(1.3, color);
    painter.line_segment([at + vec2(0.0, -5.0), at + vec2(0.0, 2.0)], stroke);
    painter.line_segment([at + vec2(-3.0, -1.0), at + vec2(0.0, 2.0)], stroke);
    painter.line_segment([at + vec2(3.0, -1.0), at + vec2(0.0, 2.0)], stroke);
    painter.line_segment([at + vec2(-4.0, 5.0), at + vec2(4.0, 5.0)], stroke);
}

/// Cards in rows across `width`, each `size`: the clicked one's id.
fn grid(
    ui: &mut Ui,
    cards: &[AssetCard],
    rows: Option<usize>,
    ranked: bool,
    assets: &mut Assets,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> Option<String> {
    let width = ui.available_width();
    let size = vec2(300.0, 168.0);
    let columns = ((width / (size.x + 12.0)).floor() as usize).max(1);
    let shown = rows.map_or(cards.len(), |rows| (rows * columns).min(cards.len()));
    let lines = shown.div_ceil(columns);
    let (area, _) =
        ui.allocate_exact_size(vec2(width, lines as f32 * (size.y + 12.0)), Sense::hover());
    let mut clicked = None;
    for (index, card) in cards.iter().take(shown).enumerate() {
        let rect = Rect::from_min_size(
            area.min
                + vec2(
                    (index % columns) as f32 * (size.x + 12.0),
                    (index / columns) as f32 * (size.y + 12.0),
                ),
            size,
        );
        if !ui.is_rect_visible(rect) {
            continue;
        }
        if card_ui(
            ui,
            rect,
            card,
            ranked.then_some(index + 1),
            assets,
            cx,
            theme,
        ) {
            clicked = Some(card.id.clone());
        }
    }
    clicked
}

/// A heading with See more and the site's own page beside it.
fn heading(ui: &mut Ui, title: &str, more: Option<&str>, theme: &Theme) -> Option<Go> {
    let mut go = None;
    ui.add_space(14.0);
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(title)
                .size(theme.heading_size)
                .color(theme.text),
        );
        if let Some(more) = more.filter(|more| !more.is_empty()) {
            if ui
                .link(RichText::new(tr!("assets-see-more")).size(theme.small_size + 0.5))
                .clicked()
            {
                go = Some(Go::Place(Place::List(more.to_owned())));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .link(RichText::new(tr!("assets-on-the-site")).size(theme.small_size + 0.5))
                    .clicked()
                {
                    go = Some(Go::Site(more.to_owned()));
                }
            });
        }
    });
    let rect = ui.available_rect_before_wrap();
    ui.painter().hline(
        rect.x_range(),
        rect.top() + 2.0,
        Stroke::new(1.0, theme.border_active.gamma_multiply(0.3)),
    );
    ui.add_space(8.0);
    go
}

/// A note: loading, or showing last time's copy.
fn status(ui: &mut Ui, assets: &Assets, url: &str, theme: &Theme) {
    if assets.loading(url) {
        ui.label(RichText::new(tr!("assets-loading")).color(theme.muted));
    } else if assets.offline(url) {
        ui.label(
            RichText::new(tr!("assets-offline"))
                .size(theme.small_size + 0.5)
                .color(theme.muted),
        );
    }
}

/// Home's rows of CLIP STUDIO ASSETS.
pub(crate) fn home_rows(
    ui: &mut Ui,
    assets: &mut Assets,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> Option<Go> {
    let mut go = None;
    let sections = assets.page(HOME).map(parse_home).unwrap_or_default();
    if sections.is_empty() {
        heading(ui, &tr!("assets-title"), None, theme);
        status(ui, assets, HOME, theme);
        if !assets.loading(HOME) && ui.button(tr!("assets-try-again")).clicked() {
            assets.reload(HOME);
        }
        return None;
    }
    status(ui, assets, HOME, theme);
    for section in &sections {
        if let Some(more) = heading(ui, &section.title, Some(&section.more), theme) {
            go = Some(more);
        }
        if let Some(id) = grid(ui, &section.cards, Some(2), false, assets, cx, theme) {
            go = Some(Go::Place(Place::Material(id)));
        }
    }
    ui.add_space(10.0);
    ui.label(
        RichText::new(tr!("assets-credit"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    go
}

/// A full list (See more), ranked.
pub(crate) fn list_page(
    ui: &mut Ui,
    url: &str,
    assets: &mut Assets,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> Option<Go> {
    let html = assets.page(url).map(str::to_owned);
    let title = html
        .as_deref()
        .and_then(|html| between(html, "<title>", "</title>"))
        .map(words)
        .unwrap_or_else(|| tr!("assets-title"));
    let mut go = heading(ui, &title, None, theme);
    status(ui, assets, url, theme);
    let cards = html.as_deref().map(parse_cards).unwrap_or_default();
    if let Some(id) = grid(ui, &cards, None, url.contains("ranking"), assets, cx, theme) {
        go = Some(Go::Place(Place::Material(id)));
    }
    go
}

/// A material's page: its picture and name, downloads and likes, kind and
/// programs, price and the way to it on the site, then the author's words
/// and pictures and what the set holds.
pub(crate) fn material_page(
    ui: &mut Ui,
    id: &str,
    assets: &mut Assets,
    cx: &mut AppletContext<'_>,
    theme: &Theme,
) -> Option<Go> {
    let url = detail_url(id);
    let mut go = None;
    let Some(html) = assets.page(&url).map(str::to_owned) else {
        status(ui, assets, &url, theme);
        return None;
    };
    status(ui, assets, &url, theme);
    let page = parse_page(&html, id);
    let width = ui.available_width().min(980.0);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let (frame, _) = ui.allocate_exact_size(vec2(72.0, 72.0), Sense::hover());
        ui.painter().rect_filled(frame, 0.0, Color32::WHITE);
        if let Some((texture, _)) = assets.texture(&page.thumbnail, cx) {
            ui.painter().image(
                texture,
                frame.shrink(2.0),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        ui.vertical(|ui| {
            ui.set_width((width - 340.0).max(200.0));
            ui.label(
                RichText::new(&page.title)
                    .size(theme.ui_size + 2.0)
                    .color(theme.text),
            );
            ui.label(
                RichText::new(tr!("assets-content-id", id = page.id.clone()))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            ui.horizontal_wrapped(|ui| {
                let (mark, _) = ui.allocate_exact_size(vec2(12.0, 16.0), Sense::hover());
                download_mark(ui.painter(), mark.center(), theme.muted);
                ui.label(RichText::new(&page.downloads).color(theme.muted));
                ui.label(RichText::new(format!("\u{2665} {}", page.likes)).color(theme.muted));
                for tag in &page.tags {
                    ui.label(
                        RichText::new(format!(" {tag} "))
                            .size(theme.small_size + 0.5)
                            .background_color(theme.pill)
                            .color(theme.text),
                    );
                }
            });
        });
        ui.vertical(|ui| {
            ui.set_width(220.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(&page.price)
                        .size(theme.ui_size + 1.0)
                        .color(theme.text),
                );
                let button =
                    egui::Button::new(RichText::new(tr!("assets-get-it")).color(Color32::WHITE))
                        .fill(Color32::from_rgb(208, 72, 72))
                        .min_size(vec2(220.0, 32.0));
                if ui
                    .add(button)
                    .on_hover_text(tr!("assets-get-it-hint"))
                    .clicked()
                {
                    go = Some(Go::Site(url.clone()));
                }
            });
        });
    });
    if !page.author.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new(tr!("assets-by", author = page.author.clone())).color(theme.muted));
    }
    ui.separator();
    if !page.lead.is_empty() {
        ui.label(RichText::new(page.lead.join("\n")).color(theme.text));
    }
    ui.add_space(8.0);
    for block in &page.description {
        match block {
            Block::Text(text) => {
                ui.label(RichText::new(text).color(theme.text));
            }
            Block::Picture(src) => {
                if let Some((texture, size)) = assets.texture(src, cx) {
                    let scale = (width / size[0].max(1) as f32).min(1.0);
                    ui.image((
                        texture,
                        vec2(size[0] as f32 * scale, size[1] as f32 * scale),
                    ));
                } else {
                    ui.add_space(40.0);
                }
            }
        }
    }
    if !page.contents.is_empty() {
        ui.add_space(10.0);
        ui.label(
            RichText::new(tr!("assets-in-this-set"))
                .size(theme.ui_size + 1.0)
                .color(theme.text),
        );
        ui.horizontal_wrapped(|ui| {
            for (picture, name) in &page.contents {
                ui.vertical(|ui| {
                    ui.set_width(140.0);
                    let (frame, _) = ui.allocate_exact_size(vec2(140.0, 140.0), Sense::hover());
                    ui.painter().rect_filled(frame, 0.0, Color32::WHITE);
                    if let Some((texture, _)) = assets.texture(picture, cx) {
                        ui.painter().image(
                            texture,
                            frame.shrink(2.0),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    ui.label(
                        RichText::new(name)
                            .size(theme.small_size + 0.5)
                            .color(theme.text),
                    );
                });
            }
        });
    }
    go
}

/// Opens `link` in the system's browser.
pub(crate) fn open_site(link: &str, cx: &mut AppletContext<'_>) {
    cx.request(ShellRequest::Command(format!("open-url:{link}")));
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = r#"<li class="materialCard" data-material-card>
        <a href="/en-us/detail?id=2317055" class="materialCard__cardContentBlock">
        <img class="materialCard__thumbmailImage"
             src="https://celclipmaterialprod.s3-ap-northeast-1.amazonaws.com/55/70/2317055/thumbnail?1793078241" alt=""/></a>
        <div class="materialCard__materialType"><a href="/en-us/search?type=catalog"
           class="materialCard__materialTypeLink">
            Material catalog
        </a></div>
        <a href="/en-us/detail?id=2317055"><h6 title="x"><span data-translated-text>
            Detail and material brush set
        </span><span class="hide" data-original-text>x</span></h6></a>
        <div class="materialCard__cardAuthor "><a href="/en-us/search?user=a"><div><img src="a.png"/>
            <p>Lemon 22</p></a></div>
        <div class="materialCard__favoriteCount"><i class="fa fa-download"></i>
            17,518
        </div>
        <ul class="materialCard__price"><li class="materialCard__priceFree">Free</li></ul>
    </li>"#;

    #[test]
    fn cards_read_their_name_kind_author_downloads_and_price() {
        let home = format!(
            "<div class=\"materialHeader index\"><h4>Today&#039;s top materials (Free)<small>\
             <a href=\"/en-us/ranking?freq=daily&amp;sales=free\">View more</a></small></h4>\
             <ul>{CARD}{CARD}</ul>"
        );
        let sections = parse_home(&home);
        assert_eq!(sections.len(), 1);
        let section = &sections[0];
        assert_eq!(section.title, "Today's top materials (Free)");
        assert_eq!(
            section.more,
            "https://assets.clip-studio.com/en-us/ranking?freq=daily&sales=free"
        );
        assert_eq!(section.cards.len(), 2);
        let card = &section.cards[0];
        assert_eq!(card.id, "2317055");
        assert_eq!(card.title, "Detail and material brush set");
        assert_eq!(card.kind, "Material catalog");
        assert_eq!(card.author, "Lemon 22");
        assert_eq!(card.downloads, "17,518");
        assert_eq!(card.price, "Free");
        assert!(card.thumbnail.ends_with("/2317055/thumbnail?1793078241"));
    }

    #[test]
    fn a_material_page_reads_its_words_pictures_and_set() {
        let html = r#"<section class="layout__materialHeader">
            <span class="materialHeaderThumbnail--pc"><img class="objectFitImage" src="https://x/thumbnail?1" /></span>
            <h1 class="materialHeaderTitle"><span data-translated-text>
                Detail and material brush set
            </span><span class="hide" data-original-text>x</span></h1></section>
            <span class="downloadCount__count"> 17,524 </span>
            <span class="favoriteButton__count" data-favorite-count-view> 1,071 </span>
            <ul class="headTags"><li><a href="/a">Material catalog</a></li>
            <li><a href="/b">CLIP STUDIO PAINT PRO/EX</a></li></ul>
            <span class="authorTop__name"> Lemon 22 </span>
            <div class="materialLead"><p><span data-translated-text>
                Four brushes<br />The Detail Pen
            </span><span class="hide" data-original-text>x</span></p></div>
            <div class="materialDescriptionContent"><div data-translated-text>
                <div>Before</div><div><img src="//assets.clip-studio.com/en-us/description/1/img/a.jpg"></div><div>After &amp; more</div>
            </div><div class="hide" data-original-text>x</div></div>
            <li class="materialContentItem__item"><a href="/x"><img class="objectFitImage" src="/en-us/description/thumb/1/a/1" />
            <div class="materialContentItem__name"><span data-translated-text> Detail Pen </span></div></a></li>"#;
        let page = parse_page(html, "2317055");
        assert_eq!(page.title, "Detail and material brush set");
        assert_eq!(page.downloads, "17,524");
        assert_eq!(page.likes, "1,071");
        assert_eq!(page.tags, ["Material catalog", "CLIP STUDIO PAINT PRO/EX"]);
        assert_eq!(page.author, "Lemon 22");
        assert_eq!(page.lead, ["Four brushes", "The Detail Pen"]);
        assert_eq!(
            page.description,
            [
                Block::Text("Before".to_owned()),
                Block::Picture(
                    "https://assets.clip-studio.com/en-us/description/1/img/a.jpg".to_owned()
                ),
                Block::Text("After & more".to_owned()),
            ]
        );
        assert_eq!(
            page.contents,
            [(
                "https://assets.clip-studio.com/en-us/description/thumb/1/a/1".to_owned(),
                "Detail Pen".to_owned()
            )]
        );
    }

    #[test]
    fn the_browser_goes_back_and_forward() {
        let mut browser = Browser::default();
        browser.go(Some(Place::List("a".to_owned())));
        browser.go(Some(Place::Material("1".to_owned())));
        browser.back();
        assert_eq!(browser.place, Some(Place::List("a".to_owned())));
        browser.back();
        assert_eq!(browser.place, None, "Home");
        browser.forward();
        assert_eq!(browser.place, Some(Place::List("a".to_owned())));
    }

    #[test]
    fn a_page_shows_from_the_cache_when_the_site_cannot_be_reached() {
        let folder = std::env::temp_dir().join(format!("sue-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let mut assets = Assets::default();
        assets.use_folder(Some(folder.clone()));
        let url = "https://assets.clip-studio.invalid/en-us/";
        assets.keep(url, b"<title>Kept</title>");
        // Old enough to ask again; the site is not there.
        let path = folder.join(cache_name(url));
        let old = std::fs::File::options().write(true).open(&path).unwrap();
        old.set_modified(SystemTime::now() - Duration::from_secs(24 * 3600))
            .unwrap();
        drop(old);
        assert_eq!(
            assets.page(url),
            Some("<title>Kept</title>"),
            "shown at once"
        );
        for _ in 0..400 {
            if !assets.poll() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert_eq!(assets.page(url), Some("<title>Kept</title>"), "still shown");
        assert!(assets.offline(url), "marked as last time's copy");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
