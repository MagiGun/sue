//! The colour of a pixel on the screen, for Edit, Obtain Screen Color.
//!
//! - [`pixel_at`] reads one pixel of the desktop as it is shown, at a
//!   point in screen pixels.
//! - [`Pick`] lets a person point at any place on any screen and click
//!   it. A clear window over every screen (one level of 255 opaque, kept
//!   out of screen captures where Windows allows it) takes the click, so
//!   it never reaches the program underneath; a right click or Esc
//!   cancels. With `hide` Sue's own windows are hidden while it waits and
//!   come back after.
//!
//! The picker runs on a thread of its own with its own message loop, and
//! the window procedure keeps its state in that thread's storage, so no
//! pointer is handed to Windows. GDI and window calls have no safe route
//! in the `windows` crate; each block below says why it is sound.

use std::cell::RefCell;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CLR_INVALID, CreateFontW, CreateSolidBrush,
    DEFAULT_CHARSET, DEFAULT_GUI_FONT, DT_LEFT, DT_NOPREFIX, DeleteObject, DrawTextW, EndPaint,
    FW_NORMAL, FillRect, GetDC, GetPixel, GetStockObject, HBRUSH, HDC, InvalidateRect,
    MONITOR_DEFAULTTONULL, MonitorFromPoint, OUT_DEFAULT_PRECIS, PAINTSTRUCT, ReleaseDC,
    SelectObject, SetBkMode, SetTextColor, TRANSPARENT, UpdateWindow,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    EnumWindows, GetClientRect, GetMessageW, GetSystemMetrics, GetWindowThreadProcessId,
    HWND_TOPMOST, IDC_CROSS, IsWindowVisible, LWA_ALPHA, LoadCursorW, MSG, PM_REMOVE, PeekMessageW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN,
    SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SW_SHOW, SW_SHOWNA,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetForegroundWindow,
    SetLayeredWindowAttributes, SetWindowDisplayAffinity, SetWindowPos, ShowWindow,
    TranslateMessage, UnregisterClassW, WDA_EXCLUDEFROMCAPTURE, WM_APP, WM_CLOSE, WM_DESTROY,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONDOWN, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::{BOOL, PCWSTR, w};

/// Why a screen colour could not be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScreenError {
    /// The point lies outside every screen.
    OffScreen,
    /// Windows would not give the pixel (no desktop to read, as in a
    /// locked session or a service).
    Capture,
    /// The picker's window could not be made.
    Window(String),
}

/// Where a [`Pick`] has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickState {
    /// Waiting for the click; where the pointer is and the colour under
    /// it, once it has moved over a screen.
    Waiting(Option<([i32; 2], [u8; 3])>),
    /// Clicked: the colour there.
    Picked([u8; 3]),
    /// Cancelled with a right click, Esc or [`Pick::cancel`].
    Cancelled,
    /// Something failed; nothing was picked.
    Failed(ScreenError),
}

/// The sRGB colour of the screen pixel at `x`, `y` (screen pixels, the
/// top left of the main screen being 0, 0).
///
/// # Errors
/// [`ScreenError::OffScreen`] for a point outside every screen,
/// [`ScreenError::Capture`] when Windows gives no pixel. Never panics.
#[allow(
    unsafe_code,
    reason = "reading a screen pixel is a GDI call with no safe route (standing approval, rule 13)"
)]
pub fn pixel_at(x: i32, y: i32) -> Result<[u8; 3], ScreenError> {
    // SAFETY: MonitorFromPoint takes a point by value and a flag, and
    // only answers a handle that is not owned (nothing to free).
    let monitor = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONULL) };
    if monitor.is_invalid() {
        return Err(ScreenError::OffScreen);
    }
    if FAIL_CAPTURE.with(std::cell::Cell::get) {
        return Err(ScreenError::Capture);
    }
    // SAFETY: GetDC(None) answers the whole screen's device context, or
    // a null one on failure; it is released below on every path.
    let dc = unsafe { GetDC(None) };
    if dc.is_invalid() {
        return Err(ScreenError::Capture);
    }
    // SAFETY: `dc` is the valid screen context just taken; GetPixel only
    // reads, and answers CLR_INVALID when it cannot.
    let color = unsafe { GetPixel(dc, x, y) };
    // SAFETY: gives back the context GetDC took above, exactly once.
    unsafe { ReleaseDC(None, dc) };
    if color.0 == CLR_INVALID {
        return Err(ScreenError::Capture);
    }
    Ok(rgb(color))
}

/// Acts, on this thread, as a desktop that gives no pixel (`true`) or as
/// it is, so tests can try the failure path.
#[doc(hidden)]
pub fn pretend_capture_fails(fail: bool) {
    FAIL_CAPTURE.with(|cell| cell.set(fail));
}

/// A COLORREF (0x00bbggrr) as red, green and blue.
fn rgb(color: COLORREF) -> [u8; 3] {
    let value = color.0;
    [
        (value & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        ((value >> 16) & 0xff) as u8,
    ]
}

/// What the picker thread and the people waiting on it share.
struct Shared {
    state: Mutex<PickState>,
    /// The picker's window once it is made (0 before and after).
    window: AtomicIsize,
    /// Set by [`Pick::cancel`], also before the window is up.
    ended: std::sync::atomic::AtomicBool,
    /// Set once a test drives the pick ([`Pick::click_at`],
    /// [`Pick::move_to`], [`click_latest`]): from then on the real
    /// pointer's moves and clicks are left alone, so a person using the
    /// mouse meanwhile never changes what the test reads.
    scripted: std::sync::atomic::AtomicBool,
    /// The first line of the label by the pointer (the command's name).
    title: String,
    /// Called after each change of state, so the interface draws again.
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Shared {
    fn set(&self, state: PickState) {
        if let Ok(mut held) = self.state.lock() {
            // A final answer stays: a late move never overwrites it.
            if matches!(*held, PickState::Waiting(_)) {
                *held = state;
            }
        }
        (self.wake)();
    }
}

/// A screen colour pick under way. Dropping it cancels it.
pub struct Pick {
    shared: Arc<Shared>,
}

impl Pick {
    /// Starts a pick: a clear window over every screen waits for a click.
    /// With `hide`, Sue's own windows are hidden first and shown again
    /// when the pick ends. A small label by the pointer reads `title`
    /// and the colour under it as "R=245 G=214 B=201". `wake` is called
    /// (on the picker's thread) whenever [`Pick::state`] changes.
    #[must_use]
    pub fn start(hide: bool, title: String, wake: Box<dyn Fn() + Send + Sync>) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(PickState::Waiting(None)),
            window: AtomicIsize::new(0),
            ended: std::sync::atomic::AtomicBool::new(false),
            scripted: std::sync::atomic::AtomicBool::new(false),
            title,
            wake,
        });
        if let Ok(mut latest) = LATEST.lock() {
            *latest = Some(Arc::downgrade(&shared));
        }
        let thread = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("sue-screen-pick".to_owned())
            .spawn(move || run_pick(&thread, hide));
        if let Err(error) = spawned {
            shared.set(PickState::Failed(ScreenError::Window(error.to_string())));
        }
        Self { shared }
    }

    /// Where the pick has got to.
    #[must_use]
    pub fn state(&self) -> PickState {
        self.shared
            .state
            .lock()
            .map_or(PickState::Cancelled, |state| state.clone())
    }

    /// Ends the pick without a colour (nothing happens once it ended).
    pub fn cancel(&self) {
        self.shared.ended.store(true, Ordering::Release);
        self.post(WM_CLOSE, 0, 0);
    }

    /// Acts as a click at screen point `x`, `y`, for tests: the picker
    /// reads that point as it reads a real click.
    #[doc(hidden)]
    pub fn click_at(&self, x: i32, y: i32) {
        self.shared.scripted.store(true, Ordering::Release);
        self.post(SCRIPTED_CLICK, 0, client_point(x, y));
    }

    /// Acts as the pointer moved to screen point `x`, `y`, for tests.
    #[doc(hidden)]
    pub fn move_to(&self, x: i32, y: i32) {
        self.shared.scripted.store(true, Ordering::Release);
        self.post(SCRIPTED_MOVE, 0, client_point(x, y));
    }

    /// Acts as Esc pressed over the picker, for tests.
    #[doc(hidden)]
    pub fn press_escape(&self) {
        self.post(WM_KEYDOWN, usize::from(VK_ESCAPE.0), 0);
    }

    /// Whether the picker's window is up (for tests that wait for it).
    #[doc(hidden)]
    #[must_use]
    pub fn window_up(&self) -> bool {
        self.shared.window.load(Ordering::Acquire) != 0
    }

    fn post(&self, message: u32, wparam: usize, lparam: isize) {
        post(&self.shared, message, wparam, lparam);
    }
}

/// The latest pick started in this process, for tests that drive Sue's
/// own command and so hold no [`Pick`] of their own.
static LATEST: Mutex<Option<std::sync::Weak<Shared>>> = Mutex::new(None);

/// Acts as a click at screen point `x`, `y` on the latest pick still
/// waiting, for tests; `false` when there is none or its window is not
/// up yet.
#[doc(hidden)]
#[must_use]
pub fn click_latest(x: i32, y: i32) -> bool {
    let Some(shared) = LATEST
        .lock()
        .ok()
        .and_then(|latest| latest.as_ref().and_then(std::sync::Weak::upgrade))
    else {
        return false;
    };
    if shared.window.load(Ordering::Acquire) == 0 {
        return false;
    }
    shared.scripted.store(true, Ordering::Release);
    post(&shared, SCRIPTED_CLICK, 0, client_point(x, y));
    true
}

/// Whether the latest pick started in this process is still waiting for
/// its click, for tests that drive Sue's own command; `false` once it
/// ended (or when none was started).
#[doc(hidden)]
#[must_use]
pub fn latest_waiting() -> bool {
    LATEST
        .lock()
        .ok()
        .and_then(|latest| latest.as_ref().and_then(std::sync::Weak::upgrade))
        .and_then(|shared| shared.state.lock().ok().map(|state| state.clone()))
        .is_some_and(|state| matches!(state, PickState::Waiting(_)))
}

/// A test's pointer move and click, posted in place of the pointer's own
/// messages (which a driven pick leaves alone).
const SCRIPTED_MOVE: u32 = WM_APP + 1;
const SCRIPTED_CLICK: u32 = WM_APP + 2;

/// Screen point `x`, `y` as the picker window's packed client point.
fn client_point(x: i32, y: i32) -> isize {
    let [left, top, ..] = virtual_screen();
    let (cx, cy) = (x - left, y - top);
    (((cy & 0xffff) << 16) | (cx & 0xffff)) as isize
}

#[allow(
    unsafe_code,
    reason = "posting to the picker's window is a window call with no safe route (standing approval, rule 13)"
)]
fn post(shared: &Shared, message: u32, wparam: usize, lparam: isize) {
    let window = shared.window.load(Ordering::Acquire);
    if window == 0 {
        if message == WM_CLOSE {
            shared.set(PickState::Cancelled);
        }
        return;
    }
    // SAFETY: PostMessageW only queues a message; a window that was
    // destroyed meanwhile makes it fail, which is ignored. No pointer
    // travels in the message.
    let _ = unsafe {
        PostMessageW(
            Some(HWND(window as *mut _)),
            message,
            WPARAM(wparam),
            LPARAM(lparam),
        )
    };
}

impl Drop for Pick {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The virtual screen (every screen together): left, top, width, height.
#[allow(
    unsafe_code,
    reason = "GetSystemMetrics has no safe route in the windows crate (standing approval, rule 13)"
)]
fn virtual_screen() -> [i32; 4] {
    // SAFETY: GetSystemMetrics takes a number and answers a number.
    unsafe {
        [
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        ]
    }
}

thread_local! {
    /// Acts as a desktop that gives no pixel, for tests.
    static FAIL_CAPTURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// The pick this thread's window serves (the window procedure reads
    /// it; one picker window per thread).
    static SERVING: RefCell<Option<Arc<Shared>>> = const { RefCell::new(None) };
    /// The label by the pointer (0 while there is none).
    static LABEL: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    /// The label's two lines and its swatch.
    static LABEL_TEXT: RefCell<(Vec<u16>, Vec<u16>, COLORREF)> =
        const { RefCell::new((Vec::new(), Vec::new(), COLORREF(0))) };
    /// The brush a patch's window is filled with, on its own thread.
    static PATCH_BRUSH: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
    /// The windows of this process found while hiding them.
    static FOUND: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
}

/// The picker thread: hide, make the window, wait, show again.
#[allow(
    unsafe_code,
    reason = "making a window and running its message loop has no safe route (standing approval, rule 13)"
)]
fn run_pick(shared: &Arc<Shared>, hide: bool) {
    let hidden = if hide { hide_own_windows() } else { Vec::new() };
    if hide {
        // Let the screen show what was under them before the click.
        std::thread::sleep(Duration::from_millis(250));
    }
    SERVING.with(|serving| *serving.borrow_mut() = Some(Arc::clone(shared)));
    if let Err(error) = make_window(shared) {
        shared.set(PickState::Failed(error));
    } else if shared.ended.load(Ordering::Acquire) {
        // Cancelled while the window was being made.
        let window = shared.window.load(Ordering::Acquire);
        // SAFETY: the window this thread just made; destroying it on its
        // own thread before its loop runs is allowed.
        let _ = unsafe { DestroyWindow(HWND(window as *mut _)) };
    } else {
        let mut message = MSG::default();
        // SAFETY: `message` is a valid MSG this loop owns; GetMessageW
        // answers 0 on WM_QUIT and -1 on failure, both ending the loop.
        while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {
            // SAFETY: the MSG GetMessageW just filled, on its own thread.
            unsafe {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
        }
    }
    shared.window.store(0, Ordering::Release);
    SERVING.with(|serving| *serving.borrow_mut() = None);
    // SAFETY: the classes were registered by this module (or
    // registering failed, and then this fails harmlessly); their windows
    // are gone.
    let _ = unsafe { UnregisterClassW(CLASS, None) };
    // SAFETY: as above.
    let _ = unsafe { UnregisterClassW(LABEL_CLASS, None) };
    show_again(&hidden);
    // A window that went before answering counts as cancelled.
    shared.set(PickState::Cancelled);
}

const CLASS: PCWSTR = w!("SueScreenPick");
const LABEL_CLASS: PCWSTR = w!("SueScreenPickLabel");
/// The label's size at 96 dots per inch (100 %); [`at_dpi`] scales it
/// and every place inside it to the screen it is on.
const LABEL_SIZE: [i32; 2] = [196, 44];

/// `value`, given at 96 dots per inch, at `dpi`.
fn at_dpi(value: i32, dpi: u32) -> i32 {
    let dpi = if dpi == 0 { 96 } else { dpi };
    (value * dpi.min(960) as i32 + 48) / 96
}

/// The label's size at `dpi`.
fn label_size(dpi: u32) -> [i32; 2] {
    LABEL_SIZE.map(|side| at_dpi(side, dpi))
}

/// Makes the clear window over every screen and brings it to the front.
#[allow(
    unsafe_code,
    reason = "registering a class and making a window has no safe route (standing approval, rule 13)"
)]
fn make_window(shared: &Shared) -> Result<(), ScreenError> {
    let error = |what: &'static str| {
        move |error: windows::core::Error| ScreenError::Window(format!("{what}: {error}"))
    };
    // SAFETY: None asks for this program's own module, which lives as
    // long as the process; nothing is freed.
    let module = unsafe { GetModuleHandleW(None) }.map_err(error("module"))?;
    // SAFETY: IDC_CROSS is a system cursor id; system cursors are shared
    // and never freed.
    let cursor = unsafe { LoadCursorW(None, IDC_CROSS) }.map_err(error("cursor"))?;
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(procedure),
        hInstance: module.into(),
        hCursor: cursor,
        lpszClassName: CLASS,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is a filled WNDCLASSEXW that lives across the call;
    // a 0 answer (already registered by another pick) is fine, since the
    // class is the same.
    let _ = unsafe { RegisterClassExW(&raw const class) };
    let [left, top, width, height] = virtual_screen();
    // SAFETY: the class was registered above with `procedure`; the names
    // are static wide strings; no parent, menu or creation data.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            CLASS,
            w!("Sue"),
            WS_POPUP,
            left,
            top,
            width,
            height,
            None,
            None,
            Some(module.into()),
            None,
        )
    }
    .map_err(error("window"))?;
    shared.window.store(window.0 as isize, Ordering::Release);
    // SAFETY: `window` was just made on this thread. One level of 255:
    // clear to the eye, still hit by the mouse. Keeping it out of screen
    // captures needs Windows 10 2004; where it fails, the one level is
    // all it adds to a read pixel, and a click reads with it hidden.
    unsafe {
        SetLayeredWindowAttributes(window, COLORREF(0), 1, LWA_ALPHA).map_err(error("clear"))?;
        let _ = SetWindowDisplayAffinity(window, WDA_EXCLUDEFROMCAPTURE);
        let _ = ShowWindow(window, SW_SHOW);
        let _ = SetForegroundWindow(window);
    }
    // The label by the pointer: a small window of its own that never
    // takes the click or the focus. Without it the pick still works.
    let label_class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(label_procedure),
        hInstance: module.into(),
        hCursor: cursor,
        lpszClassName: LABEL_CLASS,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: as for the picker's class above.
    let _ = unsafe { RegisterClassExW(&raw const label_class) };
    LABEL_TEXT.with(|text| {
        text.borrow_mut().0 = shared.title.encode_utf16().collect();
    });
    // SAFETY: the label class was registered above; static names; owned
    // by the picker's window, so it goes with it. It starts hidden.
    let label = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            LABEL_CLASS,
            w!("Sue"),
            WS_POPUP,
            left,
            top,
            LABEL_SIZE[0],
            LABEL_SIZE[1],
            Some(window),
            None,
            Some(module.into()),
            None,
        )
    };
    if let Ok(label) = label {
        // SAFETY: the label window just made on this thread.
        let _ = unsafe { SetWindowDisplayAffinity(label, WDA_EXCLUDEFROMCAPTURE) };
        LABEL.with(|cell| cell.set(label.0 as isize));
    }
    Ok(())
}

/// Moves the label next to the pointer at `x`, `y` and shows `color`.
#[allow(
    unsafe_code,
    reason = "moving and redrawing a window has no safe route (standing approval, rule 13)"
)]
fn follow(x: i32, y: i32, color: [u8; 3]) {
    let label = LABEL.with(std::cell::Cell::get);
    if label == 0 {
        return;
    }
    LABEL_TEXT.with(|text| {
        let mut text = text.borrow_mut();
        text.1 = format!("R={} G={} B={}", color[0], color[1], color[2])
            .encode_utf16()
            .collect();
        text.2 = COLORREF(
            u32::from(color[0]) | (u32::from(color[1]) << 8) | (u32::from(color[2]) << 16),
        );
    });
    let [left, top, width, height] = virtual_screen();
    let label = HWND(label as *mut _);
    // SAFETY: reads a fact of this thread's live label window: the dots
    // per inch of the screen it is on (0 on failure, read as 96).
    let dpi = unsafe { GetDpiForWindow(label) };
    let size = label_size(dpi);
    let (gap_x, gap_y) = (at_dpi(18, dpi), at_dpi(22, dpi));
    // Below right of the pointer, kept on the screen.
    let mut at = [x + gap_x, y + gap_y];
    if at[0] + size[0] > left + width {
        at[0] = x - gap_x - size[0];
    }
    if at[1] + size[1] > top + height {
        at[1] = y - gap_y - size[1];
    }
    // SAFETY: the label window of this thread, alive until the pick
    // ends; the calls only move it and ask it to draw again.
    unsafe {
        let _ = SetWindowPos(
            label,
            Some(HWND_TOPMOST),
            at[0],
            at[1],
            size[0],
            size[1],
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = InvalidateRect(Some(label), None, false);
    }
}

/// The label's procedure: a dark plate, a swatch and two lines.
#[allow(
    unsafe_code,
    reason = "painting a window is GDI and window calls (standing approval, rule 13)"
)]
unsafe extern "system" fn label_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message != WM_PAINT {
        // SAFETY: Windows' own handling, with the arguments it passed.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let (mut title, mut values, swatch) = LABEL_TEXT.with(|text| text.borrow().clone());
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: reads a fact of this live window (0 on failure, read as 96).
    let dpi = unsafe { GetDpiForWindow(window) };
    let s = |value: i32| at_dpi(value, dpi);
    let size = label_size(dpi);
    let area = |left: i32, top: i32, right: i32, bottom: i32| RECT {
        left,
        top,
        right,
        bottom,
    };
    // SAFETY: BeginPaint and EndPaint pair on this window in this
    // WM_PAINT; every brush and the font made here are deleted after
    // use, and the font that was in the context is selected back first.
    unsafe {
        let dc = BeginPaint(window, &raw mut paint);
        let boxes = [
            (COLORREF(0x0020_2020), area(0, 0, size[0], size[1])),
            (COLORREF(0x00f0_f0f0), area(s(6), s(6), s(38), s(38))),
            (swatch, area(s(7), s(7), s(37), s(37))),
        ];
        for (color, rect) in boxes {
            let brush = CreateSolidBrush(color);
            FillRect(dc, &raw const rect, brush);
            let _ = DeleteObject(brush.into());
        }
        // Windows' own interface font, as tall at this screen's scale as
        // at 100 % (the stock font when it cannot be made).
        let font = CreateFontW(
            -s(13),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            0,
            w!("Segoe UI"),
        );
        let old = if font.is_invalid() {
            SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT))
        } else {
            SelectObject(dc, font.into())
        };
        SetBkMode(dc, TRANSPARENT);
        SetTextColor(dc, COLORREF(0x00f0_f0f0));
        let mut first = area(s(46), s(5), size[0] - s(4), s(22));
        DrawTextW(dc, &mut title, &raw mut first, DT_LEFT | DT_NOPREFIX);
        let mut second = area(s(46), s(22), size[0] - s(4), s(40));
        DrawTextW(dc, &mut values, &raw mut second, DT_LEFT | DT_NOPREFIX);
        SelectObject(dc, old);
        if !font.is_invalid() {
            let _ = DeleteObject(font.into());
        }
        let _ = EndPaint(window, &raw const paint);
    }
    LRESULT(0)
}

/// The picker window's procedure, on the picker thread.
#[allow(
    unsafe_code,
    reason = "a window procedure is called by Windows and answers through window calls (standing approval, rule 13)"
)]
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let serving = SERVING.with(|serving| serving.borrow().clone());
    let point = || {
        let [left, top, ..] = virtual_screen();
        let x = i32::from((lparam.0 & 0xffff) as u16 as i16);
        let y = i32::from(((lparam.0 >> 16) & 0xffff) as u16 as i16);
        [x + left, y + top]
    };
    let close = || {
        // SAFETY: `window` is this procedure's own live window, destroyed
        // on its own thread; WM_DESTROY then ends the loop.
        let _ = unsafe { DestroyWindow(window) };
    };
    // Once a test drives the pick, the real pointer is left alone.
    let real = |shared: &Shared| !shared.scripted.load(Ordering::Acquire);
    match (message, serving) {
        (WM_MOUSEMOVE | WM_LBUTTONDOWN | WM_RBUTTONDOWN, Some(shared)) if !real(&shared) => {
            LRESULT(0)
        }
        (WM_MOUSEMOVE | SCRIPTED_MOVE, Some(shared)) => {
            let [x, y] = point();
            if let Ok(color) = pixel_at(x, y) {
                follow(x, y, color);
                shared.set(PickState::Waiting(Some(([x, y], color))));
            }
            LRESULT(0)
        }
        (WM_LBUTTONDOWN | SCRIPTED_CLICK, Some(shared)) => {
            let [x, y] = point();
            // Read with the picker out of the way, so the colour is the
            // screen's own to the last level.
            // SAFETY: hides this thread's own live windows (the label is
            // 0, so left alone, when it could not be made).
            unsafe {
                let _ = ShowWindow(window, SW_HIDE);
                let label = LABEL.with(std::cell::Cell::get);
                if label != 0 {
                    let _ = ShowWindow(HWND(label as *mut _), SW_HIDE);
                }
            }
            std::thread::sleep(Duration::from_millis(60));
            shared.set(match pixel_at(x, y) {
                Ok(color) => PickState::Picked(color),
                Err(error) => PickState::Failed(error),
            });
            close();
            LRESULT(0)
        }
        (WM_RBUTTONDOWN | WM_CLOSE, Some(shared)) => {
            shared.set(PickState::Cancelled);
            close();
            LRESULT(0)
        }
        (WM_KEYDOWN, Some(shared)) if wparam.0 == usize::from(VK_ESCAPE.0) => {
            shared.set(PickState::Cancelled);
            close();
            LRESULT(0)
        }
        (WM_DESTROY, _) => {
            // The label is owned by the picker and goes with it.
            LABEL.with(|cell| cell.set(0));
            // SAFETY: ends this thread's message loop; takes a number.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: every other message gets Windows' own handling, with
        // the arguments Windows passed in.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Hides every shown top level window of this process; answers them.
#[allow(
    unsafe_code,
    reason = "listing and hiding windows has no safe route (standing approval, rule 13)"
)]
fn hide_own_windows() -> Vec<isize> {
    FOUND.with(|found| found.borrow_mut().clear());
    // SAFETY: `found` is a plain callback that only reads its window and
    // writes this thread's own storage; no pointer goes in the LPARAM.
    let _ = unsafe { EnumWindows(Some(found), LPARAM(0)) };
    let windows = FOUND.with(|found| std::mem::take(&mut *found.borrow_mut()));
    for window in &windows {
        // SAFETY: a window handle EnumWindows just gave; hiding a window
        // that went meanwhile fails harmlessly.
        let _ = unsafe { ShowWindow(HWND(*window as *mut _), SW_HIDE) };
    }
    windows
}

/// EnumWindows' callback: keeps this process's shown windows.
#[allow(
    unsafe_code,
    reason = "an EnumWindows callback reads window facts through window calls (standing approval, rule 13)"
)]
unsafe extern "system" fn found(window: HWND, _: LPARAM) -> BOOL {
    let mut process = 0_u32;
    // SAFETY: `window` comes from EnumWindows; `process` is a live u32.
    unsafe { GetWindowThreadProcessId(window, Some(&raw mut process)) };
    // SAFETY: reads a fact of a window EnumWindows gave.
    let shown = unsafe { IsWindowVisible(window) }.as_bool();
    if process == std::process::id() && shown {
        FOUND.with(|found| found.borrow_mut().push(window.0 as isize));
    }
    BOOL(1)
}

/// Shows the windows [`hide_own_windows`] hid, the first in front.
#[allow(
    unsafe_code,
    reason = "showing windows again has no safe route (standing approval, rule 13)"
)]
fn show_again(windows: &[isize]) {
    for (index, window) in windows.iter().enumerate() {
        let window = HWND(*window as *mut _);
        // SAFETY: handles this module hid; one that went meanwhile makes
        // the calls fail harmlessly.
        unsafe {
            let _ = ShowWindow(window, if index == 0 { SW_SHOW } else { SW_SHOWNA });
            if index == 0 {
                let _ = SetForegroundWindow(window);
            }
        }
    }
}

/// A solid square of one known colour that Sue shows in a window of its
/// own, in the middle of the main screen and above other windows, for
/// tests that read the screen: what they read is then what Sue drew,
/// never what another program shows. Dropping it takes the window away.
#[doc(hidden)]
pub struct Patch {
    color: [u8; 3],
    /// The square on the screen: left, top, side.
    area: [i32; 3],
    done: Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    window: isize,
}

impl Patch {
    /// Shows a square of `color`, `side` screen pixels wide, and waits
    /// until Windows has made the window.
    ///
    /// # Errors
    /// [`ScreenError::Window`] when the window cannot be made.
    pub fn show(color: [u8; 3], side: i32) -> Result<Self, ScreenError> {
        let area = patch_area(main_screen(), side);
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (ready, wait) = std::sync::mpsc::channel();
        let ended = Arc::clone(&done);
        let thread = std::thread::Builder::new()
            .name("sue-screen-patch".to_owned())
            .spawn(move || patch_thread(area, color, &ended, &ready))
            .map_err(|error| ScreenError::Window(error.to_string()))?;
        let window = match wait.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(window)) => window,
            Ok(Err(error)) => {
                let _ = thread.join();
                return Err(error);
            }
            Err(_) => {
                done.store(true, Ordering::Release);
                return Err(ScreenError::Window("the patch did not come up".to_owned()));
            }
        };
        Ok(Self {
            color,
            area,
            done,
            thread: Some(thread),
            window,
        })
    }

    /// The colour of the square.
    #[must_use]
    pub fn color(&self) -> [u8; 3] {
        self.color
    }

    /// The square on the screen: left, top and its side.
    #[must_use]
    pub fn area(&self) -> [i32; 3] {
        self.area
    }

    /// The window's handle, for tests that ask Windows about it.
    #[must_use]
    pub fn window(&self) -> isize {
        self.window
    }

    /// A point inside the square where the screen shows its colour: the
    /// middle first, then a grid nearer the edges (another program's
    /// window may lie over a part of it). Waits up to `wait` for the
    /// square to be drawn; `None` when no point shows the colour.
    #[must_use]
    pub fn spot(&self, wait: Duration) -> Option<[i32; 2]> {
        let [left, top, side] = self.area;
        let step = (side / 8).max(1);
        let mut points = vec![[left + side / 2, top + side / 2]];
        for row in 1..8 {
            for column in 1..8 {
                points.push([left + column * step, top + row * step]);
            }
        }
        let start = std::time::Instant::now();
        loop {
            if let Some(point) = points
                .iter()
                .find(|[x, y]| pixel_at(*x, *y) == Ok(self.color))
            {
                return Some(*point);
            }
            if start.elapsed() > wait {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Patch {
    fn drop(&mut self) {
        self.done.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Where a patch `side` pixels wide lies on a main screen `screen` big:
/// left, top and its side, kept between 16 pixels and what the screen
/// holds, in the screen's middle.
fn patch_area(screen: [i32; 2], side: i32) -> [i32; 3] {
    let [width, height] = screen;
    let most = width.min(height).clamp(16, 2048);
    let side = side.clamp(16, most);
    [(width - side) / 2, (height - side) / 2, side]
}

/// The main screen's size in pixels.
#[allow(
    unsafe_code,
    reason = "GetSystemMetrics has no safe route in the windows crate (standing approval, rule 13)"
)]
fn main_screen() -> [i32; 2] {
    // SAFETY: GetSystemMetrics takes a number and answers a number.
    unsafe { [GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)] }
}

/// The patch's own procedure: fills the window with its thread's brush
/// ([`PATCH_BRUSH`]), else Windows' own handling.
#[allow(
    unsafe_code,
    reason = "a window procedure paints and hands its messages on through window calls (standing approval, rule 13)"
)]
unsafe extern "system" fn patch_procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let brush = PATCH_BRUSH.with(std::cell::Cell::get);
    if message == WM_ERASEBKGND && brush != 0 {
        let mut rect = RECT::default();
        // SAFETY: `window` is this procedure's live window; `wparam` is
        // the device context Windows hands WM_ERASEBKGND; the brush is
        // this thread's, alive until its window is gone.
        unsafe {
            let _ = GetClientRect(window, &raw mut rect);
            FillRect(
                HDC(wparam.0 as *mut _),
                &raw const rect,
                HBRUSH(brush as *mut _),
            );
        }
        return LRESULT(1);
    }
    // SAFETY: the arguments Windows passed in, handed on unchanged.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

const PATCH_CLASS: PCWSTR = w!("SueScreenPatch");

/// The patch's thread: makes the window, says so on `ready`, pumps its
/// messages until `done`, then takes it away.
#[allow(
    unsafe_code,
    reason = "making, pumping and destroying a window has no safe route (standing approval, rule 13)"
)]
fn patch_thread(
    area: [i32; 3],
    color: [u8; 3],
    done: &std::sync::atomic::AtomicBool,
    ready: &std::sync::mpsc::Sender<Result<isize, ScreenError>>,
) {
    let [left, top, side] = area;
    let reference =
        COLORREF(u32::from(color[0]) | (u32::from(color[1]) << 8) | (u32::from(color[2]) << 16));
    // SAFETY: the brush is this thread's (each patch has its own colour,
    // so the class shared by every patch holds none); it is deleted at
    // the end, after the window is gone.
    let brush = unsafe { CreateSolidBrush(reference) };
    PATCH_BRUSH.with(|cell| cell.set(brush.0 as isize));
    // SAFETY: None asks for this program's own module, which lives as
    // long as the process.
    let module = match unsafe { GetModuleHandleW(None) } {
        Ok(module) => module,
        Err(error) => {
            let _ = ready.send(Err(ScreenError::Window(error.to_string())));
            // SAFETY: the brush made above, used by nothing.
            let _ = unsafe { DeleteObject(brush.into()) };
            return;
        }
    };
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(patch_procedure),
        hInstance: module.into(),
        lpszClassName: PATCH_CLASS,
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is a filled WNDCLASSEXW that lives across the
    // call; a 0 answer (registered by an earlier patch) is fine.
    let _ = unsafe { RegisterClassExW(&raw const class) };
    // SAFETY: the class was registered above; static names; no parent,
    // menu or creation data. It never takes the focus from a person.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            PATCH_CLASS,
            w!("Sue screen patch"),
            WS_POPUP,
            left,
            top,
            side,
            side,
            None,
            None,
            Some(module.into()),
            None,
        )
    };
    match window {
        Ok(window) => {
            // SAFETY: the window just made on this thread.
            unsafe {
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
                let _ = UpdateWindow(window);
            }
            let _ = ready.send(Ok(window.0 as isize));
            let mut message = MSG::default();
            while !done.load(Ordering::Acquire) {
                // SAFETY: `message` is a MSG this loop owns, filled and
                // handed on on its own thread.
                unsafe {
                    while PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE).as_bool() {
                        let _ = TranslateMessage(&raw const message);
                        DispatchMessageW(&raw const message);
                    }
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            // SAFETY: the window this thread made, destroyed on it.
            let _ = unsafe { DestroyWindow(window) };
        }
        Err(error) => {
            let _ = ready.send(Err(ScreenError::Window(error.to_string())));
        }
    }
    PATCH_BRUSH.with(|cell| cell.set(0));
    // SAFETY: the class this module registered, its window gone (another
    // patch's window still up makes this fail harmlessly); the brush is
    // no longer used by a window of this thread.
    unsafe {
        let _ = UnregisterClassW(PATCH_CLASS, Some(module.into()));
        let _ = DeleteObject(brush.into());
    }
}

#[cfg(test)]
mod tests {
    //! These read the real screen of this machine's desktop session, but
    //! only where Sue's own patch lies, and drive the picker with
    //! scripted messages, never the real pointer.
    use super::*;
    use std::time::Instant;

    fn wait_for(pick: &Pick, done: impl Fn(&PickState) -> bool) -> PickState {
        let start = Instant::now();
        loop {
            let state = pick.state();
            if done(&state) || start.elapsed() > Duration::from_secs(10) {
                return state;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn wait_up(pick: &Pick) {
        let start = Instant::now();
        while !pick.window_up() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(pick.window_up(), "the picker's window came up");
    }

    // One test, so the patch and the picker never race another test's
    // windows on the same screen.
    #[test]
    #[allow(
        unsafe_code,
        reason = "the test reads whether its own window is shown through a window call (standing approval, rule 13)"
    )]
    fn reads_the_known_color_of_a_window_sue_painted_and_picks_it() {
        let color = [12, 200, 99];
        let patch = Patch::show(color, 240).expect("Sue's patch window");
        assert_eq!(patch.color(), color);
        let at = patch
            .spot(Duration::from_secs(5))
            .expect("a point of the patch shows its colour");
        let [left, top, side] = patch.area();
        assert!((left..left + side).contains(&at[0]) && (top..top + side).contains(&at[1]));
        assert_eq!(pixel_at(at[0], at[1]), Ok(color), "read directly");

        // A pick, clicked over the patch: the click is the picker's, the
        // colour the patch's to the last level. Moves of the real pointer
        // meanwhile change nothing.
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = Arc::clone(&woken);
        let pick = Pick::start(
            false,
            "Pick Screen Color".to_owned(),
            Box::new(move || {
                count.fetch_add(1, Ordering::Relaxed);
            }),
        );
        wait_up(&pick);
        // Moving over it shows its colour while the pick waits.
        pick.move_to(at[0], at[1]);
        let state = wait_for(
            &pick,
            |state| matches!(state, PickState::Waiting(Some((point, _))) if *point == at),
        );
        assert_eq!(state, PickState::Waiting(Some((at, color))), "hover");
        pick.click_at(at[0], at[1]);
        let state = wait_for(&pick, |state| !matches!(state, PickState::Waiting(_)));
        assert_eq!(state, PickState::Picked(color), "picked");
        assert!(woken.load(Ordering::Relaxed) > 0, "the interface was woken");

        // A point of another colour picks that colour, not the patch's:
        // the reader can tell them apart.
        let other = [230, 40, 160];
        let second = Patch::show(other, 120).expect("a second patch");
        let inner = second
            .spot(Duration::from_secs(5))
            .expect("the second patch shows");
        let pick = Pick::start(false, String::new(), Box::new(|| {}));
        wait_up(&pick);
        pick.click_at(inner[0], inner[1]);
        let state = wait_for(&pick, |state| !matches!(state, PickState::Waiting(_)));
        assert_eq!(state, PickState::Picked(other), "the second patch");
        drop(second);

        // Esc, and a cancel from Sue, end a pick without a colour.
        let pick = Pick::start(false, String::new(), Box::new(|| {}));
        wait_up(&pick);
        pick.press_escape();
        let state = wait_for(&pick, |state| !matches!(state, PickState::Waiting(_)));
        assert_eq!(state, PickState::Cancelled, "Esc");
        let pick = Pick::start(false, String::new(), Box::new(|| {}));
        pick.cancel();
        let state = wait_for(&pick, |state| !matches!(state, PickState::Waiting(_)));
        assert_eq!(state, PickState::Cancelled, "cancel");

        // Hide windows and pick: Sue's own windows (the patch is one) go
        // while it waits and come back after.
        // SAFETY: reads a fact of the patch's window, alive until the
        // patch is dropped at the end of the test.
        let shown = || unsafe { IsWindowVisible(HWND(patch.window() as *mut _)) }.as_bool();
        assert!(shown());
        let pick = Pick::start(true, String::new(), Box::new(|| {}));
        wait_up(&pick);
        assert!(!shown(), "hidden while the pick waits");
        pick.press_escape();
        let state = wait_for(&pick, |state| !matches!(state, PickState::Waiting(_)));
        assert_eq!(state, PickState::Cancelled, "Esc after hiding");
        let start = Instant::now();
        while !shown() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(shown(), "shown again after");
    }

    #[test]
    fn a_patch_sits_in_the_middle_of_the_main_screen_at_a_size_it_can_hold() {
        // Reckoned without a window: a second patch on the screen would
        // lie over the one the pick test reads, which runs meanwhile.
        assert_eq!(patch_area([1920, 1080], 240), [840, 420, 240]);
        assert_eq!(patch_area([1920, 1080], 1), [952, 532, 16], "at least 16");
        assert_eq!(
            patch_area([800, 600], 5000),
            [100, 0, 600],
            "no bigger than the screen"
        );
        assert_eq!(patch_area([0, 0], 240), [-8, -8, 16], "no screen: no panic");
    }

    #[test]
    fn a_point_outside_every_screen_is_an_error_not_a_panic() {
        let [left, top, width, height] = virtual_screen();
        for (x, y) in [
            (left - 50_000, top),
            (left + width + 50_000, top + height + 50_000),
            (i32::MIN, i32::MIN),
            (i32::MAX, i32::MAX),
        ] {
            assert_eq!(pixel_at(x, y), Err(ScreenError::OffScreen), "{x}, {y}");
        }
    }

    #[test]
    fn a_failed_capture_is_an_error_not_a_panic() {
        pretend_capture_fails(true);
        let [left, top, ..] = virtual_screen();
        assert_eq!(pixel_at(left, top), Err(ScreenError::Capture));
        pretend_capture_fails(false);
    }

    #[test]
    fn the_label_grows_with_the_screen_scale() {
        assert_eq!(label_size(96), LABEL_SIZE);
        assert_eq!(label_size(0), LABEL_SIZE, "an unknown scale reads as 100 %");
        assert_eq!(label_size(192), [392, 88]);
        assert_eq!(at_dpi(6, 144), 9);
    }

    #[test]
    fn colorrefs_read_as_red_green_blue() {
        assert_eq!(rgb(COLORREF(0x0033_2211)), [0x11, 0x22, 0x33]);
    }
}
