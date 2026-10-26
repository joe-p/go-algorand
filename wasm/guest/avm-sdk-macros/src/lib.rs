// Copyright (C) 2019-2026 Algorand Foundation Ltd.
// This file is part of go-algorand
//
// go-algorand is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// go-algorand is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with go-algorand.  If not, see <https://www.gnu.org/licenses/>.

//! Proc macros for `avm-sdk`.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, FnArg, ItemFn, PatType, Type};

fn is_active_avm_type(ty: &Type) -> bool {
    match ty {
        Type::Reference(tr) => {
            if let Type::Path(tp) = &*tr.elem {
                tp.path
                    .segments
                    .last()
                    .map(|s| s.ident == "ActiveAvm")
                    .unwrap_or(false)
            } else {
                false
            }
        }
        Type::Path(tp) => tp
            .path
            .segments
            .last()
            .map(|s| s.ident == "ActiveAvm")
            .unwrap_or(false),
        _ => false,
    }
}

/// Defines the module's `program` export from a function taking one
/// [`ActiveAvm`] and returning an approvable value (`bool`, or `Result<(), E>`).
///
/// ```ignore
/// #[avm_sdk::program_entry]
/// fn approve(avm: ActiveAvm) -> Result<(), avm_sdk::Error> {
///     // ...
///     Ok(())
/// }
/// ```
///
/// Add `alloc` to install the SDK bump allocator as the crate's global
/// allocator, enabling `alloc::vec::Vec` and friends:
///
/// ```ignore
/// #[avm_sdk::program_entry(alloc)]
/// fn approve(avm: ActiveAvm) -> Result<(), avm_sdk::Error> {
///     let mut v = alloc::vec::Vec::new();
///     v.push(1u8);
///     Ok(())
/// }
/// ```
///
/// Only one function in a crate may use this attribute: it defines the single
/// `program` export.
///
/// The macro does not generate a panic handler; guests should provide one on
/// `wasm32` (see `avm_sdk::panic_handler`).
#[proc_macro_attribute]
pub fn program_entry(attr: TokenStream, item: TokenStream) -> TokenStream {
    let install_alloc = if attr.is_empty() {
        false
    } else {
        match syn::parse::<syn::Ident>(attr) {
            Ok(ident) if ident == "alloc" => true,
            Ok(ident) => {
                return syn::Error::new_spanned(
                    ident,
                    "unsupported #[program_entry] argument; expected `alloc`",
                )
                .to_compile_error()
                .into();
            }
            Err(err) => return err.to_compile_error().into(),
        }
    };

    let input_fn = parse_macro_input!(item as ItemFn);

    if input_fn.sig.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &input_fn.sig.inputs,
            "#[program_entry] function must take exactly one `ActiveAvm` parameter",
        )
        .to_compile_error()
        .into();
    }

    let first_param = input_fn.sig.inputs.first();
    let is_active_avm = matches!(
        first_param,
        Some(FnArg::Typed(PatType { ty, .. })) if is_active_avm_type(ty)
    );
    if !is_active_avm {
        return syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "#[program_entry] function must have `ActiveAvm` as its parameter type",
        )
        .to_compile_error()
        .into();
    }

    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;

    let allocator = if install_alloc {
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[global_allocator]
            static AVM_SDK_ALLOCATOR: ::avm_sdk::BumpAllocator = ::avm_sdk::BumpAllocator::new();
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig #fn_block

        #allocator

        #[no_mangle]
        pub extern "C" fn program() -> i32 {
            let approve: bool = ::avm_sdk::Approve::approve(#fn_name(::avm_sdk::host()));
            if approve { 1 } else { 0 }
        }
    };

    TokenStream::from(expanded)
}
