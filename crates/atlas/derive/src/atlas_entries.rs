use std::collections::{HashMap, hash_map::Entry};

use hephae_macros::{
    Manifest,
    proc_macro2::TokenStream,
    quote::{ToTokens, format_ident, quote, quote_spanned},
    syn,
    syn::{
        Attribute, Data, DeriveInput, Error, LitInt, Meta, parse::ParseStream, parse_quote_spanned, parse2, spanned::Spanned,
    },
};

pub fn parse(input: TokenStream) -> syn::Result<TokenStream> {
    let bevy_asset = Manifest::resolve_bevy("asset", &input)?;
    let hephae_atlas = Manifest::resolve_hephae("atlas", &input)?;

    let DeriveInput {
        ident,
        mut generics,
        data,
        ..
    } = parse2(input)?;

    let where_clause = generics.make_where_clause();
    let mut atlases = HashMap::new();
    let mut entries = HashMap::new();

    match data {
        Data::Struct(data) => {
            for (i, f) in data.fields.iter().enumerate() {
                let id = match f.ident {
                    Some(ref id) => id.to_token_stream(),
                    None => quote_spanned! { f.span() => #i },
                };

                let mut atlas = None;
                let mut entry = None;

                fn parse_index(attr: &Attribute) -> syn::Result<usize> {
                    match attr.meta {
                        Meta::Path(..) => Ok(0),
                        Meta::List(ref list) => list.parse_args_with(|input: ParseStream| {
                            let index = input.parse::<LitInt>()?.base10_parse()?;
                            if !input.is_empty() {
                                return Err(input.error("unexpected token"))
                            }

                            Ok(index)
                        }),
                        Meta::NameValue(ref meta) => {
                            Err(Error::new_spanned(meta, "expected index in a singular list attribute"))
                        }
                    }
                }

                let ty = &f.ty;
                for attr in &f.attrs {
                    if attr.path().is_ident("atlas") {
                        if atlas.is_some() {
                            return Err(Error::new_spanned(attr, "can't repeat `#[atlas]`"))
                        } else {
                            where_clause.predicates.push(
                                parse_quote_spanned! { ty.span() => for<'__handle > &'__handle #ty: ::core::convert::Into::<#bevy_asset::AssetId::<#hephae_atlas::atlas::Atlas>> },
                            );
                            atlas = Some((id.clone(), parse_index(&attr)?));
                        }
                    } else if attr.path().is_ident("entry") {
                        if entry.is_some() {
                            return Err(Error::new_spanned(attr, "can't repeat `#[entry]`"))
                        } else {
                            where_clause.predicates.push(
                                parse_quote_spanned! { ty.span() => #ty: ::core::convert::AsRef::<::std::path::Path> },
                            );
                            entry = Some((id.clone(), parse_index(&attr)?));
                        }
                    }
                }

                if let Some((id, index)) = atlas {
                    match atlases.entry(index) {
                        Entry::Vacant(e) => {
                            e.insert(id);
                        }
                        Entry::Occupied(e) => {
                            return Err(Error::new(
                                id.span().join(e.get().span()).unwrap_or(f.span()),
                                format!("`#[atlas]` at index {id} is already occupied"),
                            ))
                        }
                    }
                }

                if let Some((id, index)) = entry {
                    entries.entry(index).or_insert(Vec::new()).push((i, id))
                }
            }
        }
        Data::Enum(..) => return Err(Error::new_spanned(ident, "`enum` is unsupported")),
        Data::Union(..) => return Err(Error::new_spanned(ident, "`union` is unsupported")),
    }

    let atlases = atlases.into_iter().fold(HashMap::new(), |mut out, (index, id)| {
        let ident = format_ident!("id{index}");

        out.insert(index, (quote!(let #ident = #bevy_asset::AssetId::from(&self.#id);), ident));
        out
    });

    let mut entries = entries.into_iter().try_fold(Vec::new(), |mut out, (index, fields)| {
        let Some((.., ident)) = atlases.get(&index) else {
            return Err(Error::new(
                fields
                    .iter()
                    .map(|(.., tokens)| tokens.span())
                    .reduce(|a, b| a.join(b).unwrap_or(ident.span()))
                    .unwrap_or(ident.span()),
                format!("no `#[atlas]` found for index {index}"),
            ))
        };

        for (order, f) in fields {
            out.push((
                order,
                quote! { (#ident, ::core::convert::AsRef::<::std::path::Path>::as_ref(&self.#f)) },
            ));
        }

        Ok(out)
    })?;

    entries.sort_unstable_by_key(|&(order, ..)| order);

    let atlases = atlases.into_values().map(|(decl, ..)| decl);
    let entries = entries.into_iter().map(|(.., entry)| entry);

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #hephae_atlas::atlas::AtlasEntries for #ident #type_generics #where_clause {
            fn entries(&self) -> impl ::core::iter::Iterator<Item = (#bevy_asset::AssetId<#hephae_atlas::atlas::Atlas>, &::std::path::Path)> {
                #(#atlases)*
                ::core::iter::IntoIterator::into_iter([#(#entries),*])
            }
        }
    })
}
