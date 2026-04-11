//! The animation crate deliberately exports **no** custom
//! `EditorCommand` types.
//!
//! All mutations to the authored-clip data model flow through the
//! existing AST command primitives in the main editor:
//!
//! - **Create clip / track / keyframe** — `SpawnEntity` with the
//!   matching reflected components ([`Clip`], [`AnimTrack`],
//!   [`Vec3Keyframe`] / [`QuatKeyframe`] / [`F32Keyframe`]).
//! - **Edit any field** — `SetJsnField` on the relevant component and
//!   field path. For instance, moving a keyframe in time is
//!   `SetJsnField` on `Vec3Keyframe.time`.
//! - **Delete anything** — `DespawnEntity`, which cascades to children
//!   (keyframes under a track, tracks under a clip).
//!
//! This module exists only to host that documentation; it intentionally
//! has no public items. Consumers should reach for
//! `jackdaw::commands::{SpawnEntity, SetJsnField, DespawnEntity}` from
//! the main editor crate when implementing new editing features.
//!
//! [`Clip`]: crate::clip::Clip
//! [`AnimTrack`]: crate::clip::AnimTrack
//! [`Vec3Keyframe`]: crate::clip::Vec3Keyframe
//! [`QuatKeyframe`]: crate::clip::QuatKeyframe
//! [`F32Keyframe`]: crate::clip::F32Keyframe
