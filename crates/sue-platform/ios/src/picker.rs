//! The document picker, shown over the topmost view controller of Sue's
//! window. Everything here runs on the main thread, where UIKit and Sue's
//! event loop run.

use std::cell::{Cell, OnceCell};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_foundation::{NSArray, NSString, NSURL};
use objc2_ui_kit::{
    UIApplication, UIDocumentPickerViewController, UIViewController, UIWindowScene,
};

use audited::Delegate;

/// How the picker answered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picked {
    /// The chosen file or folder; for an export, where the file went.
    Chosen(PathBuf),
    /// The picker closed without a choice.
    Cancelled,
    /// The picker answered nothing Sue can use.
    Failed(String),
}

static PICKED: Mutex<Option<Picked>> = Mutex::new(None);

thread_local! {
    /// The pickers' delegate, made once: UIKit keeps only a weak reference.
    static DELEGATE: OnceCell<Retained<Delegate>> = const { OnceCell::new() };
    /// The shown picker chooses a folder, whose access must be started.
    static FOLDER: Cell<bool> = const { Cell::new(false) };
}

/// The answer of the picker shown last, once it closed.
#[must_use]
pub fn take_picked() -> Option<Picked> {
    PICKED.lock().unwrap_or_else(PoisonError::into_inner).take()
}

fn answer(picked: Picked) {
    *PICKED.lock().unwrap_or_else(PoisonError::into_inner) = Some(picked);
}

/// Shows the picker to open a copy of a document with one of
/// `extensions` (any document when there are none).
///
/// # Errors
/// Off the main thread, or when Sue's window shows no view controller.
pub fn show_open(extensions: &[String]) -> Result<(), String> {
    let mtm = main_thread()?;
    let types = audited::content_types(extensions, false)?;
    present(mtm, &audited::opening(mtm, &types, true), false)
}

/// Shows the picker to choose a folder, which Sue then writes in
/// directly.
///
/// # Errors
/// As [`show_open`].
pub fn show_folder() -> Result<(), String> {
    let mtm = main_thread()?;
    let types = audited::content_types(&[], true)?;
    present(mtm, &audited::opening(mtm, &types, false), true)
}

/// Shows the picker to export a copy of `file` to a place in Files.
///
/// # Errors
/// As [`show_open`].
pub fn show_export(file: &Path) -> Result<(), String> {
    let mtm = main_thread()?;
    let url = NSURL::fileURLWithPath(&NSString::from_str(&file.to_string_lossy()));
    let urls = NSArray::from_retained_slice(&[url]);
    let picker = UIDocumentPickerViewController::initForExportingURLs_asCopy(
        UIDocumentPickerViewController::alloc(mtm),
        &urls,
        true,
    );
    present(mtm, &picker, false)
}

fn main_thread() -> Result<MainThreadMarker, String> {
    MainThreadMarker::new().ok_or_else(|| "the document picker opens on the main thread".to_owned())
}

/// Shows `picker` over the topmost view controller.
fn present(
    mtm: MainThreadMarker,
    picker: &UIDocumentPickerViewController,
    folder: bool,
) -> Result<(), String> {
    let top = top_controller(mtm).ok_or_else(|| "Sue's window is not showing".to_owned())?;
    let delegate = DELEGATE.with(|delegate| delegate.get_or_init(|| Delegate::new(mtm)).clone());
    picker.setAllowsMultipleSelection(false);
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    FOLDER.set(folder);
    *PICKED.lock().unwrap_or_else(PoisonError::into_inner) = None;
    top.presentViewController_animated_completion(picker, true, None);
    Ok(())
}

/// The view controller on top of the key window of Sue's scene.
fn top_controller(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
    let app = UIApplication::sharedApplication(mtm);
    let window = app
        .connectedScenes()
        .allObjects()
        .iter()
        .find_map(|scene| scene.downcast::<UIWindowScene>().ok()?.keyWindow());
    #[allow(
        deprecated,
        reason = "the one key window, for a scene that has none yet"
    )]
    let window = window.or_else(|| app.keyWindow())?;
    let mut top = window.rootViewController()?;
    while let Some(next) = top.presentedViewController() {
        top = next;
    }
    Some(top)
}

/// The path of the first URL the picker chose.
fn chosen(urls: &NSArray<NSURL>) -> Picked {
    let Some(url) = urls.firstObject() else {
        return Picked::Cancelled;
    };
    if FOLDER.get() {
        audited::start_access(&url);
    }
    match url.path() {
        Some(path) => Picked::Chosen(PathBuf::from(path.to_string())),
        None => Picked::Failed("the picker chose no file on this device".to_owned()),
    }
}

/// Every call into Objective-C that Rust counts as unsafe.
#[allow(
    unsafe_code,
    reason = "UIKit's document picker needs a delegate class and calls with no safe binding; approved on 2026-10-29"
)]
mod audited {
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
    use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
    use objc2_foundation::{NSArray, NSString, NSURL};
    use objc2_ui_kit::{UIDocumentPickerDelegate, UIDocumentPickerViewController};

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and the
        // delegate has no instance variables and no `Drop`.
        #[unsafe(super(NSObject))]
        #[thread_kind = MainThreadOnly]
        #[name = "SueDocumentPickerDelegate"]
        pub(super) struct Delegate;

        unsafe impl NSObjectProtocol for Delegate {}

        unsafe impl UIDocumentPickerDelegate for Delegate {
            #[unsafe(method(documentPicker:didPickDocumentsAtURLs:))]
            fn did_pick(&self, _picker: &UIDocumentPickerViewController, urls: &NSArray<NSURL>) {
                super::answer(super::chosen(urls));
            }

            #[unsafe(method(documentPickerWasCancelled:))]
            fn cancelled(&self, _picker: &UIDocumentPickerViewController) {
                super::answer(super::Picked::Cancelled);
            }
        }
    );

    impl Delegate {
        pub(super) fn new(mtm: MainThreadMarker) -> Retained<Self> {
            let this = Self::alloc(mtm).set_ivars(());
            // SAFETY: NSObject's `init` on a freshly allocated object.
            unsafe { msg_send![super(this), init] }
        }
    }

    /// The `UTType`s for `extensions`, or for folders: anything when
    /// there are none. The class is looked up at run time; UIKit loads
    /// its framework.
    pub(super) fn content_types(
        extensions: &[String],
        folder: bool,
    ) -> Result<Retained<NSArray<AnyObject>>, String> {
        let class = AnyClass::get(c"UTType").ok_or_else(|| "UTType is missing".to_owned())?;
        let identified = |identifier: &str| -> Option<Retained<AnyObject>> {
            // SAFETY: `+[UTType typeWithIdentifier:]` takes a string and
            // returns a type or nil.
            unsafe { msg_send![class, typeWithIdentifier: &*NSString::from_str(identifier)] }
        };
        let mut types: Vec<Retained<AnyObject>> = Vec::new();
        if folder {
            types.extend(identified("public.folder"));
        } else {
            for extension in extensions {
                // SAFETY: `+[UTType typeWithFilenameExtension:]` takes a
                // string and returns a type (dynamic for unknown ones).
                let kind: Option<Retained<AnyObject>> = unsafe {
                    msg_send![class, typeWithFilenameExtension: &*NSString::from_str(extension)]
                };
                types.extend(kind);
            }
            if types.is_empty() {
                types.extend(identified("public.item"));
            }
        }
        Ok(NSArray::from_retained_slice(&types))
    }

    /// A picker that opens documents of `types`, as copies or in place.
    pub(super) fn opening(
        mtm: MainThreadMarker,
        types: &NSArray<AnyObject>,
        as_copy: bool,
    ) -> Retained<UIDocumentPickerViewController> {
        // SAFETY: `initForOpeningContentTypes:asCopy:` takes an array of
        // `UTType`s, which `content_types` makes.
        unsafe {
            msg_send![
                UIDocumentPickerViewController::alloc(mtm),
                initForOpeningContentTypes: types,
                asCopy: as_copy
            ]
        }
    }

    /// Starts access to a folder outside Sue's sandbox, kept while Sue
    /// runs so that Sue can write in it.
    pub(super) fn start_access(url: &NSURL) {
        // SAFETY: a URL the document picker chose; access is never
        // stopped, which only keeps it open until Sue quits.
        let _ = unsafe { url.startAccessingSecurityScopedResource() };
    }
}
