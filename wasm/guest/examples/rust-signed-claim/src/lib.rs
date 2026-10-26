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

//! A representative compiler-generated WASM application program.
//!
//! This is a signature-authorized claim (a bearer voucher): argument 0 is a
//! 64-byte ed25519 signature over argument 1, the 8-byte big-endian claim
//! amount. The issuer public key is compiled into the program. Each sender may
//! claim once, and the cumulative total is capped.
//!
//! It uses `ed25519-dalek` exactly as an off-chain program would. On wasm32 the
//! vendored fork's `verify` calls `env.ed25519verify_bare`, so on-chain
//! verification matches the AVM and is billed at the host opcode's fuel.
//!
//! The issuer key is derived from the fixed test seed `[0x42; 32]`. The e2e
//! tests derive the same key and signature from that seed; the signature for
//! amount 1000 is also embedded in the `goal` reproduction script, because
//! `goal` cannot sign arbitrary bytes. The point of this fixture is the size
//! and budget of realistic guest code, not the production authorization model.

#![cfg_attr(target_arch = "wasm32", no_std)]

use avm_sdk::{entry, txn::app_arg_into};
use avm_sys::{app_global_get_uint, app_global_put_uint, txn_bytes, txn_uint, TxnField};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// Issuer public key derived from the seed `[0x42; 32]`.
const ISSUER: [u8; 32] = [
    0x21, 0x52, 0xf8, 0xd1, 0x9b, 0x79, 0x1d, 0x24, 0x45, 0x32, 0x42, 0xe1, 0x5f, 0x2e, 0xab, 0x6c,
    0xb7, 0xcf, 0xfa, 0x7b, 0x6a, 0x5e, 0xd3, 0x00, 0x97, 0x96, 0x0e, 0x06, 0x98, 0x81, 0xdb, 0x12,
];

/// Largest cumulative claim total. A real contract would use a
/// consensus-visible limit instead of a compiled-in one.
const MAX_TOTAL: u64 = 1_000_000;

entry!(|| {
    // Creation executes the approval program as well. Deploying must not
    // consume the claim, so a create (ApplicationID 0) simply approves.
    if txn_uint(TxnField::ApplicationID) == 0 {
        return true;
    }

    let mut sig_bytes = [0u8; 64];
    if app_arg_into(0, &mut sig_bytes).map(<[u8]>::len) != Ok(64) {
        return false;
    }

    let mut amount_bytes = [0u8; 8];
    if app_arg_into(1, &mut amount_bytes).map(<[u8]>::len) != Ok(8) {
        return false;
    }
    let amount = u64::from_be_bytes(amount_bytes);
    if amount == 0 {
        return false;
    }

    let Ok(key) = VerifyingKey::from_bytes(&ISSUER) else {
        return false;
    };
    let signature = Signature::from_bytes(&sig_bytes);
    if key.verify(&amount_bytes, &signature).is_err() {
        return false;
    }

    let mut sender = [0u8; 32];
    if txn_bytes(TxnField::Sender, &mut sender).map(|n| n == 32) != Ok(true) {
        return false;
    }
    if app_global_get_uint(&sender) != 0 {
        return false;
    }

    let Some(total) = app_global_get_uint(b"total").checked_add(amount) else {
        return false;
    };
    if total > MAX_TOTAL {
        return false;
    }

    app_global_put_uint(b"total", total);
    app_global_put_uint(&sender, amount);
    true
});

#[cfg(target_arch = "wasm32")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}
