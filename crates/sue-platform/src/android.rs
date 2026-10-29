//! Android: the activity Android starts Sue in. The launcher
//! (`crates/sue-platform/android`) hands it over with [`set_app`] and
//! [`set_java`] before Sue runs; the event loop, the sandbox paths and the
//! window reach it through [`app`], and the system's own file pickers
//! through Sue's activity in Java (`SueActivity` in `packaging/android`),
//! called with `jni`'s safe API.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The `jni` crate this module calls Java with, for the launcher.
pub use jni;
use jni::objects::{GlobalRef, JObject, JString, JValueOwned};
use jni::{JNIEnv, JavaVM};
pub use winit::platform::android::activity::AndroidApp;

use crate::picks::{Answer, Kind, System};

static APP: OnceLock<AndroidApp> = OnceLock::new();

/// Keeps the activity Android handed to Sue (the first one is kept).
pub fn set_app(app: AndroidApp) {
    let _ = APP.set(app);
}

/// The activity, once Android has started Sue.
#[must_use]
pub fn app() -> Option<AndroidApp> {
    APP.get().cloned()
}

/// The device's shared storage, where people's own files are (the
/// Download, Pictures and Documents folders).
#[must_use]
pub fn shared_storage() -> PathBuf {
    PathBuf::from("/storage/emulated/0")
}

/// Java's VM and Sue's activity object.
struct Java {
    vm: JavaVM,
    activity: GlobalRef,
}

static JAVA: OnceLock<Java> = OnceLock::new();

/// Keeps Java's VM and a reference to Sue's activity, for the system's
/// file pickers (the first pair is kept).
pub fn set_java(vm: JavaVM, activity: GlobalRef) {
    let _ = JAVA.set(Java { vm, activity });
}

/// An argument for a method of Sue's activity.
enum Arg<'a> {
    Int(i32),
    Text(&'a str),
    Texts(&'a [String]),
}

/// Calls `method` of Sue's activity from any thread. Its `String`
/// result, `None` for `null` or a `void` method.
fn call(method: &str, signature: &str, args: &[Arg<'_>]) -> Result<Option<String>, String> {
    let java = JAVA
        .get()
        .ok_or_else(|| "Android's file pickers are not reachable".to_owned())?;
    let mut env = java
        .vm
        .attach_current_thread()
        .map_err(|error| error.to_string())?;
    // A local frame frees the strings made here: the thread may stay
    // attached for good.
    let result = env.with_local_frame(8, |env| {
        invoke(env, java.activity.as_obj(), method, signature, args)
    });
    if env.exception_check().unwrap_or(false) {
        let _ = env.exception_describe();
        let _ = env.exception_clear();
    }
    result.map_err(|error| format!("{method}: {error}"))
}

fn invoke(
    env: &mut JNIEnv<'_>,
    activity: &JObject<'_>,
    method: &str,
    signature: &str,
    args: &[Arg<'_>],
) -> jni::errors::Result<Option<String>> {
    let mut owned: Vec<JValueOwned<'_>> = Vec::with_capacity(args.len());
    for arg in args {
        owned.push(match arg {
            Arg::Int(value) => JValueOwned::Int(*value),
            Arg::Text(text) => JValueOwned::Object(env.new_string(text)?.into()),
            Arg::Texts(texts) => {
                let length = i32::try_from(texts.len()).unwrap_or(i32::MAX);
                let array = env.new_object_array(length, "java/lang/String", JObject::null())?;
                for (index, text) in (0..length).zip(texts.iter()) {
                    let text = env.new_string(text)?;
                    env.set_object_array_element(&array, index, text)?;
                }
                JValueOwned::Object(array.into())
            }
        });
    }
    let values: Vec<_> = owned.iter().map(JValueOwned::borrow).collect();
    let value = env.call_method(activity, method, signature, &values)?;
    if signature.ends_with('V') {
        value.v()?;
        return Ok(None);
    }
    let object = value.l()?;
    if object.is_null() {
        return Ok(None);
    }
    let text = JString::from(object);
    let text: String = env.get_string(&text)?.into();
    Ok(Some(text))
}

/// A helper's answer: `null` when it worked, else why not.
fn done(result: Result<Option<String>, String>) -> Result<(), String> {
    match result? {
        None => Ok(()),
        Some(why) => Err(why),
    }
}

const TEXT_TEXT: &str = "(Ljava/lang/String;Ljava/lang/String;)Ljava/lang/String;";

/// Android's documents (the Storage Access Framework), through Sue's
/// activity: `ACTION_OPEN_DOCUMENT`, `ACTION_CREATE_DOCUMENT` and
/// `ACTION_OPEN_DOCUMENT_TREE`. Places are content URIs.
#[derive(Default)]
pub(crate) struct Documents;

impl System for Documents {
    const SAVE_ASKS_FIRST: bool = true;
    const FOLDERS_ARE_PATHS: bool = false;
    const WRITES_BACK: bool = true;

    fn show(&self, kind: Kind, extensions: &[String], file_name: &str) -> Result<(), String> {
        let kind = match kind {
            Kind::Open => 0,
            Kind::Save => 1,
            Kind::Folder => 2,
        };
        call(
            "showPicker",
            "(I[Ljava/lang/String;Ljava/lang/String;)V",
            &[Arg::Int(kind), Arg::Texts(extensions), Arg::Text(file_name)],
        )
        .map(|_| ())
        .map_err(|why| format!("Android's file picker is not reachable ({why})"))
    }

    fn answer(&self) -> Option<Answer> {
        match call("takePicked", "()Ljava/lang/String;", &[]) {
            Ok(None) => None,
            Ok(Some(text)) if text.is_empty() => Some(Answer::Cancelled),
            Ok(Some(text)) => Some(match text.strip_prefix('!') {
                Some(why) => Answer::Failed(why.to_owned()),
                None => Answer::Chosen(text),
            }),
            Err(why) => Some(Answer::Failed(why)),
        }
    }

    fn name(&self, place: &str) -> String {
        call(
            "displayName",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[Arg::Text(place)],
        )
        .ok()
        .flatten()
        .unwrap_or_else(|| "file".to_owned())
    }

    fn folder(&self, place: &str) -> Option<String> {
        crate::picks::document_folder(place)
    }

    fn copy_in(&self, place: &str, file: &Path) -> Result<(), String> {
        let file = file.to_string_lossy();
        done(call(
            "copyIn",
            TEXT_TEXT,
            &[Arg::Text(place), Arg::Text(&file)],
        ))
    }

    fn copy_out(&self, file: &Path, place: &str) -> Result<(), String> {
        let file = file.to_string_lossy();
        done(call(
            "copyOut",
            TEXT_TEXT,
            &[Arg::Text(&file), Arg::Text(place)],
        ))
    }

    fn create_in(&self, folder: &str, name: &str) -> Result<String, String> {
        match call("createIn", TEXT_TEXT, &[Arg::Text(folder), Arg::Text(name)])? {
            Some(text) => match text.strip_prefix('!') {
                Some(why) => Err(why.to_owned()),
                None => Ok(text),
            },
            None => Err("the folder made no file".to_owned()),
        }
    }

    fn list_in(&self, folder: &str) -> Result<Vec<(String, String)>, String> {
        let text = call(
            "listIn",
            "(Ljava/lang/String;)Ljava/lang/String;",
            &[Arg::Text(folder)],
        )?
        .unwrap_or_default();
        if let Some(why) = text.strip_prefix('!') {
            return Err(why.to_owned());
        }
        // A line each: the document's URI (which has no tabs), a tab and
        // its name.
        Ok(text
            .lines()
            .filter_map(|line| line.split_once('\t'))
            .map(|(document, name)| (document.to_owned(), name.to_owned()))
            .collect())
    }
}
