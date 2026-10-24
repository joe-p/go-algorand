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

package verify

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/crypto"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/data/transactions/logic"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

func readWasmFixture(t *testing.T, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "logic", "testdata", "wasm", name))
	require.NoError(t, err)
	return data
}

// wasmContractTxn builds a transaction authorized by a contract-account WASM
// LogicSig: the sender is the hash of the program bytes.
func wasmContractTxn(program []byte, args [][]byte) transactions.SignedTxn {
	fee := config.Consensus[protocol.ConsensusFuture].MinTxnFee
	var receiver basics.Address
	receiver[0] = 7
	return transactions.SignedTxn{
		Txn: createPayTransaction(fee, 40, 60, 1, basics.Address(logic.HashProgram(program)), receiver),
		Lsig: transactions.LogicSig{
			Logic: program,
			Args:  args,
		},
	}
}

func TestWasmLogicSigTxn(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	blkHdr := createDummyBlockHeader(protocol.ConsensusFuture)
	require.True(t, config.Consensus[protocol.ConsensusFuture].EnableWasmLogicSig)

	approve := wasmContractTxn(readWasmFixture(t, "approve.wasm"), nil)
	_, err := TxnGroup([]transactions.SignedTxn{approve}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.NoError(t, err)

	reject := wasmContractTxn(readWasmFixture(t, "reject.wasm"), nil)
	_, err = TxnGroup([]transactions.SignedTxn{reject}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.Error(t, err)
}

// TestWasmLogicSigReadsTxnField checks that a WASM LogicSig can read a
// transaction field through the host callback and be verified end to end.
func TestWasmLogicSigReadsTxnField(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	blkHdr := createDummyBlockHeader(protocol.ConsensusFuture)
	program := readWasmFixture(t, "txn_uint.wasm")

	fee := config.Consensus[protocol.ConsensusFuture].MinTxnFee
	var receiver basics.Address
	receiver[0] = 7
	sender := basics.Address(logic.HashProgram(program))

	// txn_uint.wasm approves iff FirstValid is 1234.
	approve := transactions.SignedTxn{
		Txn:  createPayTransaction(fee, 1234, 1400, 1, sender, receiver),
		Lsig: transactions.LogicSig{Logic: program},
	}
	_, err := TxnGroup([]transactions.SignedTxn{approve}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.NoError(t, err)

	// A different FirstValid rejects.
	reject := transactions.SignedTxn{
		Txn:  createPayTransaction(fee, 1235, 1400, 1, sender, receiver),
		Lsig: transactions.LogicSig{Logic: program},
	}
	_, err = TxnGroup([]transactions.SignedTxn{reject}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.Error(t, err)
}

// TestWasmLogicSigEd25519VerifyBare checks that a WASM LogicSig can verify an
// ed25519 signature through the host callback and be verified end to end.
func TestWasmLogicSigEd25519VerifyBare(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	blkHdr := createDummyBlockHeader(protocol.ConsensusFuture)
	program := readWasmFixture(t, "ed25519verify_bare.wasm")

	var seed crypto.Seed
	seed[0] = 1
	secrets := crypto.GenerateSignatureSecrets(seed)
	data := []byte("verify ed25519verify_bare end to end")
	sig := secrets.SignBytes(data)

	approve := wasmContractTxn(program, [][]byte{data, sig[:], secrets.SignatureVerifier[:]})
	_, err := TxnGroup([]transactions.SignedTxn{approve}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.NoError(t, err)

	// A tampered message rejects.
	badData := append([]byte(nil), data...)
	badData[0] ^= 0xff
	reject := wasmContractTxn(program, [][]byte{badData, sig[:], secrets.SignatureVerifier[:]})
	_, err = TxnGroup([]transactions.SignedTxn{reject}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.Error(t, err)
}

// TestWasmLogicSigArgAccess checks that the arg-access rule applies to WASM
// LogicSigs too: a program that ignores an argument it carries is rejected.
func TestWasmLogicSigArgAccess(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	blkHdr := createDummyBlockHeader(protocol.ConsensusFuture)

	// approve.wasm never reads its arguments.
	unread := wasmContractTxn(readWasmFixture(t, "approve.wasm"), [][]byte{[]byte("x")})
	_, err := TxnGroup([]transactions.SignedTxn{unread}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.Error(t, err)

	// read_arg0.wasm reads arg 0, so one argument is fine.
	read := wasmContractTxn(readWasmFixture(t, "read_arg0.wasm"), [][]byte{[]byte("x")})
	_, err = TxnGroup([]transactions.SignedTxn{read}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.NoError(t, err)
}

// TestWasmLogicSigDisabled checks that a WASM program is rejected when the
// protocol has not enabled WASM LogicSigs.
func TestWasmLogicSigDisabled(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	blkHdr := createDummyBlockHeader(protocol.ConsensusCurrentVersion)
	require.False(t, config.Consensus[protocol.ConsensusCurrentVersion].EnableWasmLogicSig)

	stxn := wasmContractTxn(readWasmFixture(t, "approve.wasm"), nil)
	_, err := TxnGroup([]transactions.SignedTxn{stxn}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.Error(t, err)
}

// TestWasmDelegatedLogicSigTxn checks the delegated form: the WASM program is
// signed by the account it spends from.
func TestWasmDelegatedLogicSigTxn(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	blkHdr := createDummyBlockHeader(protocol.ConsensusFuture)
	program := readWasmFixture(t, "approve.wasm")

	signer := keypair()
	fee := config.Consensus[protocol.ConsensusFuture].MinTxnFee
	var receiver basics.Address
	receiver[0] = 9
	txn := createPayTransaction(fee, 40, 60, 1, basics.Address(signer.SignatureVerifier), receiver)

	sig := signer.Sign(logic.Program(program))

	stxn := transactions.SignedTxn{
		Txn:  txn,
		Lsig: transactions.LogicSig{Logic: program, Sig: sig},
	}
	_, err := TxnGroup([]transactions.SignedTxn{stxn}, &blkHdr, nil, &DummyLedgerForSignature{})
	require.NoError(t, err)
}
