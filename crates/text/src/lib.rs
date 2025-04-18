#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals, doc_cfg))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

use bevy::{app::PluginGroupBuilder, platform::sync::Mutex, prelude::*, render::RenderApp};
use hephae_utils::prelude::*;

use crate::{
    atlas::{ExtractedFontAtlases, FontAtlas, extract_font_atlases},
    def::{Font, FontLoader, Text, TextAlign, TextFont, TextSpan, TextWrap, compute_structure, notify_structure},
    layout::{FontLayout, FontLayoutInner, load_fonts_to_database},
};

pub mod atlas;
pub mod def;
pub mod layout;

/// Common imports for [`hephae_text`](crate).
pub mod prelude {
    pub use crate::{
        atlas::ExtractedFontAtlases,
        def::{Font, Text, TextAlign, TextFont, TextGlyph, TextGlyphs, TextSpan, TextStructure, TextWrap},
        layout::FontLayout,
    };
}

pub use cosmic_text;

plugin_def! {
    /// Provides text-rendering functionality into the app.
    #[plugin_group]
    pub struct TextPlugin;
    fn build(self) -> PluginGroupBuilder {
        #[allow(unused_mut)]
        let mut builder = PluginGroupBuilder::start::<Self>().add(|app: &mut App| {
            let (sender, receiver) = async_channel::bounded(4);
            app.init_asset::<Font>()
                .init_asset::<FontAtlas>()
                .register_asset_loader(FontLoader { add_to_database: sender })
                .insert_resource(FontLayout(Mutex::new(FontLayoutInner::new(receiver))))
                .register_type::<Text>()
                .register_type::<TextWrap>()
                .register_type::<TextAlign>()
                .register_type::<TextFont>()
                .register_type::<TextSpan>()
                .configure_sets(Update, HephaeTextSystems::LoadFontsToDatabase)
                .configure_sets(PostUpdate, HephaeTextSystems::ComputeStructure)
                .add_systems(Update, load_fonts_to_database.in_set(HephaeTextSystems::LoadFontsToDatabase))
                .add_systems(
                    PostUpdate,
                    (compute_structure, notify_structure)
                        .chain()
                        .in_set(HephaeTextSystems::ComputeStructure),
                );

            if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
                render_app
                    .init_resource::<ExtractedFontAtlases>()
                    .configure_sets(ExtractSchedule, HephaeTextSystems::ExtractFontAtlases)
                    .add_systems(
                        ExtractSchedule,
                        extract_font_atlases.in_set(HephaeTextSystems::ExtractFontAtlases),
                    );
            }
        });

        #[cfg(feature = "locale")]
        {
            builder = builder
                .add(hephae_locale::LocaleTargetPlugin::<Text>::default())
                .add(hephae_locale::LocaleTargetPlugin::<TextSpan>::default());
        }

        builder
    }
}

/// Labels for systems added by Hephae Text.
#[derive(SystemSet, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HephaeTextSystems {
    /// System in [`ExtractSchedule`] that extracts font atlases into the render world.
    ExtractFontAtlases,
    /// System in [`Update`] that loads bytes sent from [`FontLoader`] into a [`Font`] and adds them
    /// to the database.
    LoadFontsToDatabase,
    /// System in [`PostUpdate`] that computes and marks [`TextStructure`](def::TextStructure) as
    /// changed as necessary, for convenience of systems wishing to listen for change-detection.
    ComputeStructure,
}
