// SPDX-License-Identifier: Apache-2.0 OR MIT

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn m(args: TokenStream, tokens: TokenStream) -> TokenStream {
    if !args.is_empty() {
        return args;
    }
    tokens
}
