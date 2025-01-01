#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub use cosmic_text;

pub mod asset;
pub mod compute;
pub mod def;

use bevy_app::prelude::*;
use bevy_asset::prelude::*;

use crate::asset::{Font, FontLoader};

#[derive(Copy, Clone, Default)]
pub struct HephaeTextPlugin;
impl Plugin for HephaeTextPlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<Font>()
            .register_asset_reflect::<Font>()
            .register_asset_loader(FontLoader);
    }
}
