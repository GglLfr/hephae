use bevy_macro_utils::BevyManifest;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    Attribute, Block, Ident, Meta, Path, Stmt, Token, Visibility,
};

struct Syntax {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    target: Path,
    param: Ident,
    block: Vec<Stmt>,
}

impl Parse for Syntax {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;

        let vis = input.parse()?;
        input.parse::<Token![trait]>()?;
        let name = input.parse()?;
        input.parse::<Token![for]>()?;
        let target = input.parse()?;
        input.parse::<Token![,]>()?;

        let param = input.parse()?;
        input.parse::<Token![=>]>()?;

        let block = input.call(Block::parse_within)?;
        Ok(Self {
            attrs,
            vis,
            name,
            target,
            param,
            block,
        })
    }
}

pub fn parse(input: TokenStream) -> syn::Result<TokenStream> {
    let bevy_app = BevyManifest::get_path_direct("bevy_app");
    let Syntax {
        attrs,
        vis,
        name,
        target,
        param,
        block,
    } = syn::parse2(input)?;

    let mut new_attrs = Vec::with_capacity(attrs.len());
    let mut group = false;

    for attr in attrs {
        if let Meta::Path(ref path) = attr.meta {
            if path.is_ident("plugin_group") {
                group = true;
                continue;
            }
        }

        new_attrs.push(attr);
    }

    let add_plugin = match group {
        false => quote! { builder = builder.add(plugin) },
        true => quote! { builder = builder.add_group(plugin) },
    };

    let base = quote! {
        #(#new_attrs)* #vis trait #name {
            #[doc = "Configures the plugin group builder for the types contained in this parameter type."]
            fn build(builder: #bevy_app::PluginGroupBuilder) -> #bevy_app::PluginGroupBuilder;
        }

        impl<#param: #target> #name for #param {
            #[inline]
            fn build(mut builder: #bevy_app::PluginGroupBuilder) -> #bevy_app::PluginGroupBuilder {
                let plugin = { #(#block)* };
                #add_plugin;
                builder
            }
        }
    };

    let unit = quote! {
        impl #name for () {
            #[inline]
            fn build(builder: #bevy_app::PluginGroupBuilder) -> #bevy_app::PluginGroupBuilder {
                builder
            }
        }
    };

    let tuples = (1usize..=15).fold(Vec::with_capacity(15), |mut out, end| {
        let params = (1..=end).fold(Vec::with_capacity(end), |mut params, i| {
            params.push(Ident::new(&format!("T{i}"), name.span()));
            params
        });

        let meta = if end == 1 {
            quote! {
                #[cfg_attr(any(docsrs, docsrs_dep), doc(fake_variadic))]
            }
        } else {
            quote! {
                #[doc(hidden)]
            }
        };

        out.push(quote! {
            #meta
            impl<#(#params: #name,)*> #name for (#(#params,)*) {
                fn build(mut builder: #bevy_app::PluginGroupBuilder) -> #bevy_app::PluginGroupBuilder {
                    #(builder = #params::build(builder);)*
                    builder
                }
            }
        });
        out
    });

    Ok(quote! {
        #base
        #(#tuples)*
        #unit
    })
}
