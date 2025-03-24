#![allow(internal_features)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(rustdoc_internals))]
#![doc = include_str!("../README.md")]
#![cfg_attr(doc, deny(missing_docs))]

extern crate proc_macro;

use hephae_macros::{
    Manifest,
    proc_macro2::TokenStream,
    quote::{ToTokens, quote, quote_spanned},
    syn,
    syn::{
        Error, Ident, Token, Type, parenthesized,
        parse::{Parse, ParseStream},
        spanned::Spanned,
    },
};

struct Syntax {
    #[cfg(feature = "atlas")]
    atlas: Option<TokenStream>,
    #[cfg(feature = "locale")]
    locale: Option<TokenStream>,
    render: Option<TokenStream>,
    #[cfg(feature = "text")]
    text: Option<TokenStream>,
    #[cfg(feature = "ui")]
    ui: Option<TokenStream>,
}

impl Parse for Syntax {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        fn no_data(id: &str, input: ParseStream) -> syn::Result<()> {
            if input.peek(Token![:]) {
                Err(input.error(format!("`{id}` doesn't accept type arguments")))
            } else {
                Ok(())
            }
        }

        fn no_redefine(
            id: &str,
            dst: &mut Option<TokenStream>,
            src: impl FnOnce() -> syn::Result<TokenStream>,
            tokens: impl ToTokens,
        ) -> syn::Result<()> {
            match dst {
                Some(..) => Err(Error::new_spanned(tokens, format!("`{id}` defined multiple times"))),
                None => {
                    *dst = Some(src()?);
                    Ok(())
                }
            }
        }

        #[allow(unused)]
        fn unsupported(id: &str, tokens: impl ToTokens) -> Error {
            Error::new_spanned(
                tokens,
                format!("`{id}` unsupported; have you enabled the feature in `Cargo.toml`?"),
            )
        }

        let manifest = Manifest::get();

        #[cfg(feature = "atlas")]
        let mut atlas = None;
        #[cfg(feature = "locale")]
        let mut locale = None;
        let mut render = None;
        #[cfg(feature = "text")]
        let mut text = None;
        #[cfg(feature = "ui")]
        let mut ui = None;

        while !input.is_empty() {
            if let Some(token) = input.parse::<Option<Token![..]>>()? {
                if !input.is_empty() {
                    return Err(input.error("no more tokens are allowed after `..`"))
                }

                #[cfg(feature = "atlas")]
                if let (true, Ok(atlas_crate)) = (atlas.is_none(), manifest.resolve("hephae", "atlas", token)) {
                    atlas = Some(quote_spanned! { token.span() => #atlas_crate::AtlasPlugin::default() })
                }
                #[cfg(feature = "locale")]
                if let (true, Ok(locale_crate)) = (locale.is_none(), manifest.resolve("hephae", "locale", token)) {
                    locale = Some(quote_spanned! { token.span() => #locale_crate::LocalePlugin::<(), ()>::default() })
                }
                if let (true, Ok(render_crate)) = (render.is_none(), manifest.resolve("hephae", "render", token)) {
                    render = Some(quote_spanned! { token.span() => #render_crate::RendererPlugin::<(), ()>::default() })
                }
                #[cfg(feature = "text")]
                if let (true, Ok(text_crate)) = (text.is_none(), manifest.resolve("hephae", "text", token)) {
                    text = Some(quote_spanned! { token.span() => #text_crate::TextPlugin::default() })
                }
                #[cfg(feature = "ui")]
                if let (true, Ok(ui_crate)) = (ui.is_none(), manifest.resolve("hephae", "ui", token)) {
                    ui = Some(quote_spanned! { token.span() => #ui_crate::UiPlugin::<(), ()>::default() })
                }

                break
            } else {
                let id = input.parse::<Ident>()?;
                match &*id.to_string() {
                    #[cfg(feature = "atlas")]
                    "atlas" => {
                        no_data("atlas", input)?;
                        no_redefine(
                            "atlas",
                            &mut atlas,
                            || {
                                let atlas_crate = manifest.resolve("hephae", "atlas", &id)?;
                                Ok(quote_spanned! { id.span() => #atlas_crate::AtlasPlugin::default() })
                            },
                            &id,
                        )?
                    }
                    #[cfg(not(feature = "atlas"))]
                    "atlas" => return Err(unsupported("atlas", id)),
                    #[cfg(feature = "locale")]
                    "locale" => no_redefine(
                        "locale",
                        &mut locale,
                        || {
                            let locale_crate = manifest.resolve("hephae", "locale", &id)?;
                            if input.parse::<Option<Token![:]>>()?.is_some() {
                                let data;
                                parenthesized!(data in input);

                                let arg = data.parse::<Type>()?;
                                data.parse::<Token![,]>()?;
                                let target = data.parse::<Type>()?;
                                data.parse::<Option<Token![,]>>()?;

                                if !data.is_empty() {
                                    return Err(data.error("expected end of tuple"))
                                }

                                Ok(quote_spanned! { id.span() => #locale_crate::LocalePlugin::<#arg, #target>::default() })
                            } else {
                                Ok(quote_spanned! { id.span() => #locale_crate::LocalePlugin::<(), ()>::default() })
                            }
                        },
                        &id,
                    )?,
                    #[cfg(not(feature = "locale"))]
                    "locale" => return Err(unsupported("locale", id)),
                    "render" => no_redefine(
                        "render",
                        &mut render,
                        || {
                            let render_crate = manifest.resolve("hephae", "render", &id)?;
                            if input.parse::<Option<Token![:]>>()?.is_some() {
                                let data;
                                parenthesized!(data in input);

                                let vertex = data.parse::<Type>()?;
                                data.parse::<Token![,]>()?;
                                let drawer = data.parse::<Type>()?;
                                data.parse::<Option<Token![,]>>()?;

                                if !data.is_empty() {
                                    return Err(data.error("expected end of tuple"))
                                }

                                Ok(
                                    quote_spanned! { id.span() => #render_crate::RendererPlugin::<#vertex, #drawer>::default() },
                                )
                            } else {
                                Ok(quote_spanned! { id.span() => #render_crate::RendererPlugin::<(), ()>::default() })
                            }
                        },
                        &id,
                    )?,
                    #[cfg(feature = "text")]
                    "text" => {
                        no_data("text", input)?;
                        no_redefine(
                            "text",
                            &mut text,
                            || {
                                let text_crate = manifest.resolve("hephae", "text", &id)?;
                                Ok(quote_spanned! { id.span() => #text_crate::TextPlugin::default() })
                            },
                            &id,
                        )?
                    }
                    #[cfg(not(feature = "text"))]
                    "text" => return Err(unsupported("text", id)),
                    #[cfg(feature = "ui")]
                    "ui" => no_redefine(
                        "ui",
                        &mut ui,
                        || {
                            let ui_crate = manifest.resolve("hephae", "ui", &id)?;
                            if input.parse::<Option<Token![:]>>()?.is_some() {
                                let data;
                                parenthesized!(data in input);

                                let measure = data.parse::<Type>()?;
                                data.parse::<Token![,]>()?;
                                let root = data.parse::<Type>()?;
                                data.parse::<Option<Token![,]>>()?;

                                if !data.is_empty() {
                                    return Err(data.error("expected end of tuple"))
                                }

                                Ok(quote_spanned! { id.span() => #ui_crate::UiPlugin::<#measure, #root>::default() })
                            } else {
                                Ok(quote_spanned! { id.span() => #ui_crate::UiPlugin::<(), ()>::default() })
                            }
                        },
                        &id,
                    )?,
                    #[cfg(not(feature = "ui"))]
                    "ui" => return Err(unsupported("ui", id)),
                    other => return Err(Error::new_spanned(id, format!("unknown plugin `{other}`"))),
                }
            }

            if input.is_empty() {
                break
            } else {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(Self {
            #[cfg(feature = "atlas")]
            atlas,
            #[cfg(feature = "locale")]
            locale,
            render,
            #[cfg(feature = "text")]
            text,
            #[cfg(feature = "ui")]
            ui,
        })
    }
}

/// The `hephae! { ... }` procedural macro for specifying Hephae plugins.
///
/// ```rust,no_run
/// # use hephae_plugins::hephae;
///
/// // You can define these plugin attributes in any order, as long as you only define it either zero or one times.
/// hephae! {
///     // `atlas`: Requires the `"atlas"` feature, and does not accept any type arguments.
///     atlas,
///     // `locale`: Requires the `"locale"` feature, optionally accepts `(ArgConf, TargetConf)` type arguments.
///     locale: ((MyLocaleArg1, MyLocaleArg2, ..), (MyTarget1, MyTarget2, ..)),
///     // `render`: Always available, optionally accepts `(VertexConf, DrawerConf)` type arguments.
///     render: ((MyVertex1, MyVertex2, ..), (MyDrawer1, MyDrawer2, ..)),
///     // `text`: Requires the `"text"` feature, and does not accept any type arguments.
///     text,
///     // `ui`: Requires the `"ui"` feature, optionally accepts `(MeasureConf, RootConf)` type arguments.
///     ui: ((MyMeasurer1, MyMeasurer2, ..), (MyRoot1, MyRoot2, ..)),
///     // You can also tell the macro to include every Hephae features via the feature flags with their default settings.
///     // Note that this `..` syntax must appear at the very last.
///     ..
/// }
/// ```
#[proc_macro]
pub fn hephae(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    fn parse_inner(input: TokenStream) -> syn::Result<TokenStream> {
        let span = input.span();
        let Syntax {
            atlas,
            locale,
            render,
            text,
            ui,
        } = syn::parse2(input)?;

        let mut plugins = Vec::with_capacity(5);

        if let Some(render) = render {
            plugins.push(render)
        }
        #[cfg(feature = "atlas")]
        if let Some(atlas) = atlas {
            plugins.push(atlas)
        }
        #[cfg(feature = "locale")]
        if let Some(locale) = locale {
            plugins.push(locale)
        }
        #[cfg(feature = "text")]
        if let Some(text) = text {
            plugins.push(text)
        }
        #[cfg(feature = "ui")]
        if let Some(ui) = ui {
            plugins.push(ui)
        }

        match &*plugins {
            [] => Err(Error::new(
                span,
                "at least one plugin must be specified, or `..` use all defaults",
            )),
            [data] => Ok(quote! { #data }),
            data => Ok(quote! { (#(#data),*) }),
        }
    }

    parse_inner(input.into()).unwrap_or_else(Error::into_compile_error).into()
}
