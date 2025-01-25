use std::{io::Error as IoError, str::FromStr};

use bevy_asset::{io::Reader, ron, ron::de::SpannedError, AssetLoader, LoadContext, ParseAssetPathError};
use bevy_utils::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::def::{Locale, LocaleFmt, Locales};

#[derive(Error, Debug)]
pub enum LocaleError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    InvalidFile(#[from] SpannedError),
    #[error("syntax error at key '{key}' on char {position}")]
    SyntaxError { key: String, position: usize },
}

pub struct LocaleLoader;
impl AssetLoader for LocaleLoader {
    type Asset = Locale;
    type Settings = ();
    type Error = LocaleError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        _: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let file = ron::de::from_bytes::<HashMap<String, String>>(&{
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            bytes
        })?;

        let mut asset = HashMap::<String, LocaleFmt>::with_capacity(file.len());
        for (key, value) in file {
            match LocaleFmt::from_str(&value) {
                Ok(fmt) => {
                    asset.insert_unique_unchecked(key, fmt);
                }
                Err(position) => return Err(LocaleError::SyntaxError { key, position }),
            }
        }

        Ok(Locale(asset))
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["locale.ron"]
    }
}

#[derive(Error, Debug)]
pub enum LocalesError {
    #[error(transparent)]
    Io(#[from] IoError),
    #[error(transparent)]
    InvalidFile(#[from] SpannedError),
    #[error(transparent)]
    InvalidPath(#[from] ParseAssetPathError),
    #[error("locale default '{0}' is defined, but is not available in `locales`")]
    MissingDefault(String),
}

#[derive(Serialize, Deserialize)]
pub struct LocalesFile {
    pub default: String,
    pub locales: Vec<String>,
}

pub struct LocalesLoader;
impl AssetLoader for LocalesLoader {
    type Asset = Locales;
    type Settings = ();
    type Error = LocalesError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let file = ron::de::from_bytes::<LocalesFile>(&{
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            bytes
        })?;

        let mut asset = Locales {
            default: file.default,
            locales: HashMap::with_capacity(file.locales.len()),
        };

        for key in file.locales {
            let path = load_context.asset_path().resolve_embed(&format!("locale_{key}.locale.ron"))?;
            asset.locales.insert(key, load_context.load(path));
        }

        if !asset.locales.contains_key(&asset.default) {
            return Err(LocalesError::MissingDefault(asset.default))
        }

        Ok(asset)
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["locales.ron"]
    }
}
