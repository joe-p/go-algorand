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
	"crypto/sha512"
	"strings"
	"testing"

	"github.com/algorand/avm-abi/abi"
	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

// arc4Call encodes an ARC-4 method call's application arguments: the selector
// of signature, then each argument encoded as its ABI type.
func arc4Call(t *testing.T, signature string, types []string, values ...interface{}) [][]byte {
	t.Helper()
	require.NoError(t, abi.VerifyMethodSignature(signature))
	hash := sha512.Sum512_256([]byte(signature))
	args := [][]byte{hash[:4]}
	for i, typ := range types {
		abiType, err := abi.TypeOf(typ)
		require.NoError(t, err)
		encoded, err := abiType.Encode(values[i])
		require.NoError(t, err)
		args = append(args, encoded)
	}
	return args
}

// evalArc4 calls app 888 (or creates it, for appID 0) with args and
// onCompletion, in a group of 16 app calls so the pooled budget covers the
// module's setup charge. It returns the approval result and the global delta.
func evalArc4(t *testing.T, program []byte, appID basics.AppIndex, oc transactions.OnCompletion, args [][]byte) (bool, basics.StateDelta) {
	t.Helper()
	txns := make([]transactions.SignedTxn, 16)
	for i := range txns {
		txns[i].Txn.Type = protocol.ApplicationCallTx
		txns[i].Txn.ApplicationID = 888
	}
	txns[0].Txn.ApplicationID = appID
	txns[0].Txn.OnCompletion = oc
	txns[0].Txn.ApplicationArgs = args
	ep := NewAppEvalParams(transactions.WrapSignedTxnsWithAD(txns), makeTestProto(wasmAppProto), &transactions.SpecialAddresses{})
	ledger := NewLedger(nil)
	ledger.NewApp(basics.Address{}, 888, basics.AppParams{StateSchemas: basics.StateSchemas{
		GlobalStateSchema: basics.StateSchema{NumUint: 1, NumByteSlice: 1},
	}})
	ep.Ledger = ledger
	ep.SigLedger = ledger

	pass, cx, err := EvalContract(program, 0, 888, ep)
	t.Logf("cost %d (fuel %d)", cx.Cost(), cx.wasmFuelUsed)
	// A rejection must come from the program returning 0, not from a trap or
	// an exhausted budget.
	require.NoError(t, err)
	return pass, ep.TxnGroup[0].EvalDelta.GlobalDelta
}

// TestWasmAppArc4Counter runs the avm-sdk ARC-4 counter fixture with calls
// encoded by the avm-abi package, which checks the SDK's selectors and argument
// decoding against an independent implementation.
func TestWasmAppArc4Counter(t *testing.T) {
	partitiontest.PartitionTest(t)

	program := loadWasm(t, "app_avm_sdk_arc4_counter.wasm")
	increment := func(delta uint64) [][]byte {
		return arc4Call(t, "increment(uint64)void", []string{"uint64"}, delta)
	}
	setLabel := func(label string) [][]byte {
		return arc4Call(t, "set_label(string)void", []string{"string"}, label)
	}

	t.Run("increment", func(t *testing.T) {
		pass, delta := evalArc4(t, program, 888, transactions.NoOpOC, increment(5))
		require.True(t, pass)
		require.Equal(t, basics.ValueDelta{Action: basics.SetUintAction, Uint: 5}, delta["count"])
	})

	t.Run("set_label", func(t *testing.T) {
		pass, delta := evalArc4(t, program, 888, transactions.NoOpOC, setLabel("hello"))
		require.True(t, pass)
		require.Equal(t, basics.ValueDelta{Action: basics.SetBytesAction, Bytes: "hello"}, delta["label"])
	})

	t.Run("bare create", func(t *testing.T) {
		pass, delta := evalArc4(t, program, 0, transactions.NoOpOC, nil)
		require.True(t, pass)
		require.Empty(t, delta)
	})

	rejects := []struct {
		name  string
		appID basics.AppIndex
		oc    transactions.OnCompletion
		args  [][]byte
	}{
		{"zero delta", 888, transactions.NoOpOC, increment(0)},
		{"over the cap", 888, transactions.NoOpOC, increment(1_000_001)},
		{"label too long", 888, transactions.NoOpOC, setLabel("0123456789abcdef0123456789abcdef!")},
		{"method at creation", 0, transactions.NoOpOC, increment(5)},
		{"opt-in", 888, transactions.OptInOC, increment(5)},
		{"bare call", 888, transactions.NoOpOC, nil},
		{"unknown selector", 888, transactions.NoOpOC, arc4Call(t, "decrement(uint64)void", []string{"uint64"}, uint64(5))},
		{"wrong argument type", 888, transactions.NoOpOC, arc4Call(t, "increment(uint32)void", []string{"uint32"}, uint32(5))},
		{"extra argument", 888, transactions.NoOpOC, append(increment(5), []byte{0})},
		{"short argument", 888, transactions.NoOpOC, [][]byte{increment(5)[0], {0, 0, 0, 5}}},
	}
	for _, c := range rejects {
		t.Run(c.name, func(t *testing.T) {
			pass, delta := evalArc4(t, program, c.appID, c.oc, c.args)
			require.False(t, pass)
			require.Empty(t, delta)
		})
	}
}

// TestWasmAppArc4Types runs the avm-sdk ARC-4 types fixture: dynamic arrays,
// nested tuples with packed bools, and a method with more than 15 parameters,
// all encoded by the avm-abi package.
func TestWasmAppArc4Types(t *testing.T) {
	partitiontest.PartitionTest(t)

	program := loadWasm(t, "app_avm_sdk_arc4_types.wasm")
	owner := make([]byte, 32)
	owner[31] = 5

	wide := func() [][]byte {
		types := make([]string, 17)
		for i := range types {
			types[i] = "uint64"
		}
		signature := "wide(" + strings.Join(types, ",") + ")void"
		values := make([]interface{}, 14)
		for i := range values {
			values[i] = uint64(i + 1)
		}
		args := arc4Call(t, signature, types[:14], values...)
		// ARC-4 packs parameters 15 onward into a tuple in the last argument.
		packed := arc4Call(t, "packed((uint64,uint64,uint64))void", []string{"(uint64,uint64,uint64)"},
			[]interface{}{uint64(15), uint64(16), uint64(17)})
		return append(args, packed[1])
	}

	cases := []struct {
		name  string
		args  [][]byte
		total uint64
		text  string
	}{
		{
			name:  "sum uint64[]",
			args:  arc4Call(t, "sum(uint64[])void", []string{"uint64[]"}, []interface{}{uint64(1), uint64(2), uint64(3), uint64(40)}),
			total: 46,
		},
		{
			name:  "join string[]",
			args:  arc4Call(t, "join(string[])void", []string{"string[]"}, []interface{}{"a", "bc", "日本"}),
			total: 3,
			text:  "a,bc,日本",
		},
		{
			name: "record nested tuple and tuple array",
			args: arc4Call(t, "record(((uint8,string),bool[2],address),(uint16,byte[])[])void",
				[]string{"((uint8,string),bool[2],address)", "(uint16,byte[])[]"},
				[]interface{}{[]interface{}{uint8(9), "lbl"}, []interface{}{true, true}, owner},
				[]interface{}{[]interface{}{uint16(1), []byte("x")}, []interface{}{uint16(2), []byte("yz")}}),
			total: 9 + 5 + 200 + 1 + 2,
			text:  "lblxyz",
		},
		{
			name:  "wide packs parameters past 15",
			args:  wide(),
			total: 1785, // sum of i*i for i in 1..=17
		},
	}
	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			pass, delta := evalArc4(t, program, 888, transactions.NoOpOC, c.args)
			require.True(t, pass)
			require.Equal(t, basics.ValueDelta{Action: basics.SetUintAction, Uint: c.total}, delta["total"])
			if c.text != "" {
				require.Equal(t, basics.ValueDelta{Action: basics.SetBytesAction, Bytes: c.text}, delta["text"])
			}
		})
	}

	sum := arc4Call(t, "sum(uint64[])void", []string{"uint64[]"}, []interface{}{uint64(1), uint64(2)})
	join := arc4Call(t, "join(string[])void", []string{"string[]"}, []interface{}{"ok"})
	rejects := []struct {
		name string
		args [][]byte
	}{
		{"array length overstated", [][]byte{sum[0], append([]byte{0, 3}, sum[1][2:]...)}},
		{"trailing byte", [][]byte{sum[0], append(append([]byte{}, sum[1]...), 0)}},
		{"invalid UTF-8", [][]byte{join[0], {0, 1, 0, 2, 0, 1, 0xff}}},
		{"wide unpacked", append(wide()[:15], make([]byte, 8), make([]byte, 8))},
	}
	for _, c := range rejects {
		t.Run(c.name, func(t *testing.T) {
			pass, delta := evalArc4(t, program, 888, transactions.NoOpOC, c.args)
			require.False(t, pass)
			require.Empty(t, delta)
		})
	}
}
