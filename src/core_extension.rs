use bevy::prelude::*;
use jackdaw_api::prelude::*;

pub(super) fn plugin(app: &mut App) {
    app.register_extension::<JackdawCoreExtension>();
}

#[derive(Default)]
pub struct JackdawCoreExtension;

impl JackdawExtension for JackdawCoreExtension {
    fn name() -> String {
        "Jackdaw Core Extension".to_string()
    }
    fn kind() -> ExtensionKind {
        ExtensionKind::Builtin
    }

    fn register(&self, _ctx: &mut ExtensionContext) {
        // todo: move as much builtin functionality into operators here as possible!
    }
}
