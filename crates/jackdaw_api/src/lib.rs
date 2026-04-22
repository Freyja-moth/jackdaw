//! Public API for Jackdaw editor extensions.
//!
//! Thin facade over [`jackdaw_api_internal`]. Re-exports only the
//! surface intended for third-party extension and game authors; the
//! FFI entry structs, macro-emission helpers, and cleanup registries
//! stay behind `jackdaw_api_internal`.
//!
//! # Static consumer
//!
//! ```toml
//! jackdaw_api = "0.4"
//! ```
//!
//! # Dylib extension
//!
//! ```toml
//! jackdaw_api = { version = "0.4", features = ["dynamic_linking"] }
//! bevy = "0.18"
//! ```
//!
//! The host binary must set jackdaw's `dylib` feature for runtime
//! dylib loading to be sound.

// Forces a link dependency on `jackdaw_dylib` so both the editor and
// every extension dylib share one compiled copy of jackdaw's types.
// Mirrors what `bevy/dynamic_linking` does via `bevy_dylib`.
#[cfg(feature = "dynamic_linking")]
#[allow(unused_imports)]
use jackdaw_dylib as _;

pub use jackdaw_api_internal::prelude;

pub use jackdaw_api_internal::{export_extension, export_game};

pub use jackdaw_api_internal::{
    DynJackdawExtension, ExtensionContext, ExtensionPoint, HierarchyWindow, InspectorWindow,
    JackdawExtension, MenuEntryDescriptor, PanelContext, SectionBuildFn, WindowDescriptor,
};

pub use jackdaw_api_internal::ExtensionLoaderPlugin;

pub use jackdaw_api_internal::{lifecycle, operator, pie, runtime, snapshot};

pub use jackdaw_api_internal::macros;

pub use jackdaw_api_internal::jsn;
