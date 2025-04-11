//! Defines asset loaders for [`Locale`] and [`LocaleLoader`].

use std::{fmt::Formatter, hint::unreachable_unchecked, io, num::ParseIntError, str::FromStr};

use bevy::{
    asset::{AssetLoader, LoadContext, ParseAssetPathError, io::Reader, ron, ron::error::SpannedError},
    platform_support::{collections::HashMap, hash::FixedHasher},
    prelude::*,
};
use derive_more::{Display, Error, From};
use nom::{
    Err as NomErr, IResult, Parser,
    branch::alt,
    bytes::complete::{is_not, tag, take_while_m_n, take_while1},
    character::complete::char,
    combinator::{cut, eof, map_opt, map_res, value, verify},
    error::{FromExternalError, ParseError},
    multi::fold,
    sequence::{delimited, preceded},
};
use nom_language::error::{VerboseError, convert_error};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, Visitor},
};

use crate::def::{Locale, LocaleCollection, LocaleFmt};

enum FmtFrag<'a> {
    Literal(&'a str),
    Escaped(char),
    Index(usize),
}

/// Parses `\u{xxxxxx}` escaped unicode character.
#[inline]
fn parse_unicode<'a, E: ParseError<&'a str> + FromExternalError<&'a str, ParseIntError>>(
    input: &'a str,
) -> IResult<&'a str, char, E> {
    map_opt(
        map_res(
            preceded(
                char('u'),
                cut(delimited(
                    char('{'),
                    take_while_m_n(1, 6, |c: char| c.is_ascii_hexdigit()),
                    char('}'),
                )),
            ),
            |hex| u32::from_str_radix(hex, 16),
        ),
        char::from_u32,
    )
    .parse(input)
}

/// Parses `\...` escaped character.
#[inline]
fn parse_escaped<'a, E: ParseError<&'a str> + FromExternalError<&'a str, ParseIntError>>(
    input: &'a str,
) -> IResult<&'a str, FmtFrag<'a>, E> {
    preceded(
        char('\\'),
        cut(alt((
            parse_unicode,
            value('\n', char('n')),
            value('\r', char('r')),
            value('\t', char('t')),
            value('\u{08}', char('b')),
            value('\u{0C}', char('f')),
            value('\\', char('\\')),
            value('/', char('/')),
            value('"', char('"')),
        ))),
    )
    .map(FmtFrag::Escaped)
    .parse(input)
}

/// Parses `{index}` and extracts the index as positional argument.
#[inline]
fn parse_index<'a, E: ParseError<&'a str> + FromExternalError<&'a str, ParseIntError>>(
    input: &'a str,
) -> IResult<&'a str, FmtFrag<'a>, E> {
    map_res(
        delimited(char('{'), cut(take_while1(|c: char| c.is_ascii_digit())), char('}')),
        usize::from_str,
    )
    .map(FmtFrag::Index)
    .parse(input)
}

/// Parses escaped `{{` and `}}`.
#[inline]
fn parse_brace<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, FmtFrag<'a>, E> {
    alt((value('{', tag("{{")), value('}', tag("}}"))))
        .map(FmtFrag::Escaped)
        .parse(input)
}

/// Parses any characters preceding a backslash or a brace, leaving `{{` and `}}` as special cases.
#[inline]
fn parse_literal<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, FmtFrag<'a>, E> {
    verify(is_not("\\{}"), |s: &str| !s.is_empty())
        .map(FmtFrag::Literal)
        .parse(input)
}

fn parse<'a, E: ParseError<&'a str> + FromExternalError<&'a str, ParseIntError>>(
    input: &'a str,
) -> IResult<&'a str, LocaleFmt, E> {
    cut((
        fold(
            0..,
            alt((parse_literal, parse_brace, parse_index, parse_escaped)),
            || (0, LocaleFmt::Unformatted(String::new())),
            |(start, mut fmt), frag| match frag {
                FmtFrag::Literal(lit) => match &mut fmt {
                    LocaleFmt::Unformatted(format) | LocaleFmt::Formatted { format, .. } => {
                        format.push_str(lit);
                        (start, fmt)
                    }
                },
                FmtFrag::Escaped(c) => match &mut fmt {
                    LocaleFmt::Unformatted(format) | LocaleFmt::Formatted { format, .. } => {
                        format.push(c);
                        (start, fmt)
                    }
                },
                FmtFrag::Index(i) => {
                    let (end, args) = match fmt {
                        LocaleFmt::Unformatted(format) => {
                            fmt = LocaleFmt::Formatted {
                                format,
                                args: Vec::new(),
                            };

                            // Safety: We just set `fmt` to variant `Formatted` above.
                            let LocaleFmt::Formatted { format, args } = &mut fmt else { unsafe { unreachable_unchecked() } };
                            (format.len(), args)
                        }
                        LocaleFmt::Formatted {
                            ref format,
                            ref mut args,
                        } => (format.len(), args),
                    };

                    args.push((start..end, i));
                    (end, fmt)
                }
            },
        ),
        eof,
    ))
    .map(|((.., fmt), ..)| fmt)
    .parse(input)
}

impl Serialize for LocaleFmt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        match self {
            Self::Unformatted(raw) => serializer.serialize_str(raw),
            Self::Formatted { format, args } => {
                let mut out = String::new();

                let mut last = 0;
                for &(ref range, i) in args {
                    // Some sanity checks in case some users for some reason modify the locales manually.
                    let start = range.start.min(format.len());
                    let end = range.end.min(format.len());
                    last = last.max(end);

                    // All these unwraps shouldn't panic.
                    out.push_str(&format[start..end]);
                    out.push('{');
                    out.push_str(&i.to_string());
                    out.push('}');
                }
                out.push_str(&format[last..]);

                serializer.serialize_str(&out)
            }
        }
    }
}

impl<'de> Deserialize<'de> for LocaleFmt {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where D: Deserializer<'de> {
        struct Parser;
        impl Visitor<'_> for Parser {
            type Value = LocaleFmt;

            #[inline]
            fn expecting(&self, formatter: &mut Formatter) -> std::fmt::Result {
                write!(formatter, "a valid UTF-8 string")
            }

            #[inline]
            fn visit_str<E>(self, input: &str) -> Result<Self::Value, E>
            where E: de::Error {
                match parse::<VerboseError<&str>>(input) {
                    Ok(("", fmt)) => Ok(fmt),
                    Ok(..) => unreachable!("`cut(eof)` should've ruled out leftover data"),
                    Err(e) => Err(match e {
                        NomErr::Error(e) | NomErr::Failure(e) => E::custom(convert_error(input, e)),
                        NomErr::Incomplete(..) => unreachable!("only complete operations are used"),
                    }),
                }
            }
        }

        deserializer.deserialize_str(Parser)
    }
}

impl FromStr for LocaleFmt {
    type Err = VerboseError<String>;

    #[inline]
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match parse::<VerboseError<&str>>(input) {
            Ok(("", fmt)) => Ok(fmt),
            Ok(..) => unreachable!("`cut(eof)` should've ruled out leftover data"),
            Err(e) => Err(match e {
                NomErr::Error(e) | NomErr::Failure(e) => e.into(),
                NomErr::Incomplete(..) => unreachable!("only complete operations are used"),
            }),
        }
    }
}

/// Errors that may arise when loading [`Locale`]s using [`LocaleLoader`].
#[derive(Error, Debug, Display, From)]
pub enum LocaleError {
    /// An IO error occurred.
    #[display("{_0}")]
    Io(#[from] io::Error),
    /// A syntax error occurred.
    #[display("{_0}")]
    Ron(#[from] SpannedError),
}

/// Dedicated [`AssetLoader`] for loading [`Locale`]s.
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
        Ok(Locale(ron::de::from_bytes::<HashMap<String, LocaleFmt>>(&{
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            bytes
        })?))
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["locale.ron"]
    }
}

/// Errors that may arise when loading [`LocaleCollection`]s using [`LocaleCollectionLoader`].
#[derive(Error, Debug, Display, From)]
pub enum LocaleCollectionError {
    /// An IO error occurred.
    #[display("{_0}")]
    Io(#[from] io::Error),
    /// A syntax error occurred.
    #[display("{_0}")]
    Ron(#[from] SpannedError),
    /// Invalid sub-asset path.
    #[display("{_0}")]
    InvalidPath(#[from] ParseAssetPathError),
    /// A default locale is defined, but is not available.
    #[display("locale default '{_0}' is defined, but is not available in `locales`")]
    MissingDefault(#[error(not(source))] String),
}

#[derive(Deserialize)]
struct LocaleCollectionFile {
    default: String,
    languages: Vec<String>,
}

/// Dedicated [`AssetLoader`] for loading [`LocaleCollection`]s.
pub struct LocaleCollectionLoader;
impl AssetLoader for LocaleCollectionLoader {
    type Asset = LocaleCollection;
    type Settings = ();
    type Error = LocaleCollectionError;

    async fn load(
        &self,
        reader: &mut dyn Reader,
        _: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let file = ron::de::from_bytes::<LocaleCollectionFile>(&{
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).await?;

            bytes
        })?;

        let mut asset = LocaleCollection {
            default: file.default,
            languages: HashMap::with_capacity_and_hasher(file.languages.len(), FixedHasher),
        };

        for key in file.languages {
            let path = load_context.asset_path().resolve_embed(&format!("locale_{key}.locale.ron"))?;
            asset.languages.insert(key, load_context.load(path));
        }

        if !asset.languages.contains_key(&asset.default) {
            return Err(LocaleCollectionError::MissingDefault(asset.default));
        }

        Ok(asset)
    }

    #[inline]
    fn extensions(&self) -> &[&str] {
        &["locales.ron"]
    }
}
