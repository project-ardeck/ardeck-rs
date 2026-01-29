use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(Store)]
pub fn store_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let name = &ast.ident;

    let quote = quote! {
        // use ardeck::store::StoreTrait;
        // impl Store Trait for #name {}
    };

    TokenStream::from(quote)
}
