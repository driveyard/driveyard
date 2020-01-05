extern crate proc_macro;

use syn::{Data, DeriveInput, Error, Fields, parse_macro_input};
use quote::quote;

#[proc_macro_derive(Fields)]
pub fn fields_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: syn::DeriveInput = parse_macro_input!(input as DeriveInput);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let data = match ast.data {
        Data::Struct(ref data) => data,
        _ => {
            let e = Error::new_spanned(&ast, "trait `Fields` can only be implemented for structs");
            return proc_macro::TokenStream::from(e.to_compile_error());
        }
    };
    match data.fields {
        Fields::Named(_) => {},
        _ => {
            let e = Error::new_spanned(&ast, "trait `Fields` can only be implemented for named fields");
            return proc_macro::TokenStream::from(e.to_compile_error());
        }
    }

    let offsets = data.fields.iter().map(|field| &field.ident);
    let sizes = data.fields.iter().map(|field| &field.ty);
    let aligns = data.fields.iter().map(|field| &field.ty);

    let vis = data.fields.iter().map(|field| &field.vis);
    let field = data.fields.iter().map(|field| &field.ident);
    let ty = data.fields.iter().map(|field| &field.ty);
    let index = 0..data.fields.iter().count();

    let expanded = quote! {
        unsafe impl #impl_generics ::dioptre::Fields for #ident #ty_generics #where_clause {
            const OFFSETS: &'static [fn(*mut u8) -> usize] = &[
                #(|object| unsafe {
                    let #ident { #offsets: ref field, .. } = *(object as *mut Self);
                    let offset = (field as *const _ as usize) - (object as *const _ as usize);
                    offset
                },)*
            ];

            const SIZES: &'static [usize] = &[
                #(::core::mem::size_of::<#sizes>(),)*
            ];

            const ALIGNS: &'static [usize] = &[
                #(::core::mem::align_of::<#aligns>(),)*
            ];
        }

        #[allow(non_upper_case_globals)]
        impl #impl_generics #ident #ty_generics #where_clause {
            #(#vis const #field: ::dioptre::Field<Self, #ty> = unsafe {
                ::dioptre::Field::new(#index)
            };)*
        }
    };

    proc_macro::TokenStream::from(expanded)
}
