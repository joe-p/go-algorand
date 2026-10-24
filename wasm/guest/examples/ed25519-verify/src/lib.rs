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

//! An example WASM LogicSig: verify an ed25519 signature over argument 0 using
//! the `ed25519-dalek` API, with argument 1 the 64-byte signature and argument
//! 2 the 32-byte public key.
//!
//! It uses `ed25519-dalek` exactly as an off-chain program would. On wasm32 the
//! crate is the vendored fork (see `[patch.crates-io]` in the workspace root),
//! whose `VerifyingKey::verify` calls the `env.ed25519verify_bare` host function
//! instead of doing curve arithmetic in the guest, so on-chain verification
//! matches the AVM and bills the opcode's fuel.

#![cfg_attr(target_arch = "wasm32", no_std)]

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use wasm_guest::{arg_into, entry};

entry!(|| {
    let mut data = [0u8; 4096];
    let mut sig_bytes = [0u8; 64];
    let mut pk_bytes = [0u8; 32];

    // Read every argument we carry: the node requires it, and a read also marks
    // the argument as used.
    let data_len = match arg_into(0, &mut data) {
        Ok(data) => data.len(),
        Err(_) => return false,
    };
    if arg_into(1, &mut sig_bytes).map(<[u8]>::len) != Ok(64)
        || arg_into(2, &mut pk_bytes).map(<[u8]>::len) != Ok(32)
    {
        return false;
    }

    let Ok(key) = VerifyingKey::from_bytes(&pk_bytes) else {
        return false;
    };
    let signature = Signature::from_bytes(&sig_bytes);

    // On wasm32 this is routed to `env.ed25519verify_bare`.
    key.verify(&data[..data_len], &signature).is_ok()
});

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
