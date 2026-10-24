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
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

// wasmStateParams builds ModeApp EvalParams with WASM app programs enabled, an
// app 888 whose global schema is `schema`, and a fresh in-memory ledger.
func wasmStateParams(t testing.TB, schema basics.StateSchema) *EvalParams {
	t.Helper()
	txn := makeSampleTxn()
	txn.Txn.Type = protocol.ApplicationCallTx
	txn.Txn.ApplicationID = 888
	proto := makeTestProto(wasmAppProto)
	ep := NewAppEvalParams(transactions.WrapSignedTxnsWithAD([]transactions.SignedTxn{txn}), proto, &transactions.SpecialAddresses{})
	ep.Trace = &strings.Builder{}
	ledger := NewLedger(nil)
	creator, err := basics.UnmarshalChecksumAddress(testAppCreator)
	require.NoError(t, err)
	ledger.NewApp(creator, 888, basics.AppParams{StateSchemas: basics.StateSchemas{GlobalStateSchema: schema}})
	ep.Ledger = ledger
	ep.SigLedger = ledger
	return ep
}

// TestWasmAppGlobalIncrement checks a uint global read/write round trip through
// the host and the resulting EvalDelta.
func TestWasmAppGlobalIncrement(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := wasmStateParams(t, basics.StateSchema{NumUint: 1})
	delta, err := testAppBytes(t, loadWasm(t, "app_global_inc.wasm"), ep)
	require.NoError(t, err)
	require.Equal(t, basics.ValueDelta{Action: basics.SetUintAction, Uint: 1}, delta.GlobalDelta["counter"])

	// The ledger reflects the new value.
	tv, ok, err := ep.Ledger.(*Ledger).GetGlobal(888, "counter")
	require.NoError(t, err)
	require.True(t, ok)
	require.Equal(t, basics.TealValue{Type: basics.TealUintType, Uint: 1}, tv)
}

// TestWasmAppGlobalBytes checks a byte-string global read/write round trip.
func TestWasmAppGlobalBytes(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := wasmStateParams(t, basics.StateSchema{NumByteSlice: 1})
	delta, err := testAppBytes(t, loadWasm(t, "app_global_bytes.wasm"), ep)
	require.NoError(t, err)
	require.Equal(t, basics.ValueDelta{Action: basics.SetBytesAction, Bytes: "hi"}, delta.GlobalDelta["b"])
}

// TestWasmAppGlobalDel checks that a delete lands in the EvalDelta and that the
// key reads back as absent.
func TestWasmAppGlobalDel(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := wasmStateParams(t, basics.StateSchema{NumUint: 1})
	delta, err := testAppBytes(t, loadWasm(t, "app_global_del.wasm"), ep)
	require.NoError(t, err)
	require.Equal(t, basics.ValueDelta{Action: basics.DeleteAction}, delta.GlobalDelta["d"])
}

// TestWasmAppGlobalTypeMismatch checks that reading a uint value as bytes traps.
func TestWasmAppGlobalTypeMismatch(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := wasmStateParams(t, basics.StateSchema{NumUint: 1})
	testAppBytes(t, loadWasm(t, "app_global_type_mismatch.wasm"), ep, "not a byte string")
}

// TestWasmAppGlobalSignatureModeTraps checks that the global-state host
// functions are unavailable in signature mode, matching the AVM's ModeApp-only
// restriction.
func TestWasmAppGlobalSignatureModeTraps(t *testing.T) {
	partitiontest.PartitionTest(t)

	_, _, err := EvalSignatureProgram(loadWasm(t, "app_global_inc.wasm"), nil, 0, optSigParams(wasmProto))
	require.Error(t, err)
	require.Contains(t, err.Error(), "signature mode")
}
