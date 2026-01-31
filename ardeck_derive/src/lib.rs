use proc_macro::TokenStream;
use quote::quote;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(Store)]
pub fn store_derive(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as DeriveInput);

    let name = &ast.ident;

    let quote = quote! {
        impl ardeck::store::StoreTrait for #name {}
    };

    TokenStream::from(quote)
}

#[cfg(test)]
mod tests {
    use ardeck::{config::ConfigFile, store::StoreBuilder};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, Clone)]
    struct MyConfig {
        name: String,
        age: u32,
    }

    impl Default for MyConfig {
        fn default() -> Self {
            Self {
                name: "John Doe".into(),
                age: 42,
            }
        }
    }

    impl ConfigFile for MyConfig {
        fn name() -> &'static str {
            "my_config.json"
        }
    }

    #[test]
    fn store_builder() {
        StoreBuilder::default().path("./".into()).init();

        // MyConfig::
    }
}
