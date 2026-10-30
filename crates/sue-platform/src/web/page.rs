//! The page around Sue's canvas: the browser's file pickers, downloads,
//! the clipboard, files dropped on the page, and the loading screen.
//!
//! Picks work as on phones and tablets: the picker answers later, and a
//! chosen file is staged in memory under [`PICKED`], where Sue reads it
//! by path. Whatever Sue writes there goes back to the person when it
//! calls `deliver`: to the file they chose in the browser's save dialog
//! (the File System Access API, where the browser has it) or else as a
//! download.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use js_sys::{Array, ArrayBuffer, Function, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    BlobPropertyBag, ClipboardEvent, DragEvent, File, FileList, HtmlAnchorElement,
    HtmlInputElement, KeyboardEvent, Url,
};

use crate::os::{PickNews, Picking};
use crate::time::SystemTime;
use crate::web::store;

/// Where picked, dropped and saved files are staged.
pub(crate) const PICKED: &str = "/sue/picked";

#[derive(Default)]
struct Page {
    /// Answers for the runtime's next poll.
    news: VecDeque<PickNews>,
    /// A picker is open.
    asking: bool,
    /// Staged files written since the last delivery.
    written: BTreeSet<PathBuf>,
    /// Files the save dialog chose, by their staged path.
    handles: HashMap<PathBuf, JsValue>,
    /// Files dropped on the page, staged.
    dropped: Vec<PathBuf>,
    /// Clipboard events for egui.
    events: Vec<egui::Event>,
    /// Draws a frame, so news is seen.
    waker: Option<Box<dyn Fn()>>,
    /// The last staging folder made.
    count: u32,
}

thread_local! {
    static PAGE: RefCell<Page> = RefCell::new(Page::default());
}

fn with<T>(work: impl FnOnce(&mut Page) -> T) -> T {
    PAGE.with(|page| work(&mut page.borrow_mut()))
}

fn wake() {
    // The waker draws a frame, which may itself reach the page: call it
    // with the page let go.
    let waker = with(|page| page.waker.take());
    if let Some(waker) = waker {
        waker();
        with(|page| {
            page.waker.get_or_insert(waker);
        });
    }
}

fn tell(news: PickNews) {
    with(|page| {
        page.asking = false;
        page.news.push_back(news);
    });
    wake();
}

/// A new, empty staging folder.
fn slot() -> PathBuf {
    let count = with(|page| {
        page.count += 1;
        page.count
    });
    Path::new(PICKED).join(count.to_string())
}

/// One file name, safe in a folder.
fn safe_name(name: &str) -> String {
    let name: String = name
        .chars()
        .map(|ch| {
            if matches!(ch, '/' | '\\') || ch.is_control() {
                '_'
            } else {
                ch
            }
        })
        .collect();
    match name.trim() {
        "" | "." | ".." => "Untitled".to_owned(),
        name => name.to_owned(),
    }
}

fn text(error: &JsValue) -> String {
    error
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| error.as_string())
        .unwrap_or_else(|| format!("{error:?}"))
}

fn document() -> Option<web_sys::Document> {
    web_sys::window()?.document()
}

/// Called for every file written: staged files go back at delivery.
pub(crate) fn written(path: &Path) {
    if path.starts_with(PICKED) {
        with(|page| page.written.insert(path.to_path_buf()));
    }
}

/// Puts `bytes` in staging without counting it as written by Sue.
fn stage(path: &Path, bytes: &[u8]) {
    store::with(|vfs| {
        let _ = vfs.write(path, bytes, SystemTime::now());
    });
}

async fn read_file(file: &File) -> Result<Vec<u8>, String> {
    let buffer: ArrayBuffer = JsFuture::from(file.array_buffer())
        .await
        .map_err(|error| text(&error))?
        .unchecked_into();
    Ok(Uint8Array::new(&buffer).to_vec())
}

fn files(list: Option<FileList>) -> Vec<File> {
    let Some(list) = list else {
        return Vec::new();
    };
    (0..list.length())
        .filter_map(|index| list.get(index))
        .collect()
}

/// The `accept` list of a file input: `.png,.jpg`.
fn accept(extensions: &[String]) -> String {
    extensions
        .iter()
        .map(|extension| format!(".{}", extension.trim_start_matches('.')))
        .collect::<Vec<_>>()
        .join(",")
}

/// Shows the browser's file picker for one file to open, or a folder.
pub(crate) fn pick_open(extensions: &[String], folder: bool) -> Picking {
    let Some(document) = document() else {
        return Picking::Failed("the page has no document".to_owned());
    };
    let Ok(input) = document
        .create_element("input")
        .map(JsCast::unchecked_into::<HtmlInputElement>)
    else {
        return Picking::Failed("the page cannot make a file picker".to_owned());
    };
    input.set_type("file");
    if folder {
        let _ = input.set_attribute("webkitdirectory", "");
    } else if !extensions.is_empty() && !touch_screen() {
        // Phone browsers grey out files of a type they do not know (a
        // .clip on iOS), so there the picker shows every file and what is
        // picked is checked when it opens.
        input.set_accept(&accept(extensions));
    }
    let chosen = input.clone();
    let on_change = Closure::once_into_js(move || {
        let picked = files(chosen.files());
        spawn_local(async move {
            let news = match open_files(picked, folder).await {
                Ok(Some(path)) => PickNews::Answered(Picking::Chosen(path)),
                Ok(None) => PickNews::Answered(Picking::Cancelled),
                Err(why) => PickNews::Answered(Picking::Failed(why)),
            };
            tell(news);
        });
    });
    input.set_onchange(Some(on_change.unchecked_ref()));
    let on_cancel = Closure::once_into_js(move || tell(PickNews::Answered(Picking::Cancelled)));
    let _ = input.add_event_listener_with_callback("cancel", on_cancel.unchecked_ref());
    with(|page| page.asking = true);
    input.click();
    Picking::Waiting
}

/// Stages picked files; the path of the file, or of the folder.
async fn open_files(picked: Vec<File>, folder: bool) -> Result<Option<PathBuf>, String> {
    let dir = slot();
    let mut first = None;
    for file in picked {
        let relative = if folder {
            Reflect::get(&file, &JsValue::from_str("webkitRelativePath"))
                .ok()
                .and_then(|path| path.as_string())
                .filter(|path| !path.is_empty())
                .unwrap_or_else(|| file.name())
        } else {
            file.name()
        };
        let mut path = dir.clone();
        for part in relative.split('/') {
            path.push(safe_name(part));
        }
        stage(&path, &read_file(&file).await?);
        let shown = if folder {
            relative
                .split('/')
                .next()
                .map_or_else(|| path.clone(), |top| dir.join(safe_name(top)))
        } else {
            path
        };
        first.get_or_insert(shown);
        if !folder {
            break;
        }
    }
    Ok(first)
}

/// Asks where to save `file_name`: the browser's save dialog where it
/// has one (it answers later), else a staged path whose file downloads
/// once written.
pub(crate) fn pick_save(file_name: &str, extensions: &[String]) -> Picking {
    let name = safe_name(file_name);
    let window = web_sys::window();
    let picker = window
        .as_ref()
        .and_then(|window| Reflect::get(window, &JsValue::from_str("showSaveFilePicker")).ok())
        .and_then(|picker| picker.dyn_into::<Function>().ok());
    let Some(picker) = picker else {
        return Picking::Chosen(slot().join(name));
    };
    let options = Object::new();
    let _ = Reflect::set(&options, &"suggestedName".into(), &name.clone().into());
    if !extensions.is_empty() {
        let kinds = Array::new();
        let kind = Object::new();
        let accepts = Object::new();
        let list: Array = extensions
            .iter()
            .map(|extension| JsValue::from_str(&format!(".{}", extension.trim_start_matches('.'))))
            .collect();
        let _ = Reflect::set(&accepts, &"application/octet-stream".into(), &list);
        let _ = Reflect::set(&kind, &"accept".into(), &accepts);
        kinds.push(&kind);
        let _ = Reflect::set(&options, &"types".into(), &kinds);
    }
    let promise = match picker
        .call1(
            window
                .as_ref()
                .map_or(&JsValue::NULL, |window| window.as_ref()),
            &options,
        )
        .and_then(JsCast::dyn_into::<js_sys::Promise>)
    {
        Ok(promise) => promise,
        // No dialog without a click to answer: download instead.
        Err(_) => return Picking::Chosen(slot().join(name)),
    };
    with(|page| page.asking = true);
    spawn_local(async move {
        let news = match JsFuture::from(promise).await {
            Ok(handle) => {
                let chosen = Reflect::get(&handle, &"name".into())
                    .ok()
                    .and_then(|name| name.as_string())
                    .map_or(name, |chosen| safe_name(&chosen));
                let path = slot().join(chosen);
                with(|page| page.handles.insert(path.clone(), handle));
                Picking::Chosen(path)
            }
            Err(error) if error_name(&error) == "AbortError" => Picking::Cancelled,
            // The dialog could not be shown: download instead.
            Err(_) => Picking::Chosen(slot().join(name)),
        };
        tell(PickNews::Answered(news));
    });
    Picking::Waiting
}

fn error_name(error: &JsValue) -> String {
    Reflect::get(error, &"name".into())
        .ok()
        .and_then(|name| name.as_string())
        .unwrap_or_default()
}

/// The answers of pickers since the last poll.
pub(crate) fn poll() -> Vec<PickNews> {
    with(|page| page.news.drain(..).collect())
}

/// `true` while a picker is open or its file is being read.
pub(crate) fn busy() -> bool {
    with(|page| page.asking || !page.news.is_empty())
}

/// Hands every staged file Sue wrote since the last call back to the
/// person: into the file the save dialog chose, or as a download.
pub(crate) fn deliver() {
    let written: Vec<(PathBuf, Option<JsValue>)> = with(|page| {
        let written = std::mem::take(&mut page.written);
        written
            .into_iter()
            .map(|path| {
                let handle = page.handles.get(&path).cloned();
                (path, handle)
            })
            .collect()
    });
    for (path, handle) in written {
        let Some(bytes) = store::with(|vfs| vfs.get(&path).map(|file| file.bytes.clone())) else {
            continue;
        };
        match handle {
            Some(handle) => spawn_local(async move {
                if let Err(error) = write_handle(&handle, &bytes).await {
                    with(|page| {
                        page.news.push_back(PickNews::Failed(format!(
                            "Cannot save {}: {}",
                            shown(&path),
                            text(&error)
                        )));
                    });
                    wake();
                }
            }),
            None => {
                if let Err(error) = download(&shown(&path), &bytes) {
                    with(|page| {
                        page.news.push_back(PickNews::Failed(format!(
                            "Cannot download {}: {}",
                            shown(&path),
                            text(&error)
                        )));
                    });
                }
            }
        }
    }
}

fn shown(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Calls the method `name` of `this` with `args`, awaiting the promise it
/// returns.
async fn call(this: &JsValue, name: &str, args: &Array) -> Result<JsValue, JsValue> {
    let method: Function = Reflect::get(this, &name.into())?.dyn_into()?;
    let promise: js_sys::Promise = method.apply(this, args)?.dyn_into()?;
    JsFuture::from(promise).await
}

async fn write_handle(handle: &JsValue, bytes: &Arc<[u8]>) -> Result<(), JsValue> {
    let writable = call(handle, "createWritable", &Array::new()).await?;
    let data = Uint8Array::from(&bytes[..]);
    call(&writable, "write", &Array::of1(&data)).await?;
    call(&writable, "close", &Array::new()).await?;
    Ok(())
}

/// Offers `bytes` to the browser as a download called `name`.
pub(crate) fn download(name: &str, bytes: &[u8]) -> Result<(), JsValue> {
    let document = document().ok_or("no document")?;
    let parts = Array::of1(&Uint8Array::from(bytes));
    let options = BlobPropertyBag::new();
    options.set_type("application/octet-stream");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)?;
    let url = Url::create_object_url_with_blob(&blob)?;
    let link: HtmlAnchorElement = document.create_element("a")?.unchecked_into();
    link.set_href(&url);
    link.set_download(name);
    link.click();
    // The browser has the blob once the click is handled.
    let revoke = Closure::once_into_js(move || {
        let _ = Url::revoke_object_url(&url);
    });
    if let Some(window) = web_sys::window() {
        let _ = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000);
    }
    Ok(())
}

/// Puts `text` on the browser's clipboard.
pub(crate) fn copy_text(text: &str) -> Result<(), String> {
    let window = web_sys::window().ok_or("the page has no window")?;
    // The promise fails only without permission; nothing waits for it.
    let _ = window.navigator().clipboard().write_text(text);
    Ok(())
}

/// Opens `url` in a new tab.
pub(crate) fn open_url(url: &str) -> std::io::Result<()> {
    web_sys::window()
        .and_then(|window| window.open_with_url_and_target(url, "_blank").ok())
        .flatten()
        .map(|_| ())
        .ok_or_else(|| std::io::Error::other("the browser blocked the new tab"))
}

/// Random bits from the page, for entity ids.
pub fn random_seed() -> u64 {
    let half = || (js_sys::Math::random() * f64::from(u32::MAX)) as u64;
    (half() << 32) ^ half() ^ (js_sys::Date::now() as u64)
}

/// The processor cores the browser reports (`navigator.hardwareConcurrency`),
/// at least one.
pub(crate) fn logical_cores() -> usize {
    web_sys::window()
        .map(|window| window.navigator().hardware_concurrency())
        .filter(|cores| cores.is_finite() && *cores >= 1.0)
        .map_or(1, |cores| cores as usize)
}

/// `true` when touch is how the page is used (a phone or tablet), not
/// only something the screen also takes.
pub(crate) fn touch_screen() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };
    let fine_pointer = window
        .match_media("(pointer: fine)")
        .ok()
        .flatten()
        .is_some_and(|query| query.matches());
    window.navigator().max_touch_points() > 0 && !fine_pointer
}

/// The browser's user agent, empty when it gives none.
pub(crate) fn user_agent() -> String {
    web_sys::window()
        .and_then(|window| window.navigator().user_agent().ok())
        .unwrap_or_default()
}

/// The shorter side of the device's screen in CSS pixels, when the
/// browser tells.
pub(crate) fn screen_side() -> Option<f32> {
    let screen = web_sys::window()?.screen().ok()?;
    let (width, height) = (screen.width().ok()?, screen.height().ok()?);
    let side = width.min(height);
    (side > 0).then_some(side as f32)
}

/// A file dropped on the page, staged in memory, as egui hands it on.
#[derive(Debug)]
pub(crate) struct Staged(PathBuf);

impl Staged {
    pub(crate) fn handle(path: PathBuf) -> egui::DroppedFileHandle {
        Arc::new(Self(path))
    }
}

impl egui::DroppedFile for Staged {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes_async(&self) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + '_>> {
        Box::pin(
            async move { store::with(|vfs| vfs.read(&self.0)).map_err(|error| error.to_string()) },
        )
    }
}

/// The clipboard events and dropped files since the last frame.
pub(crate) fn take_input() -> (Vec<egui::Event>, Vec<PathBuf>) {
    with(|page| {
        (
            std::mem::take(&mut page.events),
            std::mem::take(&mut page.dropped),
        )
    })
}

fn listen(
    target: &web_sys::EventTarget,
    kind: &str,
    capture: bool,
    work: impl FnMut(JsValue) + 'static,
) {
    let closure = Closure::<dyn FnMut(JsValue)>::new(work);
    let _ = target.add_event_listener_with_callback_and_bool(
        kind,
        closure.as_ref().unchecked_ref(),
        capture,
    );
    // The listeners live as long as the page.
    closure.forget();
}

/// Asks the browser for the clipboard's text (it may ask the person).
fn read_clipboard() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let promise = window.navigator().clipboard().read_text();
    spawn_local(async move {
        if let Ok(text) = JsFuture::from(promise).await
            && let Some(text) = text.as_string()
        {
            pasted(text);
        }
    });
}

fn pasted(text: String) {
    let text = text.replace("\r\n", "\n");
    if !text.is_empty() {
        with(|page| page.events.push(egui::Event::Paste(text)));
        wake();
    }
}

/// Wires the page to Sue's window: `waker` draws a frame, and
/// `let_through(true)` lets the browser see the next key.
///
/// Copy and cut come from their keys: egui hands the text back as
/// `CopyText`, which goes to the clipboard while the key still counts as
/// the person's own act. Pasting needs the browser's `paste` event, which
/// carries the clipboard without asking, but winit keeps every key from
/// the browser, so Ctrl or Cmd with V is let through; a browser that sends
/// no `paste` event to a canvas (Safari) is asked with `readText`.
pub(crate) fn connect(waker: impl Fn() + 'static, let_through: impl Fn(bool) + 'static) {
    with(|page| page.waker = Some(Box::new(waker)));
    let Some(window) = web_sys::window() else {
        return;
    };
    // A pen's pressure: winit hands a pen to Sue as a mouse, without it.
    for (kind, lifts) in [
        ("pointerdown", false),
        ("pointermove", false),
        ("pointerup", true),
        ("pointercancel", true),
    ] {
        listen(&window, kind, true, move |event| {
            let Some(pointer) = event.dyn_ref::<web_sys::PointerEvent>() else {
                return;
            };
            if pointer.pointer_type() != "pen" {
                return;
            }
            let touching = !lifts && pointer.buttons() & 1 != 0;
            crate::stylus::set(
                crate::stylus::PenSource::Browser,
                touching.then(|| pointer.pressure()),
            );
        });
    }
    let Some(document) = window.document() else {
        return;
    };
    let let_through = std::rc::Rc::new(let_through);
    let awaiting_paste = std::rc::Rc::new(std::cell::Cell::new(false));
    let timer = window.clone();
    let waiting = std::rc::Rc::clone(&awaiting_paste);
    listen(&window, "keydown", true, move |event| {
        let Some(key) = event.dyn_ref::<KeyboardEvent>() else {
            return;
        };
        if !(key.ctrl_key() || key.meta_key()) || key.alt_key() || key.shift_key() {
            return;
        }
        match key.key().to_ascii_lowercase().as_str() {
            "c" => with(|page| page.events.push(egui::Event::Copy)),
            "x" => with(|page| page.events.push(egui::Event::Cut)),
            "v" => {
                let_through(true);
                waiting.set(true);
                // The paste event comes straight after this key, if it
                // comes; then winit stops the browser's own handling again.
                let again = std::rc::Rc::clone(&let_through);
                let unanswered = std::rc::Rc::clone(&waiting);
                let reset = Closure::once_into_js(move || {
                    again(false);
                    if unanswered.replace(false) {
                        read_clipboard();
                    }
                });
                let _ = timer.set_timeout_with_callback_and_timeout_and_arguments_0(
                    reset.unchecked_ref(),
                    0,
                );
                return;
            }
            _ => return,
        }
        wake();
    });
    let waiting = awaiting_paste;
    listen(&document, "paste", false, move |event| {
        let Some(event) = event.dyn_ref::<ClipboardEvent>() else {
            return;
        };
        event.prevent_default();
        waiting.set(false);
        if let Some(text) = event
            .clipboard_data()
            .and_then(|data| data.get_data("text/plain").ok())
        {
            pasted(text);
        }
    });
    listen(&document, "dragover", false, |event| {
        if let Some(event) = event.dyn_ref::<DragEvent>() {
            event.prevent_default();
            if let Some(data) = event.data_transfer() {
                data.set_drop_effect("copy");
            }
        }
    });
    listen(&document, "drop", false, |event| {
        let Some(event) = event.dyn_ref::<DragEvent>() else {
            return;
        };
        event.prevent_default();
        let dropped = files(event.data_transfer().and_then(|data| data.files()));
        spawn_local(async move {
            for file in dropped {
                let path = slot().join(safe_name(&file.name()));
                match read_file(&file).await {
                    Ok(bytes) => {
                        stage(&path, &bytes);
                        with(|page| page.dropped.push(path));
                    }
                    Err(why) => with(|page| {
                        page.news.push_back(PickNews::Failed(format!(
                            "Cannot open {}: {why}",
                            file.name()
                        )));
                    }),
                }
            }
            wake();
        });
    });
}

/// Takes the loading screen away: Sue draws.
pub(crate) fn started() {
    if let Some(loading) = document().and_then(|document| document.get_element_by_id("loading")) {
        loading.remove();
    }
}

/// Says on the loading screen why Sue cannot start (or, once it has
/// started, why it stopped), and in the console.
pub fn failed(why: &str) {
    web_sys::console::error_1(&format!("sue: {why}").into());
    let Some(document) = document() else {
        return;
    };
    let loading = document.get_element_by_id("loading").or_else(|| {
        // Sue had started: bring the screen back for the message.
        let screen = document.create_element("div").ok()?;
        screen.set_id("loading");
        let message = document.create_element("p").ok()?;
        message.set_id("loading-message");
        let _ = screen.append_child(&message);
        let _ = document.body()?.append_child(&screen);
        Some(screen)
    });
    if let Some(message) = document.get_element_by_id("loading-message") {
        message.set_text_content(Some(&format!("Sue cannot go on here: {why}")));
    }
    if let Some(loading) = loading {
        let _ = loading.set_attribute("class", "failed");
    }
}

/// Shows a panic on the page as well as in the console: the page cannot
/// go on after one.
pub(crate) fn catch_panics() {
    std::panic::set_hook(Box::new(|info| failed(&info.to_string())));
}
