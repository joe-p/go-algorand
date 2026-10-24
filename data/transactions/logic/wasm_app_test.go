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

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

// wasmAppProto turns WASM application programs on in a test protocol.
func wasmAppProto(p *config.ConsensusParams) {
	p.EnableWasmApp = true
}

// wasmAppParams builds ModeApp EvalParams with WASM app programs enabled and a
// fresh in-memory ledger.
func wasmAppParams(txns ...transactions.SignedTxn) *EvalParams {
	if len(txns) == 0 {
		txns = []transactions.SignedTxn{{Txn: transactions.Transaction{Type: protocol.ApplicationCallTx}}}
	}
	proto := makeTestProto(wasmAppProto)
	ep := NewAppEvalParams(transactions.WrapSignedTxnsWithAD(txns), proto, &transactions.SpecialAddresses{})
	ep.Trace = &strings.Builder{}
	ledger := NewLedger(nil)
	ep.Ledger = ledger
	ep.SigLedger = ledger
	return ep
}

// TestWasmAppApproveReject runs trivial WASM programs as application programs.
func TestWasmAppApproveReject(t *testing.T) {
	partitiontest.PartitionTest(t)

	testAppBytes(t, loadWasm(t, "approve.wasm"), wasmAppParams())
	testAppBytes(t, loadWasm(t, "reject.wasm"), wasmAppParams(), "REJECT")
}

// TestWasmAppClearState runs WASM programs as clear-state programs.
func TestWasmAppClearState(t *testing.T) {
	partitiontest.PartitionTest(t)

	txn := makeSampleTxn()
	txn.Txn.Type = protocol.ApplicationCallTx
	txn.Txn.OnCompletion = transactions.ClearStateOC
	testAppBytes(t, loadWasm(t, "approve.wasm"), wasmAppParams(txn))
	testAppBytes(t, loadWasm(t, "reject.wasm"), wasmAppParams(txn), "REJECT")
}

// TestWasmAppReadsAppArgs checks the txna_bytes host function against a real
// application call: the program approves iff ApplicationArgs[0] is three bytes.
func TestWasmAppReadsAppArgs(t *testing.T) {
	partitiontest.PartitionTest(t)

	txn := makeSampleTxn()
	txn.Txn.Type = protocol.ApplicationCallTx
	// The sample transaction's first argument is protocol.PaymentTx ("pay").
	testAppBytes(t, loadWasm(t, "app_arg_len.wasm"), wasmAppParams(txn))

	// A different argument length must reject.
	bad := makeSampleTxn()
	bad.Txn.Type = protocol.ApplicationCallTx
	bad.Txn.ApplicationArgs[0] = []byte("too long")
	testAppBytes(t, loadWasm(t, "app_arg_len.wasm"), wasmAppParams(bad), "REJECT")
}

// TestWasmAppNumAppArgs checks txn_uint on the array-length field NumAppArgs.
func TestWasmAppNumAppArgs(t *testing.T) {
	partitiontest.PartitionTest(t)

	txn := makeSampleTxn()
	txn.Txn.Type = protocol.ApplicationCallTx
	testAppBytes(t, loadWasm(t, "app_num_args.wasm"), wasmAppParams(txn))
}

// TestWasmAppArgIsUnavailable checks that the signature-only arg host functions
// trap in application mode, matching the AVM.
func TestWasmAppArgIsUnavailable(t *testing.T) {
	partitiontest.PartitionTest(t)

	testAppBytes(t, loadWasm(t, "app_arg_unavailable.wasm"), wasmAppParams(), "application mode")
}

// TestWasmAppDisabled checks that WASM application programs are rejected when
// the protocol does not enable them.
func TestWasmAppDisabled(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := wasmAppParams()
	ep.Proto = makeTestProto(nil) // EnableWasmApp defaults to false
	err := CheckContract(loadWasm(t, "approve.wasm"), 0, ep)
	require.Error(t, err)
}

// TestWasmAppVersionsMixedRejected checks that an app's approval and clear
// programs must be the same kind.
func TestWasmAppVersionsMixedRejected(t *testing.T) {
	partitiontest.PartitionTest(t)

	proto := makeTestProto(wasmAppProto)
	approval := loadWasm(t, "approve.wasm")
	clear := testProg(t, "int 1", proto.LogicSigVersion).Program
	err := transactions.CheckContractVersions(approval, clear, basics.AppParams{}, proto)
	require.ErrorContains(t, err, "both be WASM or both be AVM")
}

// TestWasmAppKindChangeRejected checks that an AVM app cannot be updated to WASM.
func TestWasmAppKindChangeRejected(t *testing.T) {
	partitiontest.PartitionTest(t)

	proto := makeTestProto(wasmAppProto)
	avm := testProg(t, "int 1", proto.LogicSigVersion).Program
	previous := basics.AppParams{ApprovalProgram: avm, ClearStateProgram: avm}
	approval := loadWasm(t, "approve.wasm")
	err := transactions.CheckContractVersions(approval, approval, previous, proto)
	require.ErrorContains(t, err, "cannot change app program between WASM and AVM")
}

// TestWasmAppVersionsBothWasmOK checks that a WASM app can be created and
// updated as WASM.
func TestWasmAppVersionsBothWasmOK(t *testing.T) {
	partitiontest.PartitionTest(t)

	proto := makeTestProto(wasmAppProto)
	approval := loadWasm(t, "approve.wasm")
	require.NoError(t, transactions.CheckContractVersions(approval, approval, basics.AppParams{}, proto))
	previous := basics.AppParams{ApprovalProgram: approval, ClearStateProgram: approval}
	require.NoError(t, transactions.CheckContractVersions(approval, approval, previous, proto))
}

// TestWasmAppVersionsDisabled checks that WASM app programs are rejected by the
// version check when the protocol does not enable them.
func TestWasmAppVersionsDisabled(t *testing.T) {
	partitiontest.PartitionTest(t)

	proto := makeTestProto(nil) // EnableWasmApp defaults to false
	approval := loadWasm(t, "approve.wasm")
	err := transactions.CheckContractVersions(approval, approval, basics.AppParams{}, proto)
	require.ErrorContains(t, err, "not enabled")
}

// TestWasmAppInnerProgramVersion checks that inner app-call checks treat a WASM
// program as the protocol's maximum AVM version, rather than its (meaningless)
// version 0, so that an AVM app can create or call a WASM app.
func TestWasmAppInnerProgramVersion(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := wasmAppParams()
	cx := EvalContext{EvalParams: ep}
	v, err := innerProgramVersion(&cx, loadWasm(t, "approve.wasm"))
	require.NoError(t, err)
	require.Equal(t, ep.Proto.LogicSigVersion, v)

	// An AVM program still reports its own version byte.
	avm := testProg(t, "int 1", ep.Proto.LogicSigVersion).Program
	v, err = innerProgramVersion(&cx, avm)
	require.NoError(t, err)
	require.Equal(t, ep.Proto.LogicSigVersion, v)

	// When WASM is disabled the bytes are read as an AVM program (version 0).
	disabled := wasmAppParams()
	disabled.Proto = makeTestProto(nil)
	cxDisabled := EvalContext{EvalParams: disabled}
	v, err = innerProgramVersion(&cxDisabled, loadWasm(t, "approve.wasm"))
	require.NoError(t, err)
	require.Equal(t, uint64(0), v)
}
