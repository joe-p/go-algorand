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
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/test/partitiontest"
)

func loadWasm(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("testdata", "wasm", name))
	require.NoError(t, err)
	return data
}

// wasmProto turns WASM LogicSigs on in a test protocol.
func wasmProto(p *config.ConsensusParams) {
	p.EnableWasmLogicSig = true
}

func TestIsWasmProgram(t *testing.T) {
	partitiontest.PartitionTest(t)
	require.True(t, IsWasmProgram(loadWasm(t, "approve.wasm")))
	require.False(t, IsWasmProgram(testProg(t, "int 1", LogicVersion).Program))
	require.False(t, IsWasmProgram(nil))
	require.False(t, IsWasmProgram([]byte{}))
}

func TestWasmApproveReject(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := optSigParams(wasmProto)
	pass, cx, err := EvalSignatureProgram(loadWasm(t, "approve.wasm"), nil, 0, ep)
	require.NoError(t, err)
	require.True(t, pass)
	require.GreaterOrEqual(t, cx.Cost(), 0)

	ep = optSigParams(wasmProto)
	pass, _, err = EvalSignatureProgram(loadWasm(t, "reject.wasm"), nil, 0, ep)
	require.NoError(t, err)
	require.False(t, pass)
}

func TestWasmReadsArg(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := optSigParams(wasmProto)
	pass, cx, err := EvalSignatureProgram(loadWasm(t, "read_arg0.wasm"), [][]byte{[]byte("hello")}, 0, ep)
	require.NoError(t, err)
	require.True(t, pass)
	_, unaccounted := cx.UnaccountedArg()
	require.False(t, unaccounted)
	require.True(t, cx.argRead(0))
}

func TestWasmUnreadArgIsUnaccounted(t *testing.T) {
	partitiontest.PartitionTest(t)

	// approve.wasm ignores its arguments, so an argument it carries is
	// unaccounted for.
	ep := optSigParams(wasmProto)
	pass, cx, err := EvalSignatureProgram(loadWasm(t, "approve.wasm"), [][]byte{[]byte("x")}, 0, ep)
	require.NoError(t, err)
	require.True(t, pass)
	idx, unaccounted := cx.UnaccountedArg()
	require.True(t, unaccounted)
	require.Equal(t, 0, idx)
}

func TestWasmFuelExhaustion(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := optSigParams(func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 100
	})
	_, _, err := EvalSignatureProgram(loadWasm(t, "runaway.wasm"), nil, 0, ep)
	require.Error(t, err)
	require.Contains(t, err.Error(), "fuel")
}

func TestCheckWasmSignature(t *testing.T) {
	partitiontest.PartitionTest(t)

	require.NoError(t, CheckSignature(loadWasm(t, "approve.wasm"), optSigParams(wasmProto)))
	// No `program` export.
	require.Error(t, CheckSignature(loadWasm(t, "no_program.wasm"), optSigParams(wasmProto)))
}

func TestCheckWasmSignatureDisabled(t *testing.T) {
	partitiontest.PartitionTest(t)

	// With WASM disabled, a magic-prefixed program must not check out as AVM
	// bytecode.
	require.Error(t, CheckSignature(loadWasm(t, "approve.wasm"), optSigParams(nil)))
}

func TestValidateWasmProgram(t *testing.T) {
	partitiontest.PartitionTest(t)

	require.NoError(t, validateWasmProgram(loadWasm(t, "approve.wasm")))
	require.Error(t, validateWasmProgram(loadWasm(t, "no_program.wasm")))
	require.Error(t, validateWasmProgram([]byte("\x00asmjunk")))
}
