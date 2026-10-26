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

use std::collections::BTreeMap;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use sha2::{Digest, Sha512_256};
use syn::{
    parse_macro_input, Expr, ExprLit, Fields, FnArg, GenericArgument, ImplItem, ImplItemFn, Item,
    ItemFn, ItemImpl, ItemStruct, Lit, LitByteStr, Meta, Pat, PathArguments, ReturnType, Signature,
    Type,
};

/// The AVM's maximum global-state key length.
const MAX_KEY_LEN: usize = 64;

/// Parses the optional `alloc` argument shared by both attributes.
fn parse_alloc(attr: TokenStream, name: &str) -> Result<bool, TokenStream> {
    if attr.is_empty() {
        return Ok(false);
    }
    match syn::parse::<syn::Ident>(attr) {
        Ok(ident) if ident == "alloc" => Ok(true),
        Ok(ident) => Err(syn::Error::new_spanned(
            ident,
            format!("unsupported #[{name}] argument; expected `alloc`"),
        )
        .to_compile_error()
        .into()),
        Err(err) => Err(err.to_compile_error().into()),
    }
}

/// The `program` export, plus the allocator when requested.
fn program_export(install_alloc: bool, result: TokenStream2) -> TokenStream2 {
    let allocator = if install_alloc {
        quote! {
            #[cfg(target_arch = "wasm32")]
            #[global_allocator]
            static AVM_SDK_ALLOCATOR: ::avm_sdk::BumpAllocator = ::avm_sdk::BumpAllocator::new();
        }
    } else {
        quote! {}
    };
    quote! {
        #allocator

        #[no_mangle]
        pub extern "C" fn program() -> i32 {
            let approve: bool = ::avm_sdk::Approve::approve(#result);
            if approve { 1 } else { 0 }
        }
    }
}

/// Defines the module's `program` export from an approvable function.
///
/// The function takes no parameters; it reaches the host through the SDK's
/// helpers, which use the concrete host on wasm and the installed mock in native
/// tests:
///
/// ```ignore
/// #[avm_sdk::program_entry]
/// fn approve() -> Result<(), avm_sdk::Error> {
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
/// fn approve() -> Result<(), avm_sdk::Error> {
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
    let install_alloc = match parse_alloc(attr, "program_entry") {
        Ok(install_alloc) => install_alloc,
        Err(err) => return err,
    };

    let input_fn = parse_macro_input!(item as ItemFn);

    if !input_fn.sig.inputs.is_empty() {
        return syn::Error::new_spanned(
            &input_fn.sig.inputs,
            "#[program_entry] function must take no parameters; use the avm_sdk helpers to reach the host",
        )
        .to_compile_error()
        .into();
    }

    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_sig = &input_fn.sig;
    let fn_block = &input_fn.block;
    let fn_attrs = &input_fn.attrs;
    let export = program_export(install_alloc, quote! { #fn_name() });

    let expanded = quote! {
        #(#fn_attrs)*
        #fn_vis #fn_sig #fn_block

        #export
    };

    TokenStream::from(expanded)
}

/// Declares a contract. It goes on two items: the state struct and the `impl`
/// block holding the program logic.
///
/// On the **struct**, each field's state key is its name; `#[key = "..."]`
/// overrides it. Fields must implement `avm_sdk::StateField` (`GlobalUint`,
/// `GlobalBytes`). The macro generates `pub fn new() -> Self`, building every
/// field from its key, and `avm_sdk::Contract`, whose `GLOBAL_UINTS` /
/// `GLOBAL_BYTES` are the global-state schema the application needs at
/// creation.
///
/// On the **impl**, the macro generates the module's `program` export. Without
/// `#[abimethod]`s it calls `Self::new().approve()`, and the block must define
/// `approve(&self)` returning `bool` or `Result<(), E>`. With them it routes
/// ARC-4 calls (see below). The macro marks `approve` and every `#[abimethod]`
/// `#[inline(always)]`: once inlined into the function that builds the
/// contract, every state key is a constant, so the struct costs nothing over
/// free functions. Each ABI method gets its own entry function, because wasmi
/// charges fuel to compile a function the first time it runs; a call compiles
/// only the method it invokes.
///
/// ```ignore
/// #[avm_sdk::contract]
/// pub struct Counter {
///     count: GlobalUint,
///     #[key = "o"]
///     owner: GlobalBytes,
/// }
///
/// #[avm_sdk::contract]
/// impl Counter {
///     pub fn approve(&self) -> Result<(), avm_sdk::Error> {
///         self.count.set(self.count.get() + 1);
///         Ok(())
///     }
/// }
/// ```
///
/// # ARC-4 methods
///
/// Mark methods `#[abimethod]` to route ARC-4 calls to them. Application
/// argument 0 is the method selector (the first 4 bytes of SHA-512/256 of the
/// signature, e.g. `increment(uint64)void`), and each parameter is one
/// ABI-encoded argument after it. The router:
///
/// - sends a bare call (no arguments) to `approve` if the impl defines one, and
///   otherwise approves only a `NoOp` create;
/// - requires method calls to be `NoOp`, to match a selector, and to carry
///   exactly one argument per parameter;
/// - runs a method only on an existing application, or only at creation with
///   `#[abimethod(create)]`;
/// - rejects when an argument does not decode, and otherwise approves according
///   to the method's result.
///
/// `#[abimethod(name = "...")]` sets the ABI name. Parameters use the types in
/// `avm_sdk::arc4`, including dynamic arrays and tuples; parameters past the
/// 15th are decoded from the tuple ARC-4 packs them into. The type-to-ABI-name
/// mapping is syntactic (the selector is computed before types resolve), so a
/// type alias is not seen through, except `U256` / `U512`. Methods must return
/// `()` or `Result<(), E>` until the host has `log`. The macro also implements
/// `avm_sdk::arc4::Arc4Contract`, the method metadata for tooling.
///
/// ```ignore
/// #[avm_sdk::contract]
/// impl Counter {
///     #[abimethod]
///     pub fn increment(&self, delta: u64) -> Result<(), avm_sdk::Error> {
///         self.count.set(self.count.get() + delta);
///         Ok(())
///     }
/// }
/// ```
///
/// `#[contract(alloc)]` on the impl installs the SDK bump allocator, as with
/// [`macro@program_entry`]. Only one `#[contract]` impl or `#[program_entry]`
/// may appear in a crate: each defines the single `program` export.
#[proc_macro_attribute]
pub fn contract(attr: TokenStream, item: TokenStream) -> TokenStream {
    let result = match syn::parse::<Item>(item) {
        Ok(Item::Struct(mut input)) => {
            if attr.is_empty() {
                expand_contract_struct(&mut input)
            } else {
                Err(syn::Error::new_spanned(
                    TokenStream2::from(attr),
                    "#[contract] on a struct takes no arguments; put `alloc` on the #[contract] impl",
                ))
            }
        }
        Ok(Item::Impl(mut input)) => match parse_alloc(attr, "contract") {
            Ok(install_alloc) => expand_contract_impl(&mut input, install_alloc),
            Err(err) => return err,
        },
        Ok(other) => Err(syn::Error::new_spanned(
            other,
            "#[contract] applies to a struct or an inherent impl block",
        )),
        Err(err) => Err(err),
    };
    match result {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_contract_impl(input: &mut ItemImpl, install_alloc: bool) -> syn::Result<TokenStream2> {
    if let Some((_, path, _)) = &input.trait_ {
        return Err(syn::Error::new_spanned(
            path,
            "#[contract] applies to an inherent impl, not a trait impl",
        ));
    }
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[contract] impl cannot be generic",
        ));
    }

    let mut methods = Vec::new();
    let mut has_approve = false;
    for item in input.items.iter_mut() {
        let ImplItem::Fn(method) = item else {
            continue;
        };
        if let Some(options) = take_abimethod_attr(&mut method.attrs)? {
            methods.push(abi_method(method, options)?);
            force_inline(method);
        } else if method.sig.ident == "approve" {
            if !takes_only_ref_self(&method.sig) {
                return Err(syn::Error::new_spanned(
                    &method.sig,
                    "`approve` must take only `&self`",
                ));
            }
            has_approve = true;
            force_inline(method);
        }
    }

    let self_ty = &input.self_ty;
    let result = if methods.is_empty() {
        if !has_approve {
            return Err(syn::Error::new_spanned(
                &input.self_ty,
                "#[contract] impl must define `fn approve(&self)` or at least one #[abimethod]",
            ));
        }
        quote! { <#self_ty>::new().approve() }
    } else {
        check_unique(&methods)?;
        let route = arc4_route(self_ty, &methods, has_approve);
        quote! {{
            #route
            arc4_route()
        }}
    };
    let export = program_export(
        install_alloc,
        quote! {{
            fn assert_contract<T: ::avm_sdk::Contract>() {}
            assert_contract::<#self_ty>();
            #result
        }},
    );
    let metadata = if methods.is_empty() {
        quote! {}
    } else {
        arc4_metadata(self_ty, &methods)
    };
    Ok(quote! {
        #input

        #metadata

        #export
    })
}

/// Replaces any `#[inline]` on `method` with `#[inline(always)]`. Once inlined
/// into `program`, the contract's state keys are constants.
fn force_inline(method: &mut ImplItemFn) {
    method.attrs.retain(|attr| !attr.path().is_ident("inline"));
    method.attrs.push(syn::parse_quote!(#[inline(always)]));
}

fn takes_only_ref_self(sig: &Signature) -> bool {
    sig.inputs.len() == 1 && takes_ref_self(sig)
}

fn takes_ref_self(sig: &Signature) -> bool {
    matches!(
        sig.inputs.first(),
        Some(FnArg::Receiver(receiver)) if receiver.reference.is_some() && receiver.mutability.is_none()
    )
}

/// ARC-4 passes at most this many parameters as separate application
/// arguments; the rest are packed into a tuple in the last one.
const MAX_SEPARATE_ARGS: usize = 15;

/// The largest tuple `avm_sdk::arc4::Decode` is implemented for, which bounds
/// the packed parameters.
const MAX_TUPLE_LEN: usize = 16;

#[derive(Default)]
struct AbiOptions {
    name: Option<String>,
    create: bool,
}

/// Removes `#[abimethod]` / `#[abimethod(create, name = "...")]`.
fn take_abimethod_attr(attrs: &mut Vec<syn::Attribute>) -> syn::Result<Option<AbiOptions>> {
    let Some(index) = attrs
        .iter()
        .position(|attr| attr.path().is_ident("abimethod"))
    else {
        return Ok(None);
    };
    let attr = attrs.remove(index);
    let mut options = AbiOptions::default();
    if !matches!(attr.meta, Meta::Path(_)) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("create") {
                options.create = true;
                Ok(())
            } else if meta.path.is_ident("name") {
                options.name = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                Ok(())
            } else {
                Err(meta.error(
                    "unsupported #[abimethod] option; expected `create` or `name = \"...\"`",
                ))
            }
        })?;
    }
    if attrs.iter().any(|attr| attr.path().is_ident("abimethod")) {
        return Err(syn::Error::new_spanned(
            attr,
            "duplicate #[abimethod] attribute",
        ));
    }
    Ok(Some(options))
}

struct AbiParam {
    name: String,
    abi: String,
    ty: Type,
}

struct AbiMethod {
    ident: syn::Ident,
    name: String,
    signature: String,
    selector: [u8; 4],
    create: bool,
    params: Vec<AbiParam>,
}

fn abi_method(method: &ImplItemFn, options: AbiOptions) -> syn::Result<AbiMethod> {
    let sig = &method.sig;
    if !takes_ref_self(sig) {
        return Err(syn::Error::new_spanned(
            sig,
            "an #[abimethod] must take `&self` as its first parameter",
        ));
    }
    if !sig.generics.params.is_empty() || sig.asyncness.is_some() {
        return Err(syn::Error::new_spanned(
            sig,
            "an #[abimethod] cannot be generic or async",
        ));
    }
    check_void_return(&sig.output)?;

    let mut params = Vec::new();
    for input in sig.inputs.iter().skip(1) {
        let FnArg::Typed(typed) = input else {
            unreachable!("only the first parameter can be a receiver");
        };
        let abi = abi_name(&typed.ty)?;
        let name = match &*typed.pat {
            Pat::Ident(ident) => ident.ident.to_string(),
            _ => "_".to_string(),
        };
        params.push(AbiParam {
            name,
            abi,
            ty: (*typed.ty).clone(),
        });
    }
    let max_params = MAX_SEPARATE_ARGS - 1 + MAX_TUPLE_LEN;
    if params.len() > max_params {
        return Err(syn::Error::new_spanned(
            &sig.inputs,
            format!("an #[abimethod] takes at most {max_params} parameters"),
        ));
    }

    let name = options.name.unwrap_or_else(|| sig.ident.to_string());
    let types: Vec<&str> = params.iter().map(|p| p.abi.as_str()).collect();
    let signature = format!("{name}({})void", types.join(","));
    let hash = Sha512_256::digest(signature.as_bytes());
    Ok(AbiMethod {
        ident: sig.ident.clone(),
        name,
        signature,
        selector: [hash[0], hash[1], hash[2], hash[3]],
        create: options.create,
        params,
    })
}

/// ARC-4 return values are logged, and the host has no `log` yet, so only
/// `()` and `Result<(), E>` are accepted.
fn check_void_return(output: &ReturnType) -> syn::Result<()> {
    let ReturnType::Type(_, ty) = output else {
        return Ok(());
    };
    let is_unit = |ty: &Type| matches!(ty, Type::Tuple(tuple) if tuple.elems.is_empty());
    let ok = match &**ty {
        ty if is_unit(ty) => true,
        Type::Path(path) => path.path.segments.last().is_some_and(|segment| {
            segment.ident == "Result"
                && matches!(
                    &segment.arguments,
                    PathArguments::AngleBracketed(args)
                        if matches!(args.args.first(), Some(GenericArgument::Type(ok)) if is_unit(ok))
                )
        }),
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            ty,
            "an #[abimethod] must return `()` or `Result<(), E>`: ARC-4 return values are logged, and the host has no `log` function yet",
        ))
    }
}

/// Maps a parameter's Rust type to its ARC-4 type name.
///
/// The mapping is syntactic, because the selector is computed here, before
/// types are resolved; the runtime `Decode` impls must agree with it. Type
/// aliases other than `U256` / `U512` are not seen through.
fn abi_name(ty: &Type) -> syn::Result<String> {
    let unsupported = || {
        syn::Error::new_spanned(
            ty,
            "unsupported #[abimethod] parameter type; see avm_sdk::arc4 for the supported types",
        )
    };
    match ty {
        Type::Paren(paren) => abi_name(&paren.elem),
        Type::Group(group) => abi_name(&group.elem),
        Type::Tuple(tuple) => {
            let elems = tuple
                .elems
                .iter()
                .map(abi_name)
                .collect::<syn::Result<Vec<_>>>()?;
            if elems.len() > MAX_TUPLE_LEN {
                return Err(syn::Error::new_spanned(
                    ty,
                    format!("ARC-4 tuples are supported up to {MAX_TUPLE_LEN} elements"),
                ));
            }
            Ok(format!("({})", elems.join(",")))
        }
        Type::Array(array) => {
            let len = int_literal(&array.len, "array length")?;
            if is_ident(&array.elem, "u8") {
                Ok(format!("byte[{len}]"))
            } else {
                Ok(format!("{}[{len}]", abi_name(&array.elem)?))
            }
        }
        Type::Reference(reference) if reference.mutability.is_none() => match &*reference.elem {
            Type::Slice(slice) if is_ident(&slice.elem, "u8") => Ok("byte[]".to_string()),
            elem if is_ident(elem, "str") => Ok("string".to_string()),
            Type::Slice(slice) => Err(syn::Error::new_spanned(
                slice,
                "use avm_sdk::arc4::DynArray<T> for a dynamic array of non-byte elements",
            )),
            _ => Err(unsupported()),
        },
        Type::Path(path) if path.qself.is_none() => {
            let segment = path.path.segments.last().ok_or_else(unsupported)?;
            let generics: Vec<&GenericArgument> = match &segment.arguments {
                PathArguments::None => Vec::new(),
                PathArguments::AngleBracketed(args) => args
                    .args
                    .iter()
                    .filter(|arg| !matches!(arg, GenericArgument::Lifetime(_)))
                    .collect(),
                PathArguments::Parenthesized(_) => return Err(unsupported()),
            };
            let name = segment.ident.to_string();
            match (name.as_str(), generics.as_slice()) {
                ("u8", []) => Ok("uint8".into()),
                ("u16", []) => Ok("uint16".into()),
                ("u32", []) => Ok("uint32".into()),
                ("u64", []) => Ok("uint64".into()),
                ("u128", []) => Ok("uint128".into()),
                ("bool", []) => Ok("bool".into()),
                ("Byte", []) => Ok("byte".into()),
                ("Address", []) => Ok("address".into()),
                ("U256", []) => Ok("uint256".into()),
                ("U512", []) => Ok("uint512".into()),
                ("Uint", [GenericArgument::Const(bytes)]) => {
                    Ok(format!("uint{}", 8 * uint_bytes(bytes)?))
                }
                (
                    "UFixed",
                    [GenericArgument::Const(bytes), GenericArgument::Const(precision_expr)],
                ) => {
                    let bits = 8 * uint_bytes(bytes)?;
                    let precision = int_literal(precision_expr, "ufixed precision")?;
                    if !(1..=160).contains(&precision) {
                        return Err(syn::Error::new_spanned(
                            precision_expr,
                            "ufixed precision must be 1 to 160",
                        ));
                    }
                    Ok(format!("ufixed{bits}x{precision}"))
                }
                ("DynArray", [GenericArgument::Type(elem)]) => Ok(format!("{}[]", abi_name(elem)?)),
                _ => Err(unsupported()),
            }
        }
        _ => Err(unsupported()),
    }
}

/// The byte width of a `Uint<N>`: 1 to 64, so `uint8` to `uint512`.
fn uint_bytes(expr: &Expr) -> syn::Result<usize> {
    let bytes = int_literal(expr, "Uint byte width")?;
    if !(1..=64).contains(&bytes) {
        return Err(syn::Error::new_spanned(
            expr,
            "Uint<N> takes a byte width from 1 to 64 (uint8 to uint512)",
        ));
    }
    Ok(bytes)
}

fn int_literal(expr: &Expr, what: &str) -> syn::Result<usize> {
    let expr = match expr {
        Expr::Block(block) if block.block.stmts.len() == 1 => match &block.block.stmts[0] {
            syn::Stmt::Expr(expr, None) => expr,
            _ => expr,
        },
        _ => expr,
    };
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(int), ..
        }) => int.base10_parse(),
        _ => Err(syn::Error::new_spanned(
            expr,
            format!(
                "the {what} must be an integer literal, because it is part of the method signature"
            ),
        )),
    }
}

fn is_ident(ty: &Type, ident: &str) -> bool {
    matches!(ty, Type::Path(path)
        if path.qself.is_none()
            && path.path.segments.len() == 1
            && path.path.segments[0].ident == ident
            && path.path.segments[0].arguments.is_none())
}

fn check_unique(methods: &[AbiMethod]) -> syn::Result<()> {
    for (i, method) in methods.iter().enumerate() {
        if let Some(other) = methods[..i]
            .iter()
            .find(|other| other.selector == method.selector)
        {
            let reason = if other.signature == method.signature {
                format!("duplicate ARC-4 method `{}`", method.signature)
            } else {
                format!(
                    "ARC-4 selector collision between `{}` and `{}`",
                    other.signature, method.signature
                )
            };
            return Err(syn::Error::new_spanned(&method.ident, reason));
        }
    }
    Ok(())
}

/// The router: bare calls (no arguments) go to `approve`, or approve a plain
/// create when there is none; method calls must be `NoOp`, match a selector,
/// carry exactly the expected arguments, and validate.
///
/// All arguments are read into one buffer up front, so dynamic parameters can
/// borrow from it without allocating. The arm validates every argument against
/// the method's static descriptors with one shared call, so the entry reads
/// them unchecked and infallibly: nothing threads a `Result` through the
/// parameters, and a bad call still rejects by returning 0 rather than
/// trapping.
///
/// Each method gets its own `#[inline(never)]` entry function. wasmi compiles
/// a function the first time it runs and charges fuel per byte for it, so a
/// call should only compile the method it invokes, not every method in the
/// contract. The entry builds the contract itself, so with the method inlined
/// into it the state keys are still constants.
fn arc4_route(self_ty: &Type, methods: &[AbiMethod], has_approve: bool) -> TokenStream2 {
    let bare = if has_approve {
        quote! { ::avm_sdk::Approve::approve(<#self_ty>::new().approve()) }
    } else {
        quote! { app_id == 0 && on_completion == ::avm_sdk::arc4::NO_OP }
    };
    let mut entries = Vec::new();
    let arms: Vec<_> = methods
        .iter()
        .map(|method| {
            let selector = u32::from_le_bytes(method.selector);
            let ident = &method.ident;
            let entry = format_ident!("abi_{}", ident);
            let create = method.create;
            let (separate, packed) = if method.params.len() > MAX_SEPARATE_ARGS {
                method.params.split_at(MAX_SEPARATE_ARGS - 1)
            } else {
                (method.params.as_slice(), &[][..])
            };
            let num_args = (separate.len() + usize::from(!packed.is_empty()) + 1) as u32;

            // The argument types in order: each separate parameter, then the tuple
            // of packed ones.
            let mut types: Vec<Type> = separate.iter().map(|param| param.ty.clone()).collect();
            if !packed.is_empty() {
                let packed_types = packed.iter().map(|param| &param.ty);
                types.push(syn::parse_quote!((#(#packed_types,)*)));
            }
            let descs = types
                .iter()
                .map(|ty| quote! { &<#ty as ::avm_sdk::arc4::Decode>::DESC });

            let mut reads = Vec::new();
            let mut args = Vec::new();
            for (i, param) in separate.iter().enumerate() {
                let arg = format_ident!("arg{i}");
                let ty = &param.ty;
                let index = i + 1;
                reads.push(quote! {
                    let #arg: #ty = unsafe { args.decode(#index) };
                });
                args.push(arg);
            }
            if !packed.is_empty() {
                let offset = separate.len();
                let names: Vec<_> = (0..packed.len())
                    .map(|i| format_ident!("arg{}", offset + i))
                    .collect();
                let ty = types.last();
                let index = MAX_SEPARATE_ARGS;
                reads.push(quote! {
                    let (#(#names,)*): #ty = unsafe { args.decode(#index) };
                });
                args.extend(names);
            }
            entries.push(quote! {
                /// # Safety
                ///
                /// `args` must have validated against this method's descriptors.
                #[inline(never)]
                unsafe fn #entry(args: &::avm_sdk::arc4::Args) -> bool {
                    #(#reads)*
                    ::avm_sdk::Approve::approve(<#self_ty>::new().#ident(#(#args),*))
                }
            });
            quote! {
                #selector => {
                    if (app_id == 0) != #create || num_args != #num_args {
                        return false;
                    }
                    if !args.validate(&[#(#descs),*]) {
                        return false;
                    }
                    // SAFETY: just validated.
                    unsafe { #entry(&args) }
                }
            }
        })
        .collect();
    quote! {
        #(#entries)*

        #[inline(always)]
        fn arc4_route() -> bool {
            let app_id = ::avm_sdk::txn::application_id();
            let on_completion = ::avm_sdk::txn::uint(::avm_sdk::TxnField::OnCompletion);
            let num_args = ::avm_sdk::txn::app_args_count();
            if num_args == 0 {
                return #bare;
            }
            if on_completion != ::avm_sdk::arc4::NO_OP {
                return false;
            }
            let mut args = ::avm_sdk::arc4::Args::new();
            if !args.read(num_args) {
                return false;
            }
            // One load and compare per method, rather than four.
            let selector = match *args.get(0) {
                [b0, b1, b2, b3] => u32::from_le_bytes([b0, b1, b2, b3]),
                _ => return false,
            };
            match selector {
                #(#arms)*
                _ => false,
            }
        }
    }
}

fn arc4_metadata(self_ty: &Type, methods: &[AbiMethod]) -> TokenStream2 {
    let entries = methods.iter().map(|method| {
        let name = &method.name;
        let signature = &method.signature;
        let [b0, b1, b2, b3] = method.selector;
        let create = method.create;
        let args = method.params.iter().map(|param| {
            let name = &param.name;
            let abi = &param.abi;
            quote! { ::avm_sdk::arc4::Arg { name: #name, ty: #abi } }
        });
        quote! {
            ::avm_sdk::arc4::Method {
                name: #name,
                signature: #signature,
                selector: [#b0, #b1, #b2, #b3],
                args: &[#(#args),*],
                create: #create,
            }
        }
    });
    quote! {
        impl ::avm_sdk::arc4::Arc4Contract for #self_ty {
            const METHODS: &'static [::avm_sdk::arc4::Method] = &[#(#entries),*];
        }
    }
}

fn expand_contract_struct(input: &mut ItemStruct) -> syn::Result<TokenStream2> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            "#[contract] struct cannot be generic",
        ));
    }
    let Fields::Named(fields) = &mut input.fields else {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "#[contract] struct must have named fields",
        ));
    };

    let mut seen: BTreeMap<Vec<u8>, syn::Ident> = BTreeMap::new();
    let mut inits = Vec::new();
    let mut types = Vec::new();
    for field in fields.named.iter_mut() {
        let ident = field.ident.clone().expect("named field");
        let key = take_key_attr(&mut field.attrs)?.unwrap_or_else(|| {
            let name = ident.to_string();
            (
                name.trim_start_matches("r#").as_bytes().to_vec(),
                ident.span(),
            )
        });
        let (key, span) = key;
        if key.is_empty() || key.len() > MAX_KEY_LEN {
            return Err(syn::Error::new(
                span,
                format!(
                    "state key must be 1 to {MAX_KEY_LEN} bytes, got {}",
                    key.len()
                ),
            ));
        }
        if let Some(other) = seen.insert(key.clone(), ident.clone()) {
            return Err(syn::Error::new(
                span,
                format!(
                    "state key {:?} is already used by `{other}`",
                    String::from_utf8_lossy(&key)
                ),
            ));
        }
        let ty = &field.ty;
        let lit = LitByteStr::new(&key, span);
        inits.push(quote! { #ident: <#ty as ::avm_sdk::StateField>::from_key(#lit) });
        types.push(ty.clone());
    }

    let name = &input.ident;
    Ok(quote! {
        #input

        impl #name {
            /// Builds the contract's state handles from their keys.
            #[inline]
            pub fn new() -> Self {
                #name { #(#inits),* }
            }
        }

        impl ::avm_sdk::Contract for #name {
            const GLOBAL_UINTS: u32 = 0 #(+ <#types as ::avm_sdk::StateField>::GLOBAL_UINTS)*;
            const GLOBAL_BYTES: u32 = 0 #(+ <#types as ::avm_sdk::StateField>::GLOBAL_BYTES)*;
        }
    })
}

/// Removes a `#[key = "..."]` attribute, returning its bytes and span.
fn take_key_attr(
    attrs: &mut Vec<syn::Attribute>,
) -> syn::Result<Option<(Vec<u8>, proc_macro2::Span)>> {
    let mut key = None;
    let mut err = None;
    attrs.retain(|attr| {
        if !attr.path().is_ident("key") {
            return true;
        }
        let parsed = match &attr.meta {
            Meta::NameValue(nv) => match &nv.value {
                Expr::Lit(ExprLit {
                    lit: Lit::Str(s), ..
                }) => Ok((s.value().into_bytes(), s.span())),
                Expr::Lit(ExprLit {
                    lit: Lit::ByteStr(s),
                    ..
                }) => Ok((s.value(), s.span())),
                other => Err(syn::Error::new_spanned(
                    other,
                    "expected a string or byte-string key",
                )),
            },
            other => Err(syn::Error::new_spanned(
                other,
                "expected `#[key = \"...\"]`",
            )),
        };
        match parsed {
            Ok(value) if key.is_none() => key = Some(value),
            Ok(_) => err = Some(syn::Error::new_spanned(attr, "duplicate #[key] attribute")),
            Err(e) => err = Some(e),
        }
        false
    });
    match err {
        Some(err) => Err(err),
        None => Ok(key),
    }
}
