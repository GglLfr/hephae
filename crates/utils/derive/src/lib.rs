extern crate proc_macro;

mod plugin_conf;

#[proc_macro]
pub fn plugin_conf(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    plugin_conf::parse(input.into())
        .unwrap_or_else(|e| e.into_compile_error())
        .into()
}
