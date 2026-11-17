//! The cameras of this computer, through Media Foundation: which ones
//! there are, and pictures from one, for Sue Paint's Hand Scanner.
//!
//! [`cameras`] lists the video capture devices. [`Camera::open`] starts
//! one through the Source Reader at the size nearest the one asked for,
//! with the reader's own video processing turning whatever the camera
//! sends (YUY2, NV12, Motion JPEG) into 32-bit RGB; [`Camera::read`] waits
//! for the next picture and gives it as RGB bytes, rows from the top.
//! Dropping the camera shuts its device down, so its light goes out.
//!
//! What can go wrong is told apart ([`CameraError`]): no camera, a camera
//! another program holds, a camera Windows' privacy settings keep from
//! desktop apps, and a camera unplugged while it ran.
//!
//! A camera starts COM and Media Foundation on the thread that opens it
//! and stays on that thread (it is not `Send`): `sue-platform` gives it a
//! thread of its own.

use std::marker::PhantomData;

use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFMediaSource, IMFMediaType, IMFSourceReader,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_MT_DEFAULT_STRIDE,
    MF_MT_FRAME_SIZE, MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING,
    MF_SOURCE_READER_FIRST_VIDEO_STREAM, MF_SOURCE_READERF_ENDOFSTREAM, MFCreateAttributes,
    MFCreateMediaType, MFCreateSourceReaderFromMediaSource, MFEnumDeviceSources,
    MFGetStrideForBitmapInfoHeader, MFMediaType_Video, MFVideoFormat_RGB32,
};
use windows::Win32::System::Com::CoTaskMemFree;
use windows::core::{GUID, HRESULT, PWSTR};

use crate::media::Session;

/// Why a camera could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CameraError {
    /// This computer has no camera, or not the one asked for.
    NoCamera,
    /// Another program is using the camera.
    Busy,
    /// Windows' privacy settings do not let desktop apps use the camera.
    Denied,
    /// The camera stopped sending pictures (unplugged, or switched off).
    Gone,
    /// A Media Foundation or COM call failed: which step, the HRESULT and
    /// the system's own message.
    Os {
        /// What Sue was doing.
        step: &'static str,
        /// The HRESULT, as Windows reports it.
        code: i32,
        /// The system's text for it.
        message: String,
    },
}

impl std::fmt::Display for CameraError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoCamera => f.write_str("no camera"),
            Self::Busy => f.write_str("the camera is in use"),
            Self::Denied => f.write_str("the camera is not allowed"),
            Self::Gone => f.write_str("the camera stopped"),
            Self::Os {
                step,
                code,
                message,
            } => write!(f, "{step}: 0x{:08X} {message}", *code as u32),
        }
    }
}

impl std::error::Error for CameraError {}

/// The device is held by another program (the HRESULT of
/// `ERROR_SHARING_VIOLATION` and of `ERROR_BUSY`).
const SHARING_VIOLATION: i32 = 0x8007_0020_u32 as i32;
const DEVICE_BUSY: i32 = 0x8007_00AA_u32 as i32;
/// `MF_E_HW_MFT_FAILED_START_STREAMING`: what a camera another program
/// holds answers when asked to start.
const FAILED_START_STREAMING: i32 = 0xC00D_3704_u32 as i32;
/// `MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED` and
/// `MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED`: the camera was unplugged, or
/// taken by another program.
const DEVICE_INVALIDATED: i32 = 0xC00D_3EA2_u32 as i32;
const DEVICE_PREEMPTED: i32 = 0xC00D_3EA3_u32 as i32;
/// `ERROR_NOT_FOUND` and `ERROR_FILE_NOT_FOUND` as HRESULTs: no such
/// device.
const NOT_FOUND: i32 = 0x8007_0490_u32 as i32;
const FILE_NOT_FOUND: i32 = 0x8007_0002_u32 as i32;

/// What a failed call means for a camera, from its HRESULT.
fn classify(step: &'static str, code: HRESULT, message: &str) -> CameraError {
    match code.0 {
        code if code == E_ACCESSDENIED.0 => CameraError::Denied,
        SHARING_VIOLATION | DEVICE_BUSY | FAILED_START_STREAMING | DEVICE_PREEMPTED => {
            CameraError::Busy
        }
        DEVICE_INVALIDATED => CameraError::Gone,
        NOT_FOUND | FILE_NOT_FOUND => CameraError::NoCamera,
        code => CameraError::Os {
            step,
            code,
            message: message.trim().to_owned(),
        },
    }
}

/// Turns a failed call into a [`CameraError`] naming `step`.
fn os(step: &'static str) -> impl FnOnce(windows::core::Error) -> CameraError {
    move |error| classify(step, error.code(), &error.message())
}

fn session() -> Result<Session, CameraError> {
    Session::start().map_err(|error| match error {
        crate::media::MediaError::Os {
            step,
            code,
            message,
        } => CameraError::Os {
            step,
            code,
            message,
        },
        other => CameraError::Os {
            step: "starting Media Foundation",
            code: 0,
            message: other.to_string(),
        },
    })
}

/// A camera of this computer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraInfo {
    /// What Windows calls it.
    pub name: String,
    /// What tells it apart from the others (its symbolic link), for
    /// [`Camera::open`].
    pub id: String,
}

/// The video capture devices, as activation objects. The caller holds a
/// [`Session`].
#[allow(
    unsafe_code,
    reason = "enumerating capture devices is Media Foundation and COM (standing approval, rule 13)"
)]
fn devices() -> Result<Vec<IMFActivate>, CameraError> {
    let mut store: Option<IMFAttributes> = None;
    // SAFETY: `store` is a valid place for the out pointer.
    unsafe { MFCreateAttributes(&mut store, 1) }.map_err(os("making attributes"))?;
    let store = store.ok_or(CameraError::NoCamera)?;
    // SAFETY: both GUIDs are statics that live for the call; the store
    // copies them.
    unsafe {
        store.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )
    }
    .map_err(os("asking for cameras"))?;
    let mut list: *mut Option<IMFActivate> = std::ptr::null_mut();
    let mut count = 0_u32;
    // SAFETY: `list` and `count` are valid places for the out values. On
    // success `list` points at `count` activation objects in memory from
    // CoTaskMemAlloc, which this function now owns.
    unsafe { MFEnumDeviceSources(&store, &mut list, &mut count) }.map_err(os("listing cameras"))?;
    let mut found = Vec::with_capacity(count as usize);
    if !list.is_null() {
        for index in 0..count as usize {
            // SAFETY: `index` is below `count`, the length Windows gave
            // for the array at `list`. `read` moves the object out of the
            // array (each slot is read exactly once and the array is
            // freed below without dropping its slots), so its one
            // reference is owned by `found` from here.
            if let Some(device) = unsafe { list.add(index).read() } {
                found.push(device);
            }
        }
        // SAFETY: `list` came from CoTaskMemAlloc (MFEnumDeviceSources'
        // contract) and is freed once; every slot was moved out above.
        unsafe { CoTaskMemFree(Some(list.cast())) };
    }
    Ok(found)
}

/// A text attribute of a device (`key`), empty when it has none.
#[allow(
    unsafe_code,
    reason = "reading a device's attributes is COM (standing approval, rule 13)"
)]
fn text_of(device: &IMFActivate, key: &GUID) -> String {
    let mut text = PWSTR::null();
    let mut length = 0_u32;
    // SAFETY: `key` lives for the call; `text` and `length` are valid
    // places for the out values. On success `text` points at `length`
    // UTF-16 units in memory from CoTaskMemAlloc, owned here.
    if unsafe { device.GetAllocatedString(key, &mut text, &mut length) }.is_err() || text.is_null()
    {
        return String::new();
    }
    // SAFETY: `text` is non-null and readable for `length` units, as
    // GetAllocatedString just said; the units are copied into a String
    // before the memory is freed.
    let out =
        String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text.0, length as usize) });
    // SAFETY: `text` came from CoTaskMemAlloc and is freed once, after
    // its last use.
    unsafe { CoTaskMemFree(Some(text.0.cast())) };
    out
}

/// The cameras of this computer, in Windows' order. An empty list when
/// there is none.
///
/// # Errors
///
/// [`CameraError::Os`] when Media Foundation cannot list them.
pub fn cameras() -> Result<Vec<CameraInfo>, CameraError> {
    let _session = session()?;
    Ok(devices()?
        .iter()
        .map(|device| CameraInfo {
            name: text_of(device, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME),
            id: text_of(
                device,
                &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
            ),
        })
        .collect())
}

/// The size, of those a camera offers, to use when `want` is asked for:
/// the smallest that is at least as large both ways, else the largest
/// there is. `None` when it offers none.
#[must_use]
pub fn pick_size(offered: &[(u32, u32)], want: (u32, u32)) -> Option<(u32, u32)> {
    let area = |size: &(u32, u32)| u64::from(size.0) * u64::from(size.1);
    offered
        .iter()
        .filter(|size| size.0 >= want.0 && size.1 >= want.1)
        .min_by_key(|size| area(size))
        .or_else(|| offered.iter().max_by_key(|size| area(size)))
        .copied()
}

/// A picture of 32-bit pixels (blue, green, red, unused) as RGB bytes,
/// rows from the top. `stride` is the bytes from one row to the next in
/// `data`; a negative one means the rows are stored from the bottom up,
/// as Windows keeps RGB pictures. `None` when `data` is too short for
/// the size.
#[must_use]
pub fn bgrx_to_rgb(data: &[u8], width: usize, height: usize, stride: isize) -> Option<Vec<u8>> {
    let step = stride.unsigned_abs();
    if width == 0 || height == 0 || step < width * 4 {
        return None;
    }
    if data.len() < step * (height - 1) + width * 4 {
        return None;
    }
    let mut out = Vec::with_capacity(width * height * 3);
    for row in 0..height {
        let stored = if stride < 0 { height - 1 - row } else { row };
        let line = &data[stored * step..stored * step + width * 4];
        for [blue, green, red, _] in line.as_chunks::<4>().0 {
            out.extend_from_slice(&[*red, *green, *blue]);
        }
    }
    Some(out)
}

/// One picture from a camera.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// `width * height * 3` bytes of red, green and blue, rows from the
    /// top.
    pub rgb: Vec<u8>,
}

/// A camera that is sending pictures.
pub struct Camera {
    reader: IMFSourceReader,
    source: IMFMediaSource,
    width: u32,
    height: u32,
    stride: i32,
    // Declared last: the objects above are released before Media
    // Foundation shuts down.
    _session: Session,
    _thread: PhantomData<*const ()>,
}

impl std::fmt::Debug for Camera {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Camera")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// The first video stream of a source reader.
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

#[allow(
    unsafe_code,
    reason = "reading a media type is COM (standing approval, rule 13)"
)]
fn frame_size(kind: &IMFMediaType) -> Option<(u32, u32)> {
    // SAFETY: the key is a static that lives for the call.
    let size = unsafe { kind.GetUINT64(&MF_MT_FRAME_SIZE) }.ok()?;
    Some(((size >> 32) as u32, size as u32))
}

impl Camera {
    /// Starts the camera `id` (from [`cameras`]; the first camera when
    /// `None`) at the size nearest `want`.
    ///
    /// # Errors
    ///
    /// [`CameraError::NoCamera`] when there is no such camera,
    /// [`CameraError::Busy`] when another program holds it,
    /// [`CameraError::Denied`] when Windows' privacy settings refuse it,
    /// [`CameraError::Os`] for anything else.
    #[allow(
        unsafe_code,
        reason = "the Source Reader is Media Foundation and COM (standing approval, rule 13)"
    )]
    pub fn open(id: Option<&str>, want: (u32, u32)) -> Result<Self, CameraError> {
        let session = session()?;
        let device = devices()?
            .into_iter()
            .find(|device| {
                id.is_none_or(|id| {
                    text_of(
                        device,
                        &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                    ) == id
                })
            })
            .ok_or(CameraError::NoCamera)?;
        // SAFETY: `device` is a live activation object; the result is a
        // new reference to the media source it makes.
        let source: IMFMediaSource =
            unsafe { device.ActivateObject() }.map_err(os("starting the camera"))?;
        let made = Self::reader(&source, want);
        match made {
            Ok((reader, width, height, stride)) => Ok(Self {
                reader,
                source,
                width,
                height,
                stride,
                _session: session,
                _thread: PhantomData,
            }),
            Err(error) => {
                // SAFETY: `source` is live; Shutdown releases the device
                // and may be called once, which this is (the source is
                // dropped right after).
                let _ = unsafe { source.Shutdown() };
                Err(error)
            }
        }
    }

    /// A source reader over `source` that gives RGB pictures at the size
    /// nearest `want`: the reader, the size and the stride it settled on.
    #[allow(
        unsafe_code,
        reason = "the Source Reader is Media Foundation and COM (standing approval, rule 13)"
    )]
    fn reader(
        source: &IMFMediaSource,
        want: (u32, u32),
    ) -> Result<(IMFSourceReader, u32, u32, i32), CameraError> {
        let mut settings: Option<IMFAttributes> = None;
        // SAFETY: `settings` is a valid place for the out pointer.
        unsafe { MFCreateAttributes(&mut settings, 1) }.map_err(os("making attributes"))?;
        let settings = settings.ok_or(CameraError::NoCamera)?;
        // The reader's own converter, so any camera format comes out as
        // RGB.
        // SAFETY: the key is a static that lives for the call.
        unsafe { settings.SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1) }
            .map_err(os("setting an attribute"))?;
        // SAFETY: `source` and `settings` are live for the call; the
        // result is a new object.
        let reader = unsafe { MFCreateSourceReaderFromMediaSource(source, &settings) }
            .map_err(os("reading the camera"))?;

        // The formats the camera offers, each with its size.
        let mut native: Vec<((u32, u32), IMFMediaType)> = Vec::new();
        for index in 0..512 {
            // SAFETY: plain numbers in; the result is a new object.
            match unsafe { reader.GetNativeMediaType(VIDEO, index) } {
                Ok(kind) => native.extend(frame_size(&kind).map(|size| (size, kind))),
                // Past the last one, or a camera that will not say: what
                // was found so far is used.
                Err(_) => break,
            }
        }
        let offered: Vec<(u32, u32)> = native.iter().map(|(size, _)| *size).collect();
        let size = pick_size(&offered, want);
        // The camera is first set to send that size in a format of its
        // own (asking for RGB alone leaves it at its largest size)...
        if let Some((_, kind)) = native.iter().find(|(found, _)| Some(*found) == size) {
            // SAFETY: `kind` is live for the call; the reserved pointer
            // is None. A camera that refuses keeps its own format.
            let _ = unsafe { reader.SetCurrentMediaType(VIDEO, None, kind) };
        }

        // ...then the reader is asked to hand it over as RGB; if it will
        // not at that size, as RGB at the camera's own.
        let ask = |with_size: bool| -> Result<(), CameraError> {
            // SAFETY: MFCreateMediaType takes nothing and returns a new
            // object.
            let kind = unsafe { MFCreateMediaType() }.map_err(os("making a media type"))?;
            // SAFETY: the keys and GUID values are statics that live for
            // the calls; the type copies them.
            unsafe {
                kind.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
                    .and_then(|()| kind.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32))
            }
            .map_err(os("describing a media type"))?;
            if let (true, Some((width, height))) = (with_size, size) {
                // SAFETY: the key is a static that lives for the call.
                unsafe {
                    kind.SetUINT64(
                        &MF_MT_FRAME_SIZE,
                        (u64::from(width) << 32) | u64::from(height),
                    )
                }
                .map_err(os("describing a media type"))?;
            }
            // SAFETY: `kind` is live for the call; the reserved pointer
            // is None.
            unsafe { reader.SetCurrentMediaType(VIDEO, None, &kind) }
                .map_err(os("choosing the camera's format"))
        };
        ask(true).or_else(|_| ask(false))?;

        // SAFETY: plain number in; the result is a new object.
        let kind = unsafe { reader.GetCurrentMediaType(VIDEO) }.map_err(os("reading the type"))?;
        let (width, height) = frame_size(&kind).ok_or(CameraError::Os {
            step: "reading the size",
            code: 0,
            message: String::new(),
        })?;
        // The stride says whether rows run from the top (positive) or
        // from the bottom (negative). A type that does not say is asked
        // through the bitmap rule, by which RGB runs from the bottom.
        // SAFETY: the key is a static that lives for the call.
        let stride = match unsafe { kind.GetUINT32(&MF_MT_DEFAULT_STRIDE) } {
            Ok(stride) => stride as i32,
            // SAFETY: plain numbers in (the format's FOURCC and a width).
            Err(_) => unsafe { MFGetStrideForBitmapInfoHeader(MFVideoFormat_RGB32.data1, width) }
                .unwrap_or(-(width as i32 * 4)),
        };
        Ok((reader, width, height, stride))
    }

    /// The size of its pictures, across and down.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Waits for the camera's next picture.
    ///
    /// # Errors
    ///
    /// [`CameraError::Busy`] when another program holds the camera (some
    /// cameras only say so at the first picture), [`CameraError::Gone`]
    /// when it stopped, [`CameraError::Os`] for anything else.
    #[allow(
        unsafe_code,
        reason = "the Source Reader is Media Foundation and COM (standing approval, rule 13)"
    )]
    pub fn read(&mut self) -> Result<Picture, CameraError> {
        // A camera may answer with no picture a few times (a gap in the
        // stream, a format change); it is asked again, not forever.
        for _ in 0..64 {
            let mut flags = 0_u32;
            let mut sample = None;
            // SAFETY: the out values point at live locals; the unused
            // outs are None. The call waits for the next sample.
            unsafe {
                self.reader
                    .ReadSample(VIDEO, 0, None, Some(&mut flags), None, Some(&mut sample))
            }
            .map_err(os("reading a picture"))?;
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                return Err(CameraError::Gone);
            }
            let Some(sample) = sample else { continue };
            // SAFETY: `sample` is live; the result is a new reference.
            let buffer =
                unsafe { sample.ConvertToContiguousBuffer() }.map_err(os("joining a picture"))?;
            let mut data: *mut u8 = std::ptr::null_mut();
            let mut filled = 0_u32;
            // SAFETY: the out values point at live locals.
            unsafe { buffer.Lock(&mut data, None, Some(&mut filled)) }
                .map_err(os("locking a picture"))?;
            let rgb = if data.is_null() {
                None
            } else {
                // SAFETY: Lock gave `data`, non-null and readable for
                // `filled` bytes until the Unlock below; the pixels are
                // copied out (into `rgb`) before then.
                let all = unsafe { std::slice::from_raw_parts(data, filled as usize) };
                bgrx_to_rgb(
                    all,
                    self.width as usize,
                    self.height as usize,
                    self.stride as isize,
                )
            };
            // SAFETY: pairs the Lock above.
            let _ = unsafe { buffer.Unlock() };
            if let Some(rgb) = rgb {
                return Ok(Picture {
                    width: self.width,
                    height: self.height,
                    rgb,
                });
            }
        }
        Err(CameraError::Gone)
    }
}

impl Drop for Camera {
    #[allow(
        unsafe_code,
        reason = "releasing the capture device is COM (standing approval, rule 13)"
    )]
    fn drop(&mut self) {
        // SAFETY: `source` is live until this struct's fields drop, after
        // this call; Shutdown is called once, here, and releases the
        // device so other programs can use it and its light goes out.
        let _ = unsafe { self.source.Shutdown() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_stored_from_the_top_become_rgb() {
        // Two pixels by two, four bytes each (blue, green, red, unused).
        let data = [
            1, 2, 3, 0, 4, 5, 6, 0, //
            7, 8, 9, 0, 10, 11, 12, 0,
        ];
        assert_eq!(
            bgrx_to_rgb(&data, 2, 2, 8),
            Some(vec![3, 2, 1, 6, 5, 4, 9, 8, 7, 12, 11, 10])
        );
    }

    #[test]
    fn pictures_stored_from_the_bottom_are_turned_upright() {
        let data = [
            1, 2, 3, 0, 4, 5, 6, 0, //
            7, 8, 9, 0, 10, 11, 12, 0,
        ];
        assert_eq!(
            bgrx_to_rgb(&data, 2, 2, -8),
            Some(vec![9, 8, 7, 12, 11, 10, 3, 2, 1, 6, 5, 4])
        );
    }

    #[test]
    fn rows_wider_than_the_picture_are_skipped_over() {
        // One pixel across, two down, in rows of eight bytes.
        let data = [1, 2, 3, 0, 99, 99, 99, 99, 4, 5, 6, 0];
        assert_eq!(bgrx_to_rgb(&data, 1, 2, 8), Some(vec![3, 2, 1, 6, 5, 4]));
        assert_eq!(bgrx_to_rgb(&data, 1, 2, -8), Some(vec![6, 5, 4, 3, 2, 1]));
    }

    #[test]
    fn a_buffer_too_short_for_the_picture_is_refused() {
        let data = [0_u8; 15];
        assert_eq!(bgrx_to_rgb(&data, 2, 2, 8), None);
        assert_eq!(bgrx_to_rgb(&data, 2, 2, 4), None, "a stride under the row");
        assert_eq!(bgrx_to_rgb(&data, 0, 2, 8), None);
        assert_eq!(bgrx_to_rgb(&[], 1, 1, 4), None);
        assert_eq!(bgrx_to_rgb(&[1, 2, 3, 0], 1, 1, 4), Some(vec![3, 2, 1]));
    }

    #[test]
    fn the_size_nearest_the_one_asked_for_is_picked() {
        let offered = [
            (1920, 1080),
            (640, 480),
            (1280, 720),
            (320, 240),
            (640, 360),
        ];
        assert_eq!(pick_size(&offered, (640, 480)), Some((640, 480)));
        assert_eq!(pick_size(&offered, (800, 600)), Some((1280, 720)));
        // Nothing large enough: the largest there is.
        assert_eq!(pick_size(&offered, (4000, 3000)), Some((1920, 1080)));
        assert_eq!(pick_size(&[(320, 240)], (640, 480)), Some((320, 240)));
        assert_eq!(pick_size(&[], (640, 480)), None);
    }

    #[test]
    fn failures_are_told_apart_by_their_code() {
        let of = |code: i32| classify("testing", HRESULT(code), " text \r\n");
        assert_eq!(of(E_ACCESSDENIED.0), CameraError::Denied);
        assert_eq!(of(SHARING_VIOLATION), CameraError::Busy);
        assert_eq!(of(FAILED_START_STREAMING), CameraError::Busy);
        assert_eq!(of(DEVICE_PREEMPTED), CameraError::Busy);
        assert_eq!(of(DEVICE_INVALIDATED), CameraError::Gone);
        assert_eq!(of(NOT_FOUND), CameraError::NoCamera);
        assert_eq!(
            of(0x8000_4005_u32 as i32),
            CameraError::Os {
                step: "testing",
                code: 0x8000_4005_u32 as i32,
                message: "text".to_owned(),
            }
        );
        assert_eq!(
            of(0x8000_4005_u32 as i32).to_string(),
            "testing: 0x80004005 text"
        );
    }

    #[test]
    fn listing_cameras_works_with_or_without_one() {
        // Whatever this machine has, the list is read without failing and
        // every camera in it can be told from the others.
        let found = cameras().expect("the list");
        for (at, camera) in found.iter().enumerate() {
            assert!(!camera.id.is_empty());
            assert!(found[..at].iter().all(|other| other.id != camera.id));
        }
    }

    #[test]
    fn a_camera_that_is_not_there_cannot_be_opened() {
        assert_eq!(
            Camera::open(Some("no such camera"), (640, 480)).unwrap_err(),
            CameraError::NoCamera
        );
    }

    /// A real camera: by hand only, on a machine that has one that no
    /// other program is using.
    #[test]
    #[ignore = "needs a camera"]
    fn a_real_camera_sends_pictures() {
        let found = cameras().expect("the list");
        println!("cameras: {found:?}");
        let mut camera = Camera::open(None, (640, 480)).expect("opens");
        let (width, height) = camera.size();
        println!("size {width} x {height}");
        let started = std::time::Instant::now();
        let mut last = None;
        for _ in 0..10 {
            let picture = camera.read().expect("a picture");
            assert_eq!(picture.rgb.len(), (width * height * 3) as usize);
            last = Some(picture);
        }
        println!("ten pictures in {:?}", started.elapsed());
        let last = last.expect("a picture");
        let mean =
            last.rgb.iter().map(|byte| u64::from(*byte)).sum::<u64>() / last.rgb.len() as u64;
        println!("mean level {mean}");
        if let Ok(path) = std::env::var("SUE_CAMERA_DUMP") {
            let mut file = format!("P6\n{width} {height}\n255\n").into_bytes();
            file.extend_from_slice(&last.rgb);
            std::fs::write(path, file).expect("writes the picture");
        }
    }
}
