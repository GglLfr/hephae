use bevy_asset::prelude::*;
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_reflect::prelude::*;
use cosmic_text::{fontdb::Database, Align, FontSystem, Metrics, SwashCache};

use crate::asset::Font;

#[derive(Resource)]
pub struct Fonts(FontSystem);
impl Default for Fonts {
    #[inline]
    fn default() -> Self {
        let locale = sys_locale::get_locale().unwrap_or("en-US".into());
        Self(FontSystem::new_with_locale_and_db(locale, Database::new()))
    }
}

#[derive(Resource)]
pub struct FontCache(SwashCache);
impl Default for FontCache {
    #[inline]
    fn default() -> Self {
        Self(SwashCache::new())
    }
}

#[derive(Component, Copy, Clone)]
#[require(ComputedTextLayout)]
pub struct TextLayout {
    pub justify: Align,
}

impl Default for TextLayout {
    #[inline]
    fn default() -> Self {
        Self {
            justify: Align::Justified,
        }
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct ComputedTextLayout {
    pub(crate) line: String,
    #[reflect(ignore)]
    pub(crate) buffer: Buffer,
}

#[derive(Deref, DerefMut)]
pub(crate) struct Buffer(pub cosmic_text::Buffer);
impl Default for Buffer {
    #[inline]
    fn default() -> Self {
        Buffer(cosmic_text::Buffer::new_empty(Metrics::new(
            f32::MIN_POSITIVE,
            f32::MIN_POSITIVE,
        )))
    }
}

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct TextSpan(pub String);

#[derive(Component, Reflect, Default)]
#[reflect(Component, Default)]
pub struct TextFont {
    pub font: Handle<Font>,
}
