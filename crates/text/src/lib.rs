#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

pub use cosmic_text;

pub mod atlas;
pub mod def;
pub mod layout;

use bevy_app::prelude::*;
use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_render::{prelude::*, RenderApp};

use crate::{
    atlas::{extract_font_atlases, ExtractedFontAtlases, FontAtlas},
    def::{Font, FontLoader, Text, TextAlign, TextColor, TextFont, TextSpan, TextWrap},
    layout::{load_fonts_to_database, FontLayout},
};

pub mod prelude {
    pub use crate::{
        atlas::ExtractedFontAtlases,
        def::{Font, Text, TextAlign, TextColor, TextFont, TextGlyph, TextGlyphs, TextSpan, TextWrap},
        layout::FontLayout,
        HephaeTextPlugin,
    };
}

#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeTextSystems {
    LoadFontsToDatabase,
    ComputeStructure,
}

#[derive(Copy, Clone, Default)]
pub struct HephaeTextPlugin;
impl Plugin for HephaeTextPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = async_channel::bounded(4);
        app.init_asset::<Font>()
            .init_asset::<FontAtlas>()
            .register_asset_loader(FontLoader { add_to_database: sender })
            .insert_resource(FontLayout::new(receiver))
            .register_type::<Text>()
            .register_type::<TextWrap>()
            .register_type::<TextAlign>()
            .register_type::<TextFont>()
            .register_type::<TextColor>()
            .register_type::<TextSpan>()
            .configure_sets(Update, HephaeTextSystems::LoadFontsToDatabase)
            .configure_sets(PostUpdate, HephaeTextSystems::ComputeStructure)
            .add_systems(Update, load_fonts_to_database.in_set(HephaeTextSystems::LoadFontsToDatabase));

        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .init_resource::<ExtractedFontAtlases>()
                .add_systems(ExtractSchedule, extract_font_atlases);
        }
    }
}
