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

// A payment before and an asset transfer after the app exercise group indexing
// independently of the current index. Three app calls provide a normal 2100-unit
// pool; no protocol limits are raised for these host integration tests.
func wasmFieldParams(field, groupIndex, expected uint64) *EvalParams {
	addr := basics.AppIndex(888).Address()
	pay := transactions.SignedTxn{Txn: transactions.Transaction{
		Type:             protocol.PaymentTx,
		PaymentTxnFields: transactions.PaymentTxnFields{Receiver: addr, Amount: basics.MicroAlgos{Raw: 300_000}},
	}}
	app := transactions.SignedTxn{Txn: transactions.Transaction{
		Type:   protocol.ApplicationCallTx,
		Header: transactions.Header{Sender: addr, Fee: basics.MicroAlgos{Raw: field}, FirstValid: basics.Round(groupIndex), LastValid: basics.Round(expected)},
	}}
	axfer := transactions.SignedTxn{Txn: transactions.Transaction{
		Type:                   protocol.AssetTransferTx,
		AssetTransferTxnFields: transactions.AssetTransferTxnFields{XferAsset: 777, AssetAmount: 123, AssetReceiver: addr},
	}}
	return wasmAppParams(pay, app, axfer, app, app)
}

// TestWasmGroupAndGlobalFields checks that grouped and context reads expose
// their values in application mode and read as zero/empty when the field is
// invalid or unavailable. Uint fixtures compare against the expected value, so
// a zero read approves when the expectation is zero. Byte fixtures require a
// full-length value, so an empty read rejects.
func TestWasmGroupAndGlobalFields(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		fixture                string
		field, index, expected uint64
		pass                   bool
	}{
		{"gtxn_uint.wasm", uint64(Amount), 0, 300_000, true},
		{"gtxn_uint.wasm", uint64(XferAsset), 2, 777, true},
		{"gtxn_uint.wasm", uint64(AssetAmount), 2, 123, true},
		{"gtxn_uint.wasm", uint64(GroupIndex), 1, 1, true},
		{"gtxn_uint.wasm", uint64(GroupIndex), 4, 4, true},
		{"gtxn_uint.wasm", uint64(Amount), 5, 0, true},
		{"gtxn_uint.wasm", uint64(Sender), 0, 0, true},
		{"gtxn_uint.wasm", uint64(Assets), 0, 0, true},
		{"gtxn_uint.wasm", uint64(CreatedAssetID), 2, 0, true},
		{"gtxn_uint.wasm", uint64(CreatedAssetID), 1, 0, true},
		{"gtxn_uint.wasm", 999, 0, 0, true},
		{"gtxn_bytes.wasm", uint64(Receiver), 0, 0, true},
		{"gtxn_bytes.wasm", uint64(AssetReceiver), 2, 0, true},
		{"gtxn_bytes.wasm", uint64(Sender), 1, 0, true},
		{"gtxn_bytes.wasm", uint64(Amount), 0, 0, false},
		{"gtxn_bytes.wasm", uint64(Receiver), 5, 0, false},
		{"global_uint.wasm", uint64(GroupSize), 0, 5, true},
		{"global_uint.wasm", uint64(CurrentApplicationID), 0, 888, true},
		{"global_uint.wasm", uint64(CurrentApplicationAddress), 0, 0, true},
		{"global_uint.wasm", uint64(OpcodeBudget), 0, 0, true},
		{"global_uint.wasm", 999, 0, 0, true},
		{"global_bytes.wasm", uint64(CurrentApplicationAddress), 0, 0, true},
		{"global_bytes.wasm", uint64(GroupSize), 0, 0, false},
	} {
		t.Run(fmt.Sprintf("%s/%d/%d", tc.fixture, tc.field, tc.index), func(t *testing.T) {
			pass, cx, err := EvalContract(loadWasm(t, tc.fixture), 1, 888, wasmFieldParams(tc.field, tc.index, tc.expected))
			require.NoError(t, err)
			require.Equal(t, tc.pass, pass)
			require.Positive(t, cx.Cost())
		})
	}
}

func TestWasmGlobalFieldModesAndVersions(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		field   GlobalField
		version uint64
		mode    RunMode
		err     string
	}{
		{GroupSize, 1, ModeSig, ""},
		{CurrentApplicationID, 1, ModeApp, "invalid global field"},
		{CurrentApplicationID, 2, ModeSig, "not allowed in current mode"},
		{CurrentApplicationAddress, 4, ModeApp, "invalid global field"},
		{CurrentApplicationAddress, 5, ModeSig, "not allowed in current mode"},
	} {
		cx := EvalContext{EvalParams: wasmFieldParams(0, 0, 0), version: tc.version, runMode: tc.mode}
		_, err := cx.wasmReadField(2, 0, uint32(tc.field), 0, false)
		if tc.err == "" {
			require.NoError(t, err)
		} else {
			require.ErrorContains(t, err, tc.err)
		}
	}
}

func TestWasmGroupAndGlobalSignatureFields(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		fixture                string
		field, index, expected uint64
		pass                   bool
	}{
		{"gtxn_uint.wasm", uint64(Amount), 0, 300_000, true},
		{"gtxn_uint.wasm", uint64(XferAsset), 2, 777, true},
		{"gtxn_bytes.wasm", uint64(AssetReceiver), 2, 0, true},
		{"global_uint.wasm", uint64(GroupSize), 0, 5, true},
		// Mode-restricted fields read as zero/empty in a LogicSig rather than
		// trapping. Uint fixtures compare against 0, byte fixtures require a
		// full-length value.
		{"global_uint.wasm", uint64(CurrentApplicationID), 0, 0, true},
		{"global_bytes.wasm", uint64(CurrentApplicationAddress), 0, 0, false},
		{"gtxn_uint.wasm", uint64(CreatedAssetID), 0, 0, true},
	} {
		t.Run(fmt.Sprintf("%s/%d", tc.fixture, tc.field), func(t *testing.T) {
			appParams := wasmFieldParams(tc.field, tc.index, tc.expected)
			txns := make([]transactions.SignedTxn, len(appParams.TxnGroup))
			for i := range txns {
				txns[i] = appParams.TxnGroup[i].SignedTxn
			}
			pass, _, err := EvalSignatureProgram(loadWasm(t, tc.fixture), nil, 1, optSigParams(wasmProto, txns...))
			require.NoError(t, err)
			require.Equal(t, tc.pass, pass)
		})
	}
}

func TestWasmGroupPastEffectsAndFullWidthUint(t *testing.T) {
	partitiontest.PartitionTest(t)
	ep := wasmFieldParams(uint64(CreatedAssetID), 0, 1234)
	ep.TxnGroup[0].ApplyData.ConfigAsset = 1234
	pass, _, err := EvalContract(loadWasm(t, "gtxn_uint.wasm"), 1, 888, ep)
	require.NoError(t, err)
	require.True(t, pass)

	// WASM i64 is signed at the ABI, but transaction fields retain all 64 bits.
	const maxUint = ^uint64(0)
	ep = wasmFieldParams(uint64(Amount), 0, maxUint)
	ep.TxnGroup[0].Txn.Amount.Raw = maxUint
	pass, _, err = EvalContract(loadWasm(t, "gtxn_uint.wasm"), 1, 888, ep)
	require.NoError(t, err)
	require.True(t, pass)
}
