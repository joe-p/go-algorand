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

//go:build !gofuzz && cgo

package logic

import (
	"fmt"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

// Queries execute with three normal app calls' pooled budget, without changing
// protocol limits. Small IDs other than 0 are deliberately avoided.
func wasmAssetParams(asset uint64, exists bool, balance uint64, name []byte, holding bool) *EvalParams {
	addr := basics.AppIndex(888).Address()
	app := transactions.SignedTxn{Txn: transactions.Transaction{
		Type:   protocol.ApplicationCallTx,
		Header: transactions.Header{Sender: addr, Fee: basics.MicroAlgos{Raw: asset}, LastValid: basics.Round(balance)},
		ApplicationCallTxnFields: transactions.ApplicationCallTxnFields{
			ForeignAssets:   []basics.AssetIndex{777, 778, 779},
			ApplicationArgs: [][]byte{addr[:]},
		},
	}}
	if holding {
		app.Txn.FirstValid = 1
	}
	ep := wasmAppParams(app, app, app)
	ledger := ep.Ledger.(*Ledger)
	ledger.NewAccount(addr, 1_000_000)
	if exists {
		ledger.NewAsset(basics.Address{1}, basics.AssetIndex(asset), basics.AssetParams{Total: ^uint64(0), UnitName: string(name)})
	}
	if holding {
		ledger.NewHolding(addr, basics.AssetIndex(asset), balance, false)
	}
	return ep
}

func TestWasmAssetBalance(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		name            string
		exists, holding bool
		amount          uint64
	}{
		{"balance", true, true, 123_456},
		{"zero", true, true, 0},
		{"full-width", true, true, ^uint64(0)},
		{"not-opted-in", true, false, 0},
		{"asset-missing", false, false, 0},
	} {
		t.Run(tc.name, func(t *testing.T) {
			for _, wasm := range []bool{false, true} {
				ep := wasmAssetParams(777, tc.exists, tc.amount, nil, tc.holding)
				var program []byte
				if wasm {
					program = loadWasm(t, "asset_balance.wasm")
				} else {
					// Differential oracle: independently evaluate the AVM opcode,
					// including its exists bit, against the same ledger setup.
					program = testProg(t, `txna ApplicationArgs 0
txn Fee
asset_holding_get AssetBalance
txn FirstValid
==
assert
txn LastValid
==`, ep.Proto.LogicSigVersion).Program
				}
				pass, _, err := EvalContract(program, 0, 888, ep)
				require.NoError(t, err, "wasm=%v", wasm)
				require.True(t, pass)
			}
		})
	}
}

func TestWasmAssetUnitName(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		name   string
		exists bool
		value  []byte
	}{
		{"named", true, []byte("USDC")},
		{"empty", true, nil},
		{"eight-bytes", true, []byte("12345678")},
		{"binary", true, []byte{0, 0xff, 1}},
		{"missing", false, nil},
	} {
		t.Run(tc.name, func(t *testing.T) {
			for _, wasm := range []bool{false, true} {
				expected := uint64(len(tc.value))
				var exists int
				// The AVM pushes uint zero for an absent parameter, despite the
				// assembler's static byte type. Existence is the relevant result.
				check := "pop\nint 1"
				if !tc.exists {
					expected = ^uint64(0)
				} else {
					exists = 1
					check = fmt.Sprintf("byte 0x%x\n==", tc.value)
				}
				ep := wasmAssetParams(777, tc.exists, expected, tc.value, false)
				ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{tc.value}
				program := loadWasm(t, "asset_unit_name.wasm")
				if !wasm {
					program = testProg(t, fmt.Sprintf("txn Fee\nasset_params_get AssetUnitName\nint %d\n==\nassert\n%s", exists, check), ep.Proto.LogicSigVersion).Program
				}
				pass, _, err := EvalContract(program, 0, 888, ep)
				require.NoError(t, err, "wasm=%v", wasm)
				require.True(t, pass)
			}
		})
	}
}

func TestWasmAssetResourceChecks(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, name := range []string{"asset_balance.wasm", "asset_unit_name.wasm"} {
		t.Run(name, func(t *testing.T) {
			ep := wasmAssetParams(999, true, 10, []byte("ASA"), true)
			pass, _, err := EvalContract(loadWasm(t, name), 0, 888, ep)
			require.False(t, pass)
			require.ErrorContains(t, err, "unavailable Asset 999")
		})
	}

	t.Run("foreign-slot", func(t *testing.T) {
		ep := wasmAssetParams(777, true, 10, nil, true)
		ep.TxnGroup[0].Txn.Fee.Raw = 0 // resolves ForeignAssets[0], just like AVM
		pass, _, err := EvalContract(loadWasm(t, "asset_balance.wasm"), 0, 888, ep)
		require.NoError(t, err)
		require.True(t, pass)
	})

	t.Run("available-shared-holding", func(t *testing.T) {
		ep := wasmAssetParams(777, true, 10, nil, true)
		other := basics.Address{42}
		ep.Ledger.(*Ledger).NewAccount(other, 1_000_000)
		ep.Ledger.(*Ledger).NewHolding(other, 777, 10, false)
		ep.TxnGroup[0].Txn.ForeignAssets = nil
		ep.TxnGroup[1].Txn.Accounts = []basics.Address{other}
		ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{other[:]}
		pass, _, err := EvalContract(loadWasm(t, "asset_balance.wasm"), 0, 888, ep)
		require.NoError(t, err)
		require.True(t, pass)
	})

	t.Run("unavailable-account", func(t *testing.T) {
		ep := wasmAssetParams(777, true, 10, nil, true)
		other := basics.Address{42}
		ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{other[:]}
		pass, _, err := EvalContract(loadWasm(t, "asset_balance.wasm"), 0, 888, ep)
		require.False(t, pass)
		require.ErrorContains(t, err, "unavailable Account")
	})

	// Asset and account availability separately do not imply that their holding
	// is available when resources are shared from different group transactions.
	t.Run("unavailable-shared-holding", func(t *testing.T) {
		ep := wasmAssetParams(777, true, 10, nil, true)
		other := basics.Address{42}
		for i := range ep.TxnGroup {
			ep.TxnGroup[i].Txn.ForeignAssets = nil
		}
		ep.TxnGroup[1].Txn.ForeignAssets = []basics.AssetIndex{777}
		ep.TxnGroup[2].Txn.Accounts = []basics.Address{other}
		ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{other[:]}
		pass, _, err := EvalContract(loadWasm(t, "asset_balance.wasm"), 0, 888, ep)
		require.False(t, pass)
		require.ErrorContains(t, err, "unavailable Holding")
	})
}

func TestWasmAssetQueriesSignatureMode(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, name := range []string{"asset_balance.wasm", "asset_unit_name.wasm"} {
		t.Run(name, func(t *testing.T) {
			ep := wasmAssetParams(777, true, 10, []byte("ASA"), true)
			var txns []transactions.SignedTxn
			for _, tx := range ep.TxnGroup {
				txns = append(txns, tx.SignedTxn)
			}
			pass, _, err := EvalSignatureProgram(loadWasm(t, name), nil, 0, optSigParams(wasmProto, txns...))
			require.False(t, pass)
			require.ErrorContains(t, err, "asset queries are not available in signature mode")
		})
	}
}
