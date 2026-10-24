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
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/crypto"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
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

// TestWasmReadsTxnUint checks the txn_uint host function against a real
// transaction: the program approves iff FirstValid is 1234.
func TestWasmReadsTxnUint(t *testing.T) {
	partitiontest.PartitionTest(t)

	txn := transactions.SignedTxn{Txn: transactions.Transaction{
		Type:   protocol.PaymentTx,
		Header: transactions.Header{FirstValid: 1234},
	}}

	ep := optSigParams(wasmProto, txn)
	pass, _, err := EvalSignatureProgram(loadWasm(t, "txn_uint.wasm"), nil, 0, ep)
	require.NoError(t, err)
	require.True(t, pass)

	// A different field value makes the program reject rather than error.
	txn.Txn.FirstValid = 1
	ep = optSigParams(wasmProto, txn)
	pass, _, err = EvalSignatureProgram(loadWasm(t, "txn_uint.wasm"), nil, 0, ep)
	require.NoError(t, err)
	require.False(t, pass)
}

// TestWasmReadsTxnBytes checks the txn_bytes host function: the program reads
// the Sender and approves iff it is 32 bytes.
func TestWasmReadsTxnBytes(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := optSigParams(wasmProto, transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.PaymentTx}})
	pass, _, err := EvalSignatureProgram(loadWasm(t, "txn_bytes.wasm"), nil, 0, ep)
	require.NoError(t, err)
	require.True(t, pass)

	txn := transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.PaymentTx}}
	copy(txn.Txn.Sender[:], bytes.Repeat([]byte{7}, len(txn.Txn.Sender)))
	program := loadWasm(t, "txn_bytes_field.wasm")
	pass, _, err = EvalSignatureProgram(program, [][]byte{{byte(Sender)}}, 0, optSigParams(wasmProto, txn))
	require.NoError(t, err)
	require.True(t, pass)
	txn.Txn.Sender[0] = 8
	pass, _, err = EvalSignatureProgram(program, [][]byte{{byte(Sender)}}, 0, optSigParams(wasmProto, txn))
	require.NoError(t, err)
	require.False(t, pass)
}

// TestWasmEd25519VerifyBare checks the ed25519verify_bare host function against
// a real ed25519 signature: the program approves iff the signature is valid.
func TestWasmEd25519VerifyBare(t *testing.T) {
	partitiontest.PartitionTest(t)

	c := crypto.GenerateSignatureSecrets(randSeed())
	data := []byte("wasm ed25519verify_bare test message")
	sig := c.SignBytes(data)
	program := loadWasm(t, "ed25519verify_bare.wasm")

	pass, _, err := EvalSignatureProgram(program, [][]byte{data, sig[:], c.SignatureVerifier[:]}, 0, optSigParams(wasmProto))
	require.NoError(t, err)
	require.True(t, pass)

	// A tampered message fails verification, so the program rejects rather than
	// erroring.
	badData := append([]byte(nil), data...)
	badData[0] ^= 0xff
	pass, _, err = EvalSignatureProgram(program, [][]byte{badData, sig[:], c.SignatureVerifier[:]}, 0, optSigParams(wasmProto))
	require.NoError(t, err)
	require.False(t, pass)

	// A short signature and a short public key are errors, as in the AVM.
	_, _, err = EvalSignatureProgram(program, [][]byte{data, sig[1:], c.SignatureVerifier[:]}, 0, optSigParams(wasmProto))
	require.ErrorContains(t, err, "invalid signature")
	_, _, err = EvalSignatureProgram(program, [][]byte{data, sig[:], c.SignatureVerifier[1:]}, 0, optSigParams(wasmProto))
	require.ErrorContains(t, err, "invalid public key")
}

// TestWasmEd25519DalekGuest checks a Rust guest that uses ed25519-dalek's
// `VerifyingKey::verify` against the patched host-backed crate: the fork routes
// verification to ed25519verify_bare, so a real Go-produced signature is
// accepted and a tampered message rejected.
func TestWasmEd25519DalekGuest(t *testing.T) {
	partitiontest.PartitionTest(t)

	c := crypto.GenerateSignatureSecrets(randSeed())
	data := []byte("wasm ed25519-dalek guest test message")
	sig := c.SignBytes(data)
	program := loadWasm(t, "ed25519_dalek.wasm")

	pass, _, err := EvalSignatureProgram(program, [][]byte{data, sig[:], c.SignatureVerifier[:]}, 0, optSigParams(wasmProto))
	require.NoError(t, err)
	require.True(t, pass)

	// A tampered message fails verification, so the program rejects.
	badData := append([]byte(nil), data...)
	badData[0] ^= 0xff
	pass, _, err = EvalSignatureProgram(program, [][]byte{badData, sig[:], c.SignatureVerifier[:]}, 0, optSigParams(wasmProto))
	require.NoError(t, err)
	require.False(t, pass)

	// A wrong-length signature is rejected: the guest requires exactly 64 bytes.
	pass, _, err = EvalSignatureProgram(program, [][]byte{data, sig[1:], c.SignatureVerifier[:]}, 0, optSigParams(wasmProto))
	require.NoError(t, err)
	require.False(t, pass)
}

// TestWasmTxnFieldTypeError checks that asking for a field with the wrong type
// traps, just as an invalid field would in the AVM.
func TestWasmTxnFieldTypeError(t *testing.T) {
	partitiontest.PartitionTest(t)

	ep := optSigParams(wasmProto, transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.PaymentTx}})
	_, _, err := EvalSignatureProgram(loadWasm(t, "txn_bad_type.wasm"), nil, 0, ep)
	require.Error(t, err)
	require.Contains(t, err.Error(), "not a uint")
}

func TestWasmTxnBytesLimits(t *testing.T) {
	partitiontest.PartitionTest(t)
	program := loadWasm(t, "txn_bytes_field.wasm")
	for _, size := range []int{0, 32, maxStringSize, maxStringSize + 1} {
		t.Run(fmt.Sprint(size), func(t *testing.T) {
			txn := transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.ApplicationCallTx}}
			txn.Txn.ApprovalProgram = bytes.Repeat([]byte{7}, size)
			ep := optSigParams(wasmProto, txn)
			pass, _, err := EvalSignatureProgram(program, [][]byte{{byte(ApprovalProgram)}}, 0, ep)
			if size > maxStringSize {
				require.ErrorContains(t, err, "too big")
			} else {
				require.NoError(t, err)
				require.True(t, pass)
			}
		})
	}
}

func TestWasmTxnFieldRestrictions(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, field := range []TxnField{ApplicationArgs, Accounts, NumLogs, LastLog, invalidTxnField} {
		t.Run(field.String(), func(t *testing.T) {
			ep := optSigParams(wasmProto)
			_, _, err := EvalSignatureProgram(loadWasm(t, "txn_bytes_field.wasm"), [][]byte{{byte(field)}}, 0, ep)
			require.Error(t, err)
		})
	}
	ep := optSigParams(func(p *config.ConsensusParams) {
		wasmProto(p)
		p.LogicSigVersion = 1
	})
	_, _, err := EvalSignatureProgram(loadWasm(t, "txn_bytes_field.wasm"), [][]byte{{byte(RekeyTo)}}, 0, ep)
	require.ErrorContains(t, err, "invalid txn field RekeyTo")
}

func TestWasmTxnConcurrentContexts(t *testing.T) {
	partitiontest.PartitionTest(t)
	program := loadWasm(t, "txn_uint.wasm")
	for i := range 32 {
		t.Run(fmt.Sprint(i), func(t *testing.T) {
			t.Parallel()
			txn := transactions.SignedTxn{Txn: transactions.Transaction{Type: protocol.PaymentTx}}
			txn.Txn.FirstValid = 1234
			if i%2 != 0 {
				txn.Txn.FirstValid++
			}
			pass, _, err := EvalSignatureProgram(program, nil, 0, optSigParams(wasmProto, txn))
			require.NoError(t, err)
			require.Equal(t, i%2 == 0, pass)
		})
	}
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
