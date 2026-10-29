//! The page's files: an in-memory [`Vfs`] whose settings, data and
//! documents are copied to the browser's IndexedDB as they are written,
//! and read back from it before Sue starts, so they survive a reload.
//! Caches and the files staged for picks and downloads stay in memory.

use std::cell::RefCell;
use std::io;
use std::path::{Path, PathBuf};

use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, IdbOpenDbRequest, IdbRequest, IdbTransactionMode};

use crate::time::SystemTime;
use crate::vfs::Vfs;

/// The folders kept across reloads (`PlatformPaths` puts everything
/// under `/sue`).
const KEPT: [&str; 3] = ["/sue/config", "/sue/data", "/sue/documents"];
const DATABASE: &str = "sue";
const FILES: &str = "files";

thread_local! {
    static VFS: RefCell<Vfs> = RefCell::new(Vfs::default());
    static DB: RefCell<Option<IdbDatabase>> = const { RefCell::new(None) };
}

fn kept(path: &Path) -> bool {
    KEPT.iter().any(|root| path.starts_with(root))
}

/// Runs `work` on the files.
pub(crate) fn with<T>(work: impl FnOnce(&mut Vfs) -> T) -> T {
    VFS.with(|vfs| work(&mut vfs.borrow_mut()))
}

/// Writes a file, and keeps it if it is in a kept folder.
pub(crate) fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let path = with(|vfs| vfs.write(path, bytes, SystemTime::now()))?;
    if kept(&path) {
        put(&path, bytes);
    }
    crate::web::page::written(&path);
    Ok(())
}

/// Removes a file, here and in the browser's storage.
pub(crate) fn remove_file(path: &Path) -> io::Result<()> {
    with(|vfs| vfs.remove_file(path))?;
    forget(&crate::vfs::normal(path));
    Ok(())
}

/// Removes a folder and everything in it.
pub(crate) fn remove_dir_all(path: &Path) -> io::Result<()> {
    for file in with(|vfs| vfs.remove_dir_all(path))? {
        forget(&file);
    }
    Ok(())
}

/// Moves a file.
pub(crate) fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let to = with(|vfs| vfs.rename(from, to))?;
    forget(&crate::vfs::normal(from));
    if kept(&to)
        && let Some(bytes) = with(|vfs| vfs.get(&to).map(|file| file.bytes.clone()))
    {
        put(&to, &bytes);
    }
    Ok(())
}

fn key(path: &Path) -> JsValue {
    JsValue::from_str(&path.to_string_lossy())
}

fn put(path: &Path, bytes: &[u8]) {
    let value = Uint8Array::from(bytes);
    let key = key(path);
    let _ = with_store(|store| store.put_with_key(&value, &key));
}

fn forget(path: &Path) {
    if kept(path) {
        let key = key(path);
        let _ = with_store(|store| store.delete(&key));
    }
}

fn with_store(
    work: impl FnOnce(&web_sys::IdbObjectStore) -> Result<IdbRequest, JsValue>,
) -> Result<(), JsValue> {
    DB.with(|db| {
        let db = db.borrow();
        let Some(db) = db.as_ref() else {
            return Ok(());
        };
        let store = db
            .transaction_with_str_and_mode(FILES, IdbTransactionMode::Readwrite)?
            .object_store(FILES)?;
        work(&store).map(|_| ())
    })
}

/// Waits for an IndexedDB request and returns its result.
async fn done(request: &IdbRequest) -> Result<JsValue, JsValue> {
    let promise = Promise::new(&mut |resolve, reject| {
        let ok = resolve.clone();
        request.set_onsuccess(Some(
            Closure::once_into_js(move || ok.call0(&JsValue::NULL)).unchecked_ref(),
        ));
        request.set_onerror(Some(
            Closure::once_into_js(move || reject.call0(&JsValue::NULL)).unchecked_ref(),
        ));
    });
    JsFuture::from(promise).await?;
    request.result()
}

/// Opens the browser's storage and reads every kept file into memory.
/// Without storage (a private window that refuses it) Sue still runs,
/// and nothing survives a reload.
pub(crate) async fn load() -> Result<usize, String> {
    let text = |error: JsValue| format!("{error:?}");
    let factory = web_sys::window()
        .ok_or("no window")?
        .indexed_db()
        .map_err(text)?
        .ok_or("this browser keeps no storage for pages")?;
    let open: IdbOpenDbRequest = factory.open_with_u32(DATABASE, 1).map_err(text)?;
    let upgrade = open.clone();
    open.set_onupgradeneeded(Some(
        Closure::once_into_js(move || {
            if let Ok(db) = upgrade.result().and_then(JsCast::dyn_into::<IdbDatabase>) {
                let _ = db.create_object_store(FILES);
            }
        })
        .unchecked_ref(),
    ));
    let db: IdbDatabase = done(&open).await.map_err(text)?.dyn_into().map_err(text)?;
    let store = db
        .transaction_with_str(FILES)
        .and_then(|transaction| transaction.object_store(FILES))
        .map_err(text)?;
    let keys = store.get_all_keys().map_err(text)?;
    let values = store.get_all().map_err(text)?;
    let keys: Array = done(&keys).await.map_err(text)?.unchecked_into();
    let values: Array = done(&values).await.map_err(text)?.unchecked_into();
    let mut count = 0;
    with(|vfs| {
        for (key, value) in keys.iter().zip(values.iter()) {
            if let (Some(path), Ok(bytes)) = (key.as_string(), value.dyn_into::<Uint8Array>()) {
                let _ = vfs.write(&PathBuf::from(path), &bytes.to_vec(), SystemTime::now());
                count += 1;
            }
        }
    });
    DB.with(|slot| *slot.borrow_mut() = Some(db));
    Ok(count)
}
