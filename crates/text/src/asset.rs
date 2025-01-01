use std::{io::Error as IoError, sync::Arc};

use bevy_asset::{io::Reader, prelude::*, AssetLoader, LoadContext, ReflectAsset};
use bevy_reflect::prelude::*;
use cosmic_text::{
    fontdb::Source,
    ttf_parser::{Face, FaceParsingError},
};
use thiserror::Error;

#[derive(Asset, Reflect)]
#[reflect(Asset)]
pub struct Font(Arc<[u8]>);
impl Font {
    #[inline]
    pub fn try_from_bytes(bytes: impl Into<Arc<[u8]>>) -> Result<Self, FaceParsingError> {
        let bytes = bytes.into();

        Face::parse(&bytes, 0)?;
        Ok(Self(bytes))
    }

    #[inline]
    pub fn source(&self) -> Source {
        Source::Binary(Arc::new(self.0.clone()))
    }
}

#[derive(Error, Debug)]
pub enum FontError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    Face(#[from] FaceParsingError),
}

pub struct FontLoader;
impl AssetLoader for FontLoader {
    type Asset = Font;
    type Settings = ();
    type Error = FontError;

    #[inline]
    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;

        let font = Font::try_from_bytes(bytes)?;
        Ok(font)
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["ttf"]
    }
}
