#![recursion_limit = "128"]
extern crate proc_macro;

use syn::{Data, DeriveInput, Error, parse_macro_input};
use quote::quote;

#[proc_macro_derive(Columns)]
pub fn columns_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: syn::DeriveInput = parse_macro_input!(input as DeriveInput);
    let ident = &ast.ident;
    let (impl_generics, ty_generics, where_clause) = ast.generics.split_for_impl();
    let data = match ast.data {
        Data::Struct(ref data) => data,
        _ => {
            let e = Error::new_spanned(&ast, "trait `Columns` can only be implemented for structs");
            return proc_macro::TokenStream::from(e.to_compile_error());
        }
    };

    let sizes = data.fields.iter().map(|field| &field.ty);
    let aligns = data.fields.iter().map(|field| &field.ty);

    let pointers = data.fields.iter().count();
    let dangling = data.fields.iter().map(|field| &field.ty);

    let field = data.fields.iter().map(|field| &field.ident);
    let ty = data.fields.iter().map(|field| &field.ty);
    let index = 0..pointers;

    let expanded = quote! {
        unsafe impl #impl_generics ::soak::Columns for #ident #ty_generics #where_clause {
            const SIZES: &'static [usize] = &[
                #(::core::mem::size_of::<#sizes>(),)*
            ];

            const ALIGNS: &'static [usize] = &[
                #(::core::mem::align_of::<#aligns>(),)*
            ];

            type Pointers = [::core::ptr::NonNull<u8>; #pointers];

            fn dangling() -> Self::Pointers {
                [ #(unsafe { ::core::ptr::NonNull::new_unchecked(
                    ::core::mem::align_of::<#dangling>() as *mut u8
                ) },)* ]
            }
        }

        #[allow(non_upper_case_globals)]
        impl #impl_generics #ident #ty_generics #where_clause {
            #(const #field: ::soak::Field<Self, #ty> = unsafe { ::soak::Field::new(#index) };)*
        }
    };

    proc_macro::TokenStream::from(expanded)
}
