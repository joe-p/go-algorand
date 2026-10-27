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
	"encoding/binary"
	"fmt"
	"testing"

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
	"github.com/stretchr/testify/require"
)

func TestWasmARC4ResourceArgumentsAndReturns(t *testing.T) {
	partitiontest.PartitionTest(t)
	hash := sha512.Sum512_256([]byte("deposit(asset,axfer,account,application)uint64"))
	for _, tc := range []struct {
		name                string
		asset, account, app byte
		txType              protocol.TxType
		assetID             basics.AssetIndex
		want                bool
	}{
		{"valid", 0, 0, 0, protocol.AssetTransferTx, 777, true},
		{"foreign", 1, 1, 1, protocol.AssetTransferTx, 778, true},
		{"bad asset ref", 2, 0, 0, protocol.AssetTransferTx, 777, false},
		{"bad account ref", 0, 2, 0, protocol.AssetTransferTx, 777, false},
		{"bad application ref", 0, 0, 2, protocol.AssetTransferTx, 777, false},
		{"wrong type", 0, 0, 0, protocol.PaymentTx, 777, false},
		{"method error", 0, 0, 0, protocol.AssetTransferTx, 778, false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			sender, other := basics.Address{42}, basics.Address{43}
			receiver := sender
			if tc.account == 1 {
				receiver = other
			}
			transfer := transactions.SignedTxn{Txn: transactions.Transaction{Type: tc.txType,
				AssetTransferTxnFields: transactions.AssetTransferTxnFields{XferAsset: tc.assetID, AssetAmount: ^uint64(0), AssetReceiver: receiver}}}
			app := transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.ApplicationCallTx,
				Header: transactions.Header{Sender: sender},
				ApplicationCallTxnFields: transactions.ApplicationCallTxnFields{ApplicationID: 888,
					ForeignAssets: []basics.AssetIndex{777, 778}, Accounts: []basics.Address{other}, ForeignApps: []basics.AppIndex{888},
					ApplicationArgs: [][]byte{hash[:4], {tc.asset}, {tc.account}, {tc.app}}}}}
			// One preceding transfer, then 15 normal app calls for pooled fuel.
			txns := []transactions.SignedTxn{transfer}
			for i := 0; i < 15; i++ {
				txns = append(txns, app)
			}
			ep := wasmAppParams(txns...)
			pass, cx, err := EvalContract(loadWasm(t, "app_avm_sdk_arc4_resources.wasm"), 1, 888, ep)
			require.NoError(t, err)
			require.Equal(t, tc.want, pass)
			if tc.want {
				var result [12]byte
				copy(result[:4], []byte{0x15, 0x1f, 0x7c, 0x75})
				binary.BigEndian.PutUint64(result[4:], ^uint64(0))
				require.Equal(t, []string{string(result[:])}, cx.txn.EvalDelta.Logs)
				t.Logf("size=%d cost=%d", len(cx.program), cx.Cost())
			} else {
				require.Empty(t, cx.txn.EvalDelta.Logs)
			}
		})
	}
}

func TestWasmLogAVMParity(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		size, count int
		err         string
	}{
		{0, 32, ""}, {4, 1, ""}, {1024, 1, ""}, {512, 2, ""},
		{1, 33, "too many log calls"}, {1025, 1, "logs too large"}, {513, 2, "logs too large"},
	} {
		t.Run(fmt.Sprintf("%dx%d", tc.count, tc.size), func(t *testing.T) {
			var logs [][]string
			for _, wasm := range []bool{false, true} {
				app := transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.ApplicationCallTx,
					Header: transactions.Header{Fee: basics.MicroAlgos{Raw: uint64(tc.size)}, FirstValid: basics.Round(tc.count), LastValid: 1}}}
				ep := wasmAppParams(app, app, app, app, app, app, app, app)
				program := loadWasm(t, "app_log.wasm")
				if !wasm {
					payload := make([]byte, tc.size)
					copy(payload, []byte{0x15, 0x1f, 0x7c, 0x75})
					source := ""
					for i := 0; i < tc.count; i++ {
						source += fmt.Sprintf("byte 0x%x\nlog\n", payload)
					}
					source += "int 1"
					program = testProg(t, source, ep.Proto.LogicSigVersion).Program
				}
				pass, cx, err := EvalContract(program, 0, 888, ep)
				if tc.err != "" {
					require.False(t, pass)
					require.ErrorContains(t, err, tc.err)
				} else {
					require.NoError(t, err)
					require.True(t, pass)
					logs = append(logs, cx.txn.EvalDelta.Logs)
				}
			}
			if tc.err == "" {
				require.Equal(t, logs[0], logs[1])
			}
		})
	}
}
