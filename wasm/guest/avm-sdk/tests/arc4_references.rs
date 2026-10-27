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

mod common;
use avm_sdk::arc4::{Account, Arc4Contract, AssetTransferTxn, PaymentTxn, Transaction};
use avm_sdk::testing::set_host;
use avm_sdk::{contract, Address, Application, Asset, GlobalUint};
use common::CallAvm;

#[contract]
struct References {
    calls: GlobalUint,
}

#[contract]
impl References {
    #[abimethod]
    fn check(
        &self,
        a: Asset,
        pay: PaymentTxn,
        account: Account,
        app: Application,
        transfer: AssetTransferTxn,
    ) {
        assert_eq!(a.id, u64::MAX);
        assert_eq!(pay.group_index(), 1);
        assert_eq!(account.address, Address([7; 32]));
        assert_eq!(app.id, 999);
        assert_eq!(transfer.group_index(), 2);
        self.calls.set(1);
    }
    #[abimethod]
    fn current(&self, account: Account, app: Application, a: Asset) {
        assert_eq!(account.address, Address([42; 32]));
        assert_eq!(app.id, 888);
        assert_eq!(a.id, 777);
        self.calls.set(1);
    }
    #[abimethod]
    fn any(&self, tx: Transaction) {
        assert_eq!(tx.group_index(), 0);
    }

    #[abimethod]
    #[allow(clippy::too_many_arguments)]
    fn packed(
        &self,
        tx: PaymentTxn,
        a: u8,
        b: u8,
        c: u8,
        d: u8,
        e: u8,
        f: u8,
        g: u8,
        h: u8,
        i: u8,
        j: u8,
        k: u8,
        l: u8,
        m: u8,
        n: u8,
        asset: Asset,
        account: Account,
    ) {
        assert_eq!([a, b, c, d, e, f, g, h, i, j, k, l, m, n], [0; 14]);
        assert_eq!(asset.id, 777);
        assert_eq!(account.address, Address([42; 32]));
        assert_eq!(tx.group_index(), 0);
    }
}

#[test]
fn references_convert_to_and_from_native_types() {
    let asset: Asset = 777.into();
    assert_eq!(asset.id, 777);
    assert_eq!(u64::from(asset), 777);

    let application = Application::from(999);
    assert_eq!(application.id, 999);
    assert_eq!(u64::from(application), 999);

    let address = Address::from([7; 32]);
    assert_eq!(address, Address([7; 32]));
    assert_eq!(<[u8; 32]>::from(address), [7; 32]);
    assert_eq!(Address::ZERO, Address([0; 32]));

    let _host = set_host(CallAvm::call(&[]));
    let asset_slot = GlobalUint::<Asset>::new_typed(b"asset");
    asset_slot.set(asset);
    assert_eq!(asset_slot.get(), asset);

    let app_slot = GlobalUint::<Application>::new_typed(b"application");
    app_slot.set(application);
    assert_eq!(app_slot.get(), application);
}

#[test]
fn references_and_transactions_are_distinct_wire_arguments() {
    let method = &References::METHODS[0];
    assert_eq!(
        method.signature,
        "check(asset,pay,account,application,axfer)void"
    );
    let mut avm = CallAvm::call(&[&method.selector, &[1], &[1], &[1]]);
    avm.assets = vec![777, u64::MAX];
    avm.accounts = vec![[7; 32]];
    avm.applications = vec![999];
    avm.group_types = vec![6, 1, 4];
    let host = set_host(avm);
    assert_eq!(program(), 1);
    assert_eq!(host.uint(b"calls"), Some(1));
}

#[test]
fn zero_references_have_arc4_indexing() {
    let mut avm = CallAvm::call(&[&References::METHODS[1].selector, &[0], &[0], &[0]]);
    avm.assets = vec![777];
    let _host = set_host(avm);
    assert_eq!(program(), 1);
}

#[test]
fn bad_reference_indices_and_transaction_types_reject_before_method() {
    for (asset, account, app, types) in [
        (2, 1, 1, vec![1, 4]),
        (1, 2, 1, vec![1, 4]),
        (1, 1, 2, vec![1, 4]),
        (1, 1, 1, vec![]),
        (1, 1, 1, vec![1]),
        (1, 1, 1, vec![4, 1]),
    ] {
        let mut avm = CallAvm::call(&[
            &References::METHODS[0].selector,
            &[asset],
            &[account],
            &[app],
        ]);
        avm.assets = vec![777, u64::MAX];
        avm.accounts = vec![[7; 32]];
        avm.applications = vec![999];
        avm.group_types = types;
        let host = set_host(avm);
        assert_eq!(program(), 0);
        assert_eq!(host.uint(b"calls"), None);
    }
    for bytes in [&[][..], &[0, 0][..], &[255][..]] {
        let mut avm = CallAvm::call(&[&References::METHODS[1].selector, &[0], &[0], bytes]);
        avm.assets = vec![777];
        let _host = set_host(avm);
        assert_eq!(program(), 0);
    }
}

#[test]
fn generic_transaction_and_packed_references() {
    let mut avm = CallAvm::call(&[&References::METHODS[2].selector]);
    avm.group_types = vec![5];
    let host = set_host(avm);
    assert_eq!(program(), 1);
    drop(host);
    let mut args = vec![References::METHODS[3].selector.to_vec()];
    args.extend((0..14).map(|_| vec![0]));
    args.push(vec![0, 0]); // packed (asset,account) indices
    let slices: Vec<_> = args.iter().map(Vec::as_slice).collect();
    let mut avm = CallAvm::call(&slices);
    avm.group_types = vec![1];
    avm.assets = vec![777];
    let _host = set_host(avm);
    assert_eq!(program(), 1);
}
