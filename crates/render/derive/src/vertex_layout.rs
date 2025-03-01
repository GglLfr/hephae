use hephae_macros::Manifest;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_quote, punctuated::Punctuated};

pub fn parse(input: TokenStream) -> syn::Result<TokenStream> {
    let bevy_render = Manifest::resolve_bevy("render", &input)?;
    let hephae_render = Manifest::resolve_hephae("render", &input)?;

    let DeriveInput {
        ident,
        mut generics,
        data,
        ..
    } = syn::parse2(input)?;

    let Data::Struct(data) = data else {
        return Err(syn::Error::new_spanned(ident, "`VertexLayout` only supports `struct`s"))
    };

    let where_clause = generics.make_where_clause();
    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(fields) => &fields.unnamed,
        Fields::Unit => &Punctuated::new(),
    };

    where_clause
        .predicates
        .push(parse_quote! { Self: #hephae_render::bytemuck::NoUninit });

    for field in fields {
        let ty = &field.ty;
        where_clause
            .predicates
            .push(parse_quote! { #ty: #hephae_render::attribute::IsVertexAttribute });
    }

    let attributes = fields.iter().enumerate().fold(Vec::new(), |mut out, (i, field)| {
        let name = &field.ident;
        let ty = &field.ty;
        let index = i as u32;

        out.push(quote! {
            #bevy_render::render_resource::VertexAttribute {
                format: <#ty as #hephae_render::attribute::IsVertexAttribute>::FORMAT,
                offset: ::std::mem::offset_of!(Self, #name) as u64,
                shader_location: #index,
            }
        });
        out
    });

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        unsafe impl #impl_generics #hephae_render::attribute::VertexLayout for #ident #type_generics #where_clause {
            const ATTRIBUTES: &'static [#bevy_render::render_resource::VertexAttribute] = &[
                #(#attributes),*
            ];
        }
    })
}
