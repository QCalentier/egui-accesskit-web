//! An [AccessKit](https://accesskit.dev) adapter for the web.
//!
//! A toolkit drawing on a `<canvas>` (egui, and any other on AccessKit)
//! gives a screen reader nothing but an empty picture there: AccessKit's
//! adapters talk to Windows, macOS, Linux, Android and iOS, none to a
//! browser. This adapter mirrors the tree the toolkit builds into
//! elements laid over the canvas, invisible but present: each node an
//! element with its ARIA role, name, states and place, so the browser's
//! own accessibility gives it to screen readers as any page's (browse
//! mode, headings, landmarks, forms). The focus follows the toolkit's, and
//! what a screen reader does to an element (focusing it, activating it)
//! comes back as AccessKit's [`ActionRequest`](accesskit::ActionRequest)s.
//!
//! ```ignore
//! let adapter = accesskit_web::Adapter::new(&canvas, |request| {
//!     // hand the request to the toolkit
//! })?;
//! // each frame:
//! adapter.update(tree_update);
//! ```
//!
//! Only what changed is touched each frame: AccessKit's consumer says
//! which nodes were added, changed or removed.

mod dom;
mod element;
mod mirror;
mod role;
mod web;

pub use web::Adapter;
