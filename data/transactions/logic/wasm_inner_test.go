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
	"encoding/binary"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/ledger/ledgercore"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

func wasmInnerParams() *EvalParams {
	app := transactions.SignedTxn{Txn: transactions.Transaction{
		Type:                     protocol.ApplicationCallTx,
		Header:                   transactions.Header{Sender: basics.Address{42}, Fee: basics.MicroAlgos{Raw: 10_000}, FirstValid: 1, LastValid: 1000},
		ApplicationCallTxnFields: transactions.ApplicationCallTxnFields{ApplicationID: 888, ForeignAssets: []basics.AssetIndex{777, 778}},
	}}
	// Eight normal calls pool budget; no consensus limits are lifted.
	ep := wasmAppParams(app, app, app, app, app, app, app, app)
	l := ep.Ledger.(*Ledger)
	l.NewAccount(basics.AppIndex(888).Address(), 1_000_000)
	l.NewAccount(app.Txn.Sender, 1_000_000)
	l.NewApp(app.Txn.Sender, 888, basics.AppParams{})
	l.NewAsset(app.Txn.Sender, 777, basics.AssetParams{Total: 1000})
	l.NewAsset(app.Txn.Sender, 778, basics.AssetParams{Total: 1000})
	return ep
}

func TestWasmInnerAssetCreation(t *testing.T) {
	partitiontest.PartitionTest(t)
	ep := wasmInnerParams()
	credit := *ep.FeeCredit
	pass, cx, err := EvalContract(loadWasm(t, "inner_asset_create.wasm"), 0, 888, ep)
	require.NoError(t, err)
	require.True(t, pass)
	inners := cx.txn.EvalDelta.InnerTxns
	require.Len(t, inners, 1)
	inner := inners[0]
	addr := basics.AppIndex(888).Address()
	require.Equal(t, protocol.AssetConfigTx, inner.Txn.Type)
	require.Equal(t, addr, inner.Txn.Sender)
	require.Zero(t, inner.Txn.Fee.Raw)
	require.NotZero(t, inner.ApplyData.ConfigAsset)
	params, creator, err := ep.Ledger.AssetParams(inner.ApplyData.ConfigAsset)
	require.NoError(t, err)
	require.Equal(t, addr, creator)
	require.Equal(t, basics.AssetParams{Total: 10_000_000_000, Decimals: 3, UnitName: "dpt", AssetName: "DPT-A-B", Manager: addr, Reserve: addr}, params)
	require.Less(t, ep.FeeCredit.Raw, credit.Raw)
	require.Positive(t, cx.Cost())
}

func TestWasmInnerAssetTransfers(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, amount := range []uint64{0, 42} {
		ep := wasmInnerParams()
		addr := basics.AppIndex(888).Address()
		receiver := addr
		if amount != 0 {
			receiver = ep.TxnGroup[0].Txn.Sender
			ep.Ledger.(*Ledger).NewHolding(addr, 777, 100, false)
		}
		var raw [8]byte
		binary.LittleEndian.PutUint64(raw[:], amount)
		ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{receiver[:], raw[:]}
		pass, cx, err := EvalContract(loadWasm(t, "inner_asset_transfer.wasm"), 0, 888, ep)
		require.NoError(t, err)
		require.True(t, pass)
		require.Len(t, cx.txn.EvalDelta.InnerTxns, 1)
		holding, err := ep.Ledger.AssetHolding(addr, 777)
		require.NoError(t, err)
		if amount == 0 {
			require.Zero(t, holding.Amount)
		} else {
			require.Equal(t, uint64(58), holding.Amount)
		}
	}
}

func TestWasmInnerAssetGroup(t *testing.T) {
	partitiontest.PartitionTest(t)
	ep := wasmInnerParams()
	trace := &wasmInnerTracer{}
	ep.Tracer = trace
	pass, cx, err := EvalContract(loadWasm(t, "inner_asset_group.wasm"), 0, 888, ep)
	require.NoError(t, err)
	require.True(t, pass)
	require.Len(t, cx.txn.EvalDelta.InnerTxns, 2)
	for _, asset := range []basics.AssetIndex{777, 778} {
		holding, err := ep.Ledger.AssetHolding(basics.AppIndex(888).Address(), asset)
		require.NoError(t, err)
		require.Zero(t, holding.Amount)
	}
	require.NotEmpty(t, cx.txn.EvalDelta.InnerTxns[0].Txn.Group)
	require.Equal(t, cx.txn.EvalDelta.InnerTxns[0].Txn.Group, cx.txn.EvalDelta.InnerTxns[1].Txn.Group)
	require.Equal(t, []string{"group begin", "txn begin", "txn end", "txn begin", "txn end", "group end"}, trace.events)
}

type wasmInnerTracer struct {
	NullEvalTracer
	events []string
}

func (t *wasmInnerTracer) BeforeTxnGroup(*EvalParams) { t.events = append(t.events, "group begin") }
func (t *wasmInnerTracer) AfterTxnGroup(_ *EvalParams, _ *ledgercore.StateDelta, _ error) {
	t.events = append(t.events, "group end")
}
func (t *wasmInnerTracer) BeforeTxn(_ *EvalParams, _ int) { t.events = append(t.events, "txn begin") }
func (t *wasmInnerTracer) AfterTxn(_ *EvalParams, _ int, _ transactions.ApplyData, _ error) {
	t.events = append(t.events, "txn end")
}

func TestWasmInnerAssetDifferential(t *testing.T) {
	partitiontest.PartitionTest(t)
	var results []transactions.SignedTxnWithAD
	for _, wasm := range []bool{false, true} {
		ep := wasmInnerParams()
		addr := basics.AppIndex(888).Address()
		receiver := ep.TxnGroup[0].Txn.Sender
		ep.Ledger.(*Ledger).NewHolding(addr, 777, 100, false)
		var raw [8]byte
		binary.LittleEndian.PutUint64(raw[:], 42)
		ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{receiver[:], raw[:]}
		program := loadWasm(t, "inner_asset_transfer.wasm")
		if !wasm {
			program = testProg(t, `itxn_begin
int axfer
itxn_field TypeEnum
int 0
itxn_field Fee
txna Assets 0
itxn_field XferAsset
int 42
itxn_field AssetAmount
txna ApplicationArgs 0
itxn_field AssetReceiver
itxn_submit
itxn CreatedAssetID
!
`, ep.Proto.LogicSigVersion).Program
		}
		pass, cx, err := EvalContract(program, 0, 888, ep)
		require.NoError(t, err)
		require.True(t, pass)
		require.Len(t, cx.txn.EvalDelta.InnerTxns, 1)
		results = append(results, cx.txn.EvalDelta.InnerTxns[0])
		holding, err := ep.Ledger.AssetHolding(addr, 777)
		require.NoError(t, err)
		require.Equal(t, uint64(58), holding.Amount)
	}
	require.Equal(t, results[0], results[1])
}

func wasmInnerContext(t *testing.T) *EvalContext {
	t.Helper()
	pass, cx, err := EvalContract(loadWasm(t, "approve.wasm"), 0, 888, wasmInnerParams())
	require.NoError(t, err)
	require.True(t, pass)
	return cx
}

func TestWasmInnerRestrictions(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, op := range []uint32{0, 1, 2, 3, 4, 5} {
		cx := wasmInnerContext(t)
		cx.runMode = ModeSig
		_, err := cx.wasmInnerTxn(op, Fee, 0, nil)
		require.ErrorContains(t, err, "signature mode")
	}
	cx := wasmInnerContext(t)
	proto := *cx.Proto
	proto.IsolateClearState = true
	cx.Proto = &proto
	cx.txn.Txn.OnCompletion = transactions.ClearStateOC
	_, err := cx.wasmInnerTxn(0, 0, 0, nil)
	require.ErrorContains(t, err, "clear state")

	for _, op := range []uint32{1, 2, 4, 5} {
		cx = wasmInnerContext(t)
		_, err = cx.wasmInnerTxn(op, Fee, 0, nil)
		require.Error(t, err)
	}
	cx = wasmInnerContext(t)
	_, err = cx.wasmInnerTxn(0, 0, 0, nil)
	require.NoError(t, err)
	_, err = cx.wasmInnerTxn(0, 0, 0, nil)
	require.ErrorContains(t, err, "without itxn_submit")
	for _, field := range []TxnField{ApplicationID, Receiver, RekeyTo, ApprovalProgram, Assets} {
		_, err = cx.wasmInnerTxn(2, field, 0, nil)
		require.ErrorContains(t, err, "unsupported wasm inner field")
	}
	_, err = cx.wasmInnerTxn(2, TypeEnum, 6, nil)
	require.ErrorContains(t, err, "only acfg and axfer")
	_, err = cx.wasmInnerTxn(3, Type, 0, []byte("appl"))
	require.ErrorContains(t, err, "only acfg and axfer")
	_, err = cx.wasmInnerTxn(2, ConfigAssetName, 0, nil)
	require.ErrorContains(t, err, "wrong type")
	_, err = cx.wasmInnerTxn(3, ConfigAssetTotal, 0, []byte{})
	require.ErrorContains(t, err, "wrong type")
	_, err = cx.wasmInnerTxn(3, ConfigAssetName, 0, []byte{})
	require.NoError(t, err) // empty bytes stay bytes, not uint zero
	_, err = cx.wasmInnerTxn(2, ConfigAssetDecimals, 100, nil)
	require.Error(t, err)
	_, err = cx.wasmInnerTxn(2, XferAsset, 999, nil)
	require.ErrorContains(t, err, "unavailable Asset")
}

func TestWasmInnerFeesAndLimits(t *testing.T) {
	partitiontest.PartitionTest(t)
	ep := wasmInnerParams()
	ep.FeeCredit.Raw = 0
	pass, _, err := EvalContract(loadWasm(t, "inner_asset_create.wasm"), 0, 888, ep)
	require.False(t, pass)
	require.ErrorContains(t, err, "group fee")

	cx := wasmInnerContext(t)
	_, err = cx.wasmInnerTxn(0, 0, 0, nil)
	require.NoError(t, err)
	for i := 1; i < cx.Proto.MaxTxGroupSize; i++ {
		_, err = cx.wasmInnerTxn(1, 0, 0, nil)
		require.NoError(t, err)
	}
	_, err = cx.wasmInnerTxn(1, 0, 0, nil)
	require.ErrorContains(t, err, "too many inner transactions")
}
