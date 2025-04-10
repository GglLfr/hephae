use hephae_macros::{
    Manifest,
    proc_macro2::TokenStream,
    quote::{ToTokens, format_ident, quote, quote_spanned},
    syn::{
        self, Data, DeriveInput, Error, Fields, TypePath, parse::ParseStream, parse_quote, punctuated::Punctuated,
        spanned::Spanned,
    },
};

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
        return Err(Error::new_spanned(ident, "`VertexLayout` only supports `struct`s"))
    };

    let where_clause = generics.make_where_clause();
    let fields = match &data.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(fields) => &fields.unnamed,
        Fields::Unit => &Punctuated::new(),
    };

    where_clause
        .predicates
        .push(parse_quote! { Self: #hephae_render::bytemuck::Pod });

    let mut impl_attribs = Vec::new();
    for (i, field) in fields.iter().enumerate() {
        let ty = &field.ty;
        where_clause
            .predicates
            .push(parse_quote! { #ty: #hephae_render::attribute::IsAttribData });

        let mut impl_attrib = None;
        for attr in &field.attrs {
            if attr.path().is_ident("attrib") {
                if impl_attrib.is_some() {
                    return Err(Error::new_spanned(attr, "multiple `#[attrib(...)]` found"))
                } else {
                    impl_attrib = Some(attr.parse_args_with(|input: ParseStream| {
                        let mut ty = input.parse::<TypePath>()?;
                        let Some(last) = ty.path.segments.last_mut() else {
                            return Err(Error::new_spanned(ty, "empty path"))
                        };

                        last.ident = format_ident!("{}Attrib", last.ident);
                        Ok(ty)
                    })?);
                }
            }
        }

        if let Some(attrib) = impl_attrib {
            impl_attribs.push((
                match field.ident {
                    Some(ref id) => id.to_token_stream(),
                    None => quote_spanned! { field.span() => #i },
                },
                &field.ty,
                attrib,
            ));
        }
    }

    let attributes = fields.iter().enumerate().fold(Vec::new(), |mut out, (i, field)| {
        let name = match field.ident {
            Some(ref id) => id.to_token_stream(),
            None => quote_spanned! { field.span() => #i },
        };

        let ty = &field.ty;
        let index = i as u32;

        out.push(quote! {
            #bevy_render::render_resource::VertexAttribute {
                format: <#ty as #hephae_render::attribute::IsAttribData>::FORMAT,
                offset: ::core::mem::offset_of!(Self, #name) as u64,
                shader_location: #index,
            }
        });
        out
    });

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let impl_attribs = impl_attribs.into_iter().map(|(f, field_type, attrib)| {
        quote_spanned! { f.span() =>
            unsafe impl #impl_generics #hephae_render::attribute::HasAttrib::<#attrib> for #ident #type_generics
                #where_clause, #attrib: #hephae_render::attribute::Attrib<Data = #field_type>,
            {
                const OFFSET: usize = ::core::mem::offset_of!(Self, #f);
            }
        }
    });

    Ok(quote! {
        unsafe impl #impl_generics #hephae_render::attribute::VertexLayout for #ident #type_generics #where_clause {
            const ATTRIBUTES: &'static [#bevy_render::render_resource::VertexAttribute] = &[
                #(#attributes),*
            ];
        }

        #(#impl_attribs)*
    })
}
