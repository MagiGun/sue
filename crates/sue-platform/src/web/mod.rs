//! The browser, for Sue built to WebAssembly: its files kept in memory and
//! in IndexedDB ([`store`]), and the page around the canvas: pickers,
//! downloads, the clipboard and dropped files ([`page`]). Everything here
//! runs on the page's one thread; nothing needs wasm threads, which a
//! static host such as GitHub Pages cannot enable.

pub(crate) mod page;
pub(crate) mod store;

pub use page::{failed, random_seed};
