use std::{io::Error as IoError, str::FromStr};

use bevy_asset::{io::Reader, ron, ron::de::SpannedError, AssetLoader, LoadContext, ParseAssetPathError};
use bevy_utils::HashMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::def::{Locale, LocaleFmt, Locales};

impl FromStr for LocaleFmt {
    type Err = usize;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let chars = s.chars().enumerate();

        #[derive(Copy, Clone)]
        enum State {
            Unescaped,
            PreEscaped(bool),
            Index,
        }

        use State::*;

        let mut format = String::new();
        let mut args = Vec::new();
        let mut range = 0..0;
        let mut state = Unescaped;

        let mut index = 0usize;
        for (i, char) in chars {
            match char {
                '{' => {
                    state = match state {
                        Unescaped => PreEscaped(false),
                        PreEscaped(false) => {
                            format.push('{');
                            range.end = format.len();

                            Unescaped
                        }
                        PreEscaped(true) | Index => return Err(i),
                    }
                }
                '}' => {
                    state = match state {
                        Unescaped => PreEscaped(true),
                        PreEscaped(false) => return Err(i),
                        PreEscaped(true) => {
                            format.push('}');
                            range.end = format.len();

                            Unescaped
                        }
                        Index => {
                            args.push((range.clone(), index));
                            range.start = range.end;

                            Unescaped
                        }
                    }
                }
                '0'..='9' => match state {
                    Unescaped => {
                        format.push(char);
                        range.end = format.len();
                    }
                    PreEscaped(false) => state = Index,
                    PreEscaped(true) => return Err(i),
                    Index => {
                        index = index
                            .checked_mul(10)
                            .and_then(|index| index.checked_add(char.to_digit(10)? as usize))
                            .ok_or(i)?;
                    }
                },
                _ => match state {
                    Unescaped => {
                        format.push(char);
                        range.end = format.len();
                    }
                    _ => return Err(i),
                },
            }
        }

        Ok(if args.is_empty() {
            Self::Unformatted(format)
        } else {
            Self::Formatted { format, args }
        })
    }
}

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
