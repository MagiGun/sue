//! Cameras: which ones this computer has, and a stream of pictures from
//! one, read on a thread of its own so the interface never waits. Sue
//! Paint's Hand Scanner is the one user: the camera runs only while its
//! dialog is open, and the pictures stay in memory on this machine.
//!
//! [`cameras`] lists them and [`CameraStream::open`] starts one at a
//! modest size. The stream keeps only the newest picture
//! ([`CameraStream::latest`]), shared, never copied. Dropping the stream
//! stops the camera.
//!
//! Windows is implemented (Media Foundation, in the audited crate
//! `crates/sue-platform/windows`, module `camera`). Every other system
//! answers [`CameraError::Unsupported`] for now, so the interface can say
//! that cameras are not available there yet.
//!
//! A stream reads from any [`CameraSource`]: tests and previews hand it a
//! [`FakeCamera`] ([`CameraStream::with`]) and need no hardware.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A camera of this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraInfo {
    /// What the system calls it.
    pub name: String,
    /// What tells it apart from the others, for [`CameraStream::open`].
    pub id: String,
}

/// Why a camera could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CameraError {
    /// Sue cannot use cameras on this system yet.
    Unsupported,
    /// This computer has no camera, or not the one asked for.
    NoCamera,
    /// Another program is using the camera.
    Busy,
    /// The system's privacy settings do not let Sue use the camera.
    Denied,
    /// The camera stopped sending pictures (unplugged, or switched off).
    Gone,
    /// Anything else: what the system said.
    Failed(String),
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported => f.write_str("cameras are not available on this system yet"),
            Self::NoCamera => f.write_str("no camera was found"),
            Self::Busy => f.write_str("another program is using the camera"),
            Self::Denied => f.write_str("the system does not let Sue use the camera"),
            Self::Gone => f.write_str("the camera stopped"),
            Self::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for CameraError {}

/// One picture from a camera.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraFrame {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// `width * height * 3` bytes of red, green and blue, rows from the
    /// top.
    pub rgb: Vec<u8>,
    /// Which picture of its stream it is, counted from one.
    pub number: u64,
}

/// Something that gives pictures one after another: a real camera, or a
/// made-up one. It lives on the stream's thread.
pub trait CameraSource {
    /// Waits for the next picture: its width, height and RGB bytes.
    ///
    /// # Errors
    ///
    /// [`CameraError`] when the camera cannot give one; the stream then
    /// ends.
    fn read(&mut self) -> Result<(u32, u32, Vec<u8>), CameraError>;
}

/// Makes a [`CameraSource`] on the stream's own thread (a real camera
/// must be opened on the thread that reads it).
pub type CameraOpener = Box<dyn FnOnce() -> Result<Box<dyn CameraSource>, CameraError> + Send>;

/// `true` on systems where Sue can use cameras.
#[must_use]
pub fn available() -> bool {
    cfg!(target_os = "windows")
}

/// The cameras of this computer; an empty list when it has none.
///
/// # Errors
///
/// [`CameraError::Unsupported`] where Sue cannot use cameras yet,
/// [`CameraError::Failed`] when the system cannot list them.
pub fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    #[cfg(target_os = "windows")]
    {
        sue_windows::camera::cameras()
            .map(|found| {
                found
                    .into_iter()
                    .map(|camera| CameraInfo {
                        name: camera.name,
                        id: camera.id,
                    })
                    .collect()
            })
            .map_err(system::error)
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err(CameraError::Unsupported)
    }
}

#[cfg(target_os = "windows")]
mod system {
    //! Windows' cameras as [`CameraSource`]s.

    use sue_windows::camera::{Camera, CameraError as Os};

    use super::{CameraError, CameraSource};

    pub(super) fn error(error: Os) -> CameraError {
        match error {
            Os::NoCamera => CameraError::NoCamera,
            Os::Busy => CameraError::Busy,
            Os::Denied => CameraError::Denied,
            Os::Gone => CameraError::Gone,
            other @ Os::Os { .. } => CameraError::Failed(other.to_string()),
        }
    }

    impl CameraSource for Camera {
        fn read(&mut self) -> Result<(u32, u32, Vec<u8>), CameraError> {
            let picture = Camera::read(self).map_err(error)?;
            Ok((picture.width, picture.height, picture.rgb))
        }
    }

    pub(super) fn open(
        id: Option<&str>,
        want: (u32, u32),
    ) -> Result<Box<dyn CameraSource>, CameraError> {
        Ok(Box::new(Camera::open(id, want).map_err(error)?))
    }
}

/// Opens the system's camera `id` (the first one for `None`) at about
/// `want` pixels, as a source to read on the calling thread: what
/// [`CameraStream::open`] runs on the stream's own thread.
///
/// # Errors
///
/// [`CameraError::Unsupported`] where Sue cannot use cameras yet, and
/// whatever keeps the camera from opening.
pub fn open_source(
    id: Option<&str>,
    want: (u32, u32),
) -> Result<Box<dyn CameraSource>, CameraError> {
    #[cfg(target_os = "windows")]
    {
        system::open(id, want)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (id, want);
        Err(CameraError::Unsupported)
    }
}

/// What the stream's thread and its owner share.
struct Shared {
    latest: Mutex<Option<Arc<CameraFrame>>>,
    error: Mutex<Option<CameraError>>,
    stop: AtomicBool,
    paused: AtomicBool,
    count: AtomicU64,
}

/// A running camera: its newest picture, read on a thread of its own.
/// Dropping it stops the camera.
pub struct CameraStream {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for CameraStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CameraStream")
            .field("pictures", &self.pictures())
            .field("paused", &self.paused())
            .finish_non_exhaustive()
    }
}

impl CameraStream {
    /// The size the Hand Scanner asks a camera for: enough to find a
    /// hand, small enough to be quick.
    pub const MODEST: (u32, u32) = (640, 480);

    /// Starts the camera `id` (from [`cameras`]; the first one when
    /// `None`) at about `want` pixels. It answers at once: a camera that
    /// cannot be opened shows as [`Self::error`] a moment later. `wake`
    /// is called (on the camera's thread) whenever there is a new
    /// picture or an error to look at.
    #[must_use]
    pub fn open(id: Option<&str>, want: (u32, u32), wake: impl Fn() + Send + 'static) -> Self {
        let id = id.map(str::to_owned);
        Self::with(Box::new(move || open_source(id.as_deref(), want)), wake)
    }

    /// A stream over whatever `open` makes: a real camera, or a
    /// [`FakeCamera`].
    #[must_use]
    pub fn with(open: CameraOpener, wake: impl Fn() + Send + 'static) -> Self {
        let shared = Arc::new(Shared {
            latest: Mutex::new(None),
            error: Mutex::new(None),
            stop: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            count: AtomicU64::new(0),
        });
        let thread = Arc::clone(&shared);
        let fail = |shared: &Shared, error: CameraError| {
            if let Ok(mut slot) = shared.error.lock() {
                *slot = Some(error);
            }
        };
        let started = std::thread::Builder::new()
            .name("sue-camera".to_owned())
            .spawn(move || {
                let mut source = match open() {
                    Ok(source) => source,
                    Err(error) => {
                        fail(&thread, error);
                        wake();
                        return;
                    }
                };
                while !thread.stop.load(Ordering::Relaxed) {
                    if thread.paused.load(Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_millis(15));
                        continue;
                    }
                    match source.read() {
                        Ok((width, height, rgb)) => {
                            let number = thread.count.fetch_add(1, Ordering::Relaxed) + 1;
                            if let Ok(mut slot) = thread.latest.lock() {
                                *slot = Some(Arc::new(CameraFrame {
                                    width,
                                    height,
                                    rgb,
                                    number,
                                }));
                            }
                            wake();
                        }
                        Err(error) => {
                            fail(&thread, error);
                            wake();
                            return;
                        }
                    }
                }
                // The source is dropped here, on its own thread: the
                // camera is released.
            });
        if let Err(error) = started {
            fail(&shared, CameraError::Failed(error.to_string()));
        }
        Self { shared }
    }

    /// The newest picture, if the camera has sent one. It is shared, not
    /// copied.
    #[must_use]
    pub fn latest(&self) -> Option<Arc<CameraFrame>> {
        self.shared.latest.lock().ok()?.clone()
    }

    /// How many pictures the camera has sent.
    #[must_use]
    pub fn pictures(&self) -> u64 {
        self.shared.count.load(Ordering::Relaxed)
    }

    /// Why the camera stopped, if it did. The stream is over then.
    #[must_use]
    pub fn error(&self) -> Option<CameraError> {
        self.shared.error.lock().ok()?.clone()
    }

    /// Holds the stream still (`true`: no new pictures are read, the
    /// newest stays) or lets it run again.
    pub fn pause(&self, paused: bool) {
        self.shared.paused.store(paused, Ordering::Relaxed);
    }

    /// `true` while the stream is held still.
    #[must_use]
    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }
}

impl Drop for CameraStream {
    fn drop(&mut self) {
        // The thread ends after the picture it is waiting for and
        // releases the camera; nobody waits for it.
        self.shared.stop.store(true, Ordering::Relaxed);
    }
}

/// What paints a [`FakeCamera`]'s picture: its number and its RGB bytes.
type Painter = Box<dyn FnMut(u64, &mut [u8]) + Send>;

/// A camera that needs no hardware: it gives the pictures a function
/// makes, at a steady pace, for tests and previews.
pub struct FakeCamera {
    width: u32,
    height: u32,
    pace: Duration,
    count: u64,
    paint: Painter,
    /// Fails with this after so many pictures.
    ends: Option<(u64, CameraError)>,
}

impl std::fmt::Debug for FakeCamera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FakeCamera")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

impl FakeCamera {
    /// A camera of `width` by `height` pixels whose picture number `n`
    /// (from 0) is what `paint` writes into its RGB bytes (all zero
    /// before), one every `pace`.
    #[must_use]
    pub fn new(
        width: u32,
        height: u32,
        pace: Duration,
        paint: impl FnMut(u64, &mut [u8]) + Send + 'static,
    ) -> Self {
        Self {
            width,
            height,
            pace,
            count: 0,
            paint: Box::new(paint),
            ends: None,
        }
    }

    /// The same camera, failing with `error` after `pictures` pictures.
    #[must_use]
    pub fn ending(mut self, pictures: u64, error: CameraError) -> Self {
        self.ends = Some((pictures, error));
        self
    }

    /// What starts it on a stream's thread ([`CameraStream::with`]).
    pub fn opener(self) -> CameraOpener {
        Box::new(move || Ok(Box::new(self) as Box<dyn CameraSource>))
    }

    /// An opener that fails with `error`, as a camera that cannot be
    /// opened does.
    pub fn failing(error: CameraError) -> CameraOpener {
        Box::new(move || Err(error))
    }
}

impl CameraSource for FakeCamera {
    fn read(&mut self) -> Result<(u32, u32, Vec<u8>), CameraError> {
        if let Some((pictures, error)) = &self.ends
            && self.count >= *pictures
        {
            return Err(error.clone());
        }
        std::thread::sleep(self.pace);
        let mut rgb = vec![0_u8; self.width as usize * self.height as usize * 3];
        (self.paint)(self.count, &mut rgb);
        self.count += 1;
        Ok((self.width, self.height, rgb))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn wait_for(what: &str, done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() {
            assert!(
                started.elapsed().as_secs() < 10,
                "waited too long for {what}"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn a_stream_keeps_the_newest_picture_and_wakes_its_owner() {
        let woken = Arc::new(AtomicU64::new(0));
        let wake = Arc::clone(&woken);
        let camera = FakeCamera::new(4, 2, Duration::from_millis(1), |n, rgb| {
            rgb.fill(n as u8 + 1)
        });
        let stream = CameraStream::with(camera.opener(), move || {
            wake.fetch_add(1, Ordering::Relaxed);
        });
        wait_for("three pictures", || stream.pictures() >= 3);
        let frame = stream.latest().expect("a picture");
        assert_eq!((frame.width, frame.height), (4, 2));
        assert_eq!(frame.rgb.len(), 24);
        assert!(frame.number >= 3);
        assert_eq!(frame.rgb[0], frame.number as u8, "picture n is painted n");
        assert!(woken.load(Ordering::Relaxed) >= 3);
        assert_eq!(stream.error(), None);
        // The same picture is handed out shared, not copied.
        let again = stream.latest().expect("a picture");
        assert!(again.number > frame.number || Arc::ptr_eq(&again, &frame));
    }

    #[test]
    fn a_paused_stream_stands_still_and_runs_again() {
        let camera = FakeCamera::new(2, 2, Duration::from_millis(1), |_, _| {});
        let stream = CameraStream::with(camera.opener(), || {});
        wait_for("a picture", || stream.pictures() >= 1);
        stream.pause(true);
        assert!(stream.paused());
        // Whatever was being read when it paused arrives; then nothing.
        std::thread::sleep(Duration::from_millis(40));
        let held = stream.pictures();
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(stream.pictures(), held);
        assert!(stream.latest().is_some(), "the newest picture stays");
        stream.pause(false);
        wait_for("more pictures", || stream.pictures() > held);
    }

    #[test]
    fn a_camera_that_cannot_open_or_stops_says_why() {
        for error in [
            CameraError::NoCamera,
            CameraError::Busy,
            CameraError::Denied,
            CameraError::Unsupported,
        ] {
            let stream = CameraStream::with(FakeCamera::failing(error.clone()), || {});
            wait_for("the error", || stream.error().is_some());
            assert_eq!(stream.error(), Some(error));
            assert!(stream.latest().is_none());
        }
        // Unplugged after two pictures: the two stay, the error shows.
        let camera =
            FakeCamera::new(2, 2, Duration::from_millis(1), |_, _| {}).ending(2, CameraError::Gone);
        let stream = CameraStream::with(camera.opener(), || {});
        wait_for("the error", || stream.error().is_some());
        assert_eq!(stream.error(), Some(CameraError::Gone));
        assert_eq!(stream.pictures(), 2);
        assert_eq!(stream.latest().expect("the last picture").number, 2);
    }

    #[test]
    fn dropping_a_stream_stops_its_camera() {
        let reads = Arc::new(AtomicU64::new(0));
        let count = Arc::clone(&reads);
        let camera = FakeCamera::new(2, 2, Duration::from_millis(1), move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
        });
        let stream = CameraStream::with(camera.opener(), || {});
        wait_for("a picture", || stream.pictures() >= 1);
        drop(stream);
        std::thread::sleep(Duration::from_millis(40));
        let after = reads.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(60));
        assert_eq!(
            reads.load(Ordering::Relaxed),
            after,
            "the camera kept running"
        );
    }

    #[test]
    fn every_error_reads_as_a_sentence() {
        assert_eq!(CameraError::NoCamera.to_string(), "no camera was found");
        assert_eq!(
            CameraError::Unsupported.to_string(),
            "cameras are not available on this system yet"
        );
        assert_eq!(CameraError::Failed("why".to_owned()).to_string(), "why");
    }

    #[test]
    fn cameras_are_listed_where_sue_can_use_them() {
        match cameras() {
            Ok(_) => assert!(available()),
            Err(CameraError::Unsupported) => assert!(!available()),
            Err(other) => panic!("could not list cameras: {other}"),
        }
    }

    /// A real camera, through the stream: by hand only.
    #[test]
    #[ignore = "needs a camera"]
    fn a_real_camera_streams_pictures() {
        let stream = CameraStream::open(None, CameraStream::MODEST, || {});
        wait_for("five pictures or an error", || {
            stream.pictures() >= 5 || stream.error().is_some()
        });
        assert_eq!(stream.error(), None);
        let frame = stream.latest().expect("a picture");
        println!(
            "{} x {}, picture {}",
            frame.width, frame.height, frame.number
        );
        assert_eq!(frame.rgb.len(), (frame.width * frame.height * 3) as usize);
    }
}
