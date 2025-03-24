use std::iter::repeat_n;

use hephae_macros::{
    Manifest,
    proc_macro2::{Ident, TokenStream},
    quote::{ToTokens, quote, quote_spanned},
    syn,
    syn::{
        Attribute, ConstParam, GenericParam, Generics, ImplItemFn, LifetimeParam, Meta, Token, TypeParam, Visibility,
        parse::{Parse, ParseStream},
        spanned::Spanned,
    },
};

struct Syntax {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    generics: Generics,
    impls: Vec<ImplItemFn>,
}

impl Parse for Syntax {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;

        let vis = input.parse()?;
        input.parse::<Token![struct]>()?;
        let name = input.parse()?;
        let generics = input.parse()?;
        input.parse::<Token![;]>()?;

        let mut impls = Vec::new();
        while !input.is_empty() {
            impls.push(input.parse()?)
        }

        Ok(Self {
            attrs,
            vis,
            name,
            generics,
            impls,
        })
    }
}

pub fn parse(input: TokenStream) -> syn::Result<TokenStream> {
    let bevy_app = Manifest::resolve_bevy("app", &input)?;
    let Syntax {
        attrs,
        vis,
        name,
        generics,
        impls,
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

    let (phantom, args) = generics
        .params
        .iter()
        .fold((Vec::new(), Vec::new()), |(mut phantom, mut args), param| {
            match param {
                GenericParam::Lifetime(LifetimeParam { lifetime, .. }) => {
                    phantom.push(quote_spanned!(param.span()=> fn() -> #lifetime ()))
                }
                GenericParam::Type(TypeParam { ident, .. }) => {
                    phantom.push(quote_spanned!(param.span()=> fn() -> #ident ));
                    args.push(quote_spanned!(param.span()=> ::core::any::type_name::<#ident>()))
                }
                GenericParam::Const(ConstParam { ident, .. }) => args.push(ident.into_token_stream()),
            }
            (phantom, args)
        });

    let phantom = match &*phantom {
        [] => quote!(::core::marker::PhantomData::<()>),
        [data] => quote!(::core::marker::PhantomData::<#data>),
        data => quote!(::core::marker::PhantomData::<(#(#data),*)>),
    };

    let fmt = match &*args {
        [] => quote!(::core::stringify!(#name)),
        args => {
            let braces = repeat_n(quote!({}), args.len());
            quote!(::core::stringify!(#name<#(#braces) *>))
        }
    };

    let (params, where_clause) = (&generics.params, &generics.where_clause);
    let this = quote!(#(#new_attrs)* #vis struct #name <#params> (#phantom) #where_clause; );

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let impl_plugin = if group {
        quote! {
            impl #impl_generics #bevy_app::PluginGroup for #name #type_generics #where_clause {
                #(#impls)*
            }
        }
    } else {
        quote! {
            impl #impl_generics #bevy_app::Plugin for #name #type_generics #where_clause {
                #(#impls)*
            }
        }
    };

    Ok(quote! {
        #this

        impl #impl_generics ::core::fmt::Debug for #name #type_generics #where_clause {
            #[inline]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, #fmt, #(#args),*)
            }
        }

        impl #impl_generics ::core::clone::Clone for #name #type_generics #where_clause {
            #[inline]
            fn clone(&self) -> Self {
                *self
            }
        }

        impl #impl_generics ::core::marker::Copy for #name #type_generics #where_clause {}

        impl #impl_generics ::core::default::Default for #name #type_generics #where_clause {
            #[inline]
            fn default() -> Self {
                Self(::core::marker::PhantomData)
            }
        }

        #impl_plugin
    })
}
