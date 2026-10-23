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

package ledger

import (
	"context"
	"encoding/binary"
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/agreement"
	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/bookkeeping"
	"github.com/algorand/go-algorand/data/committee"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/data/transactions/logic"
	"github.com/algorand/go-algorand/data/transactions/verify"
	"github.com/algorand/go-algorand/ledger/eval"
	"github.com/algorand/go-algorand/ledger/ledgercore"
	ledgertesting "github.com/algorand/go-algorand/ledger/testing"
	"github.com/algorand/go-algorand/logging"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/util/execpool"
)

// benchLsigsPerBlock is the number of LogicSig transactions placed in each
// benchmark block.
const benchLsigsPerBlock = 100

// tealFibSrc is a fibonacci LogicSig that reads N from LogicSig argument 0
// (big-endian) and computes fib(N) in a loop. It is paired with fib.wasm, which
// does the same computation under the wasmi executor, so the two blocks can be
// compared at an equal input.
//
// The running values are masked to 2^62 so the `+` cannot overflow: TEAL's `+`
// fails on uint64 overflow, which would otherwise cap N at 92. fib.wasm applies
// the same mask.
const tealFibSrc = `#pragma version 8
int 0        // a = fib(0)
int 1        // b = fib(1)
arg 0
btoi         // n
fib_loop:
  dup
  int 0
  ==
  bnz fib_done
  // [a b n] -> [b (a+b)%2^62 n-1]
  dig 1
  dig 3
  +
  int 0x3FFFFFFFFFFFFFFF
  &
  uncover 3
  pop
  swap
  int 1
  -
  b fib_loop
fib_done:
  pop
  pop
  pop
  int 1
`

// BenchmarkBlockEvaluatorLsigs times the cost of *executing* a block of
// fibonacci LogicSigs: the eval/verification pass, where the LogicSigs run.
// It isolates the AVM vs wasmi execution cost. See
// BenchmarkBlockEvaluatorLsigsFull for validation, apply, and in-memory commit.
func BenchmarkBlockEvaluatorLsigs(b *testing.B) {
	teal, wasm, arg := lsigFibPrograms(b)
	b.Run("teal-fib", func(b *testing.B) {
		benchmarkLsigEval(b, teal, arg)
	})
	b.Run("wasm-fib", func(b *testing.B) {
		benchmarkLsigEval(b, wasm, arg)
	})
}

// BenchmarkBlockEvaluatorLsigsFull times the in-memory processing path for a block
// of fibonacci LogicSigs: Ledger.Validate (which executes the LogicSigs and
// computes the state delta) followed by AddValidatedBlock and a wait for the
// block to be committed to the in-memory block database. This includes the
// non-execution work that BenchmarkBlockEvaluatorLsigs leaves out, but does not
// measure disk persistence or every background tracker commit.
func BenchmarkBlockEvaluatorLsigsFull(b *testing.B) {
	teal, wasm, arg := lsigFibPrograms(b)
	b.Run("teal-fib", func(b *testing.B) {
		benchmarkLsigFullBlock(b, teal, arg)
	})
	b.Run("wasm-fib", func(b *testing.B) {
		benchmarkLsigFullBlock(b, wasm, arg)
	})
}

// lsigFibPrograms returns the TEAL and WASM fibonacci LogicSigs and the largest
// fib input N that fits the shared LogicSigMaxCost budget in both engines.
//
// WASM spends more of the budget per iteration than TEAL, so N is limited by
// WASM and the TEAL block does not quite exhaust its budget. The budget use is
// logged so the comparison is interpretable.
func lsigFibPrograms(b *testing.B) (teal, wasm, arg []byte) {
	proto := config.Consensus[protocol.ConsensusFuture]
	require.True(b, proto.EnableWasmLogicSig, "vFuture must enable WASM LogicSigs")

	tealOps, err := logic.AssembleString(tealFibSrc)
	require.NoError(b, err)
	wasm, err = os.ReadFile(filepath.Join("..", "data", "transactions", "logic", "testdata", "wasm", "fib.wasm"))
	require.NoError(b, err)

	tealMax := maxFibN(b, tealOps.Program, proto)
	wasmMax := maxFibN(b, wasm, proto)
	n := min(tealMax, wasmMax)
	arg = be32(n)

	_, tealCx, err := logic.EvalSignatureProgram(tealOps.Program, [][]byte{arg}, 0, lsigEvalParams(proto))
	require.NoError(b, err)
	_, wasmCx, err := logic.EvalSignatureProgram(wasm, [][]byte{arg}, 0, lsigEvalParams(proto))
	require.NoError(b, err)
	b.Logf("fib N=%d: teal max=%d cost=%d/%d, wasm max=%d fuel=%d/%d",
		n, tealMax, tealCx.Cost(), proto.LogicSigMaxCost, wasmMax, wasmCx.Cost(), proto.LogicSigMaxCost)
	return tealOps.Program, wasm, arg
}

// benchmarkLsigEval measures only the block eval/verification pass, against a
// static ledger, so it isolates LogicSig execution.
func benchmarkLsigEval(b *testing.B, program, arg []byte) {
	l := openLsigLedger(b, lsigGenesis(b, program))
	defer l.Close()

	blk := buildLsigBlock(b, l, program, arg, 0)

	backlogPool := execpool.MakeBacklog(nil, 0, execpool.LowPriority, nil)
	defer backlogPool.Shutdown()
	// GetMockedCache(false) reports every group as unverified, so each iteration
	// re-runs the LogicSigs instead of hitting the ledger's verification cache.
	cache := verify.GetMockedCache(false)

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := eval.Eval(context.Background(), l, blk, true, cache, backlogPool, nil)
		require.NoError(b, err)
	}
	b.StopTimer()
	b.ReportMetric(float64(b.Elapsed().Nanoseconds())/float64(b.N*benchLsigsPerBlock), "ns/tx")
}

// benchmarkLsigFullBlock measures Validate + AddValidatedBlock + commit for a
// chain of b.N blocks. The chain is built (and applied to a scratch ledger) up
// front, then replayed against a fresh ledger inside the timed region.
func benchmarkLsigFullBlock(b *testing.B, program, arg []byte) {
	// Both ledgers must share a genesis so the pre-built blocks' branch hashes
	// match when replayed on the second ledger.
	genesis := lsigGenesis(b, program)
	scratch := openLsigLedger(b, genesis)
	defer scratch.Close()
	l := openLsigLedger(b, genesis)
	defer l.Close()

	// Build a chain of b.N blocks, advancing the scratch ledger as we go.
	cert := agreement.Certificate{}
	blocks := make([]bookkeeping.Block, 0, b.N)
	for i := 0; i < b.N; i++ {
		blk := buildLsigBlock(b, scratch, program, arg, uint64(i)*benchLsigsPerBlock)
		require.NoError(b, scratch.AddBlock(blk, cert))
		scratch.WaitForCommit(blk.Round())
		blocks = append(blocks, blk)
	}

	backlogPool := execpool.MakeBacklog(nil, 0, execpool.LowPriority, nil)
	defer backlogPool.Shutdown()

	b.ReportAllocs()
	b.ResetTimer()
	for _, blk := range blocks {
		vb, err := l.Validate(context.Background(), blk, backlogPool)
		require.NoError(b, err)
		require.NoError(b, l.AddValidatedBlock(*vb, cert))
		l.WaitForCommit(blk.Round())
	}
	b.StopTimer()
	b.ReportMetric(float64(b.Elapsed().Nanoseconds())/float64(b.N*benchLsigsPerBlock), "ns/tx")
}

// lsigGenesis builds a genesis state that funds the contract account that
// program authorizes. The same state must be reused to open ledgers that will
// replay the same blocks, since the genesis hash is random per call.
func lsigGenesis(b *testing.B, program []byte) ledgercore.InitState {
	genesisInitState, _, _ := ledgertesting.GenesisWithProto(10, protocol.ConsensusFuture)
	contractAddr := basics.Address(logic.HashProgram(program))
	genesisInitState.Accounts[contractAddr] = basics.AccountData{
		MicroAlgos: basics.MicroAlgos{Raw: 1_000_000_000_000},
		Status:     basics.NotParticipating,
	}
	return genesisInitState
}

// openLsigLedger opens an in-memory ledger from the given genesis state.
func openLsigLedger(b *testing.B, genesisInitState ledgercore.InitState) *Ledger {
	log := logging.TestingLog(b)
	log.SetLevel(logging.Error)
	cfg := config.GetDefaultLocal()
	cfg.Archival = true

	dbName := filepath.Join(b.TempDir(), "ledger")
	l, err := OpenLedger(log, dbName, true, genesisInitState, cfg)
	require.NoError(b, err)
	return l
}

// buildLsigBlock builds, but does not apply, a block of benchLsigsPerBlock
// contract-account LogicSig payments starting at l's next round. noteBase keeps
// txids distinct across a chain of blocks.
func buildLsigBlock(b *testing.B, l *Ledger, program, arg []byte, noteBase uint64) bookkeeping.Block {
	contractAddr := basics.Address(logic.HashProgram(program))
	genesisHash := l.GenesisHash()

	prevHdr, err := l.BlockHdr(l.Latest())
	require.NoError(b, err)
	nextHdr := bookkeeping.MakeBlock(prevHdr).BlockHeader
	bev, err := l.StartEvaluator(nextHdr, 0, 0, nil)
	require.NoError(b, err)

	minFee := config.Consensus[protocol.ConsensusFuture].MinFee()
	for i := 0; i < benchLsigsPerBlock; i++ {
		var note [8]byte
		binary.BigEndian.PutUint64(note[:], noteBase+uint64(i))
		txn := transactions.Transaction{
			Type: protocol.PaymentTx,
			Header: transactions.Header{
				Sender:      contractAddr,
				Fee:         minFee,
				FirstValid:  bev.Round(),
				LastValid:   bev.Round(),
				GenesisHash: genesisHash,
				Note:        note[:],
			},
			PaymentTxnFields: transactions.PaymentTxnFields{
				Receiver: contractAddr,
			},
		}
		stxn := transactions.SignedTxn{
			Txn:  txn,
			Lsig: transactions.LogicSig{Logic: program, Args: [][]byte{arg}},
		}
		require.NoError(b, bev.TransactionGroup(stxn.WithAD()))
	}

	ub, err := bev.GenerateBlock(nil)
	require.NoError(b, err)
	blk := ub.UnfinishedBlock()
	// Validation and apply require a proposer when payouts are enabled;
	// agreement would normally supply it. Use the fee sink, and a zero payout.
	proposer := blk.BlockHeader.FeeSink
	return blk.WithProposer(committee.Seed(proposer), proposer, true)
}

// maxFibN returns the largest fib input N whose program fits within the
// protocol's LogicSigMaxCost budget. Cost is monotonic in N, so a binary search
// finds the boundary where evaluation starts exceeding the budget.
func maxFibN(b *testing.B, program []byte, proto config.ConsensusParams) int {
	b.Helper()

	fits := func(n int) bool {
		pass, _, err := logic.EvalSignatureProgram(program, [][]byte{be32(n)}, 0, lsigEvalParams(proto))
		return err == nil && pass
	}

	lo, hi := 0, 1
	for fits(hi) {
		lo = hi
		hi *= 2
		if hi > 1<<24 {
			b.Fatalf("no fib input exceeds the LogicSig budget %d", proto.LogicSigMaxCost)
		}
	}
	for hi-lo > 1 {
		mid := lo + (hi-lo)/2
		if fits(mid) {
			lo = mid
		} else {
			hi = mid
		}
	}
	return lo
}

// lsigEvalParams builds signature-mode EvalParams for a single dummy
// transaction, so the pooled LogicSig budget is allocated as it is in a real
// single-transaction group.
func lsigEvalParams(proto config.ConsensusParams) *logic.EvalParams {
	var txn transactions.SignedTxn
	txn.Lsig.Logic = []byte{1} // non-empty, so NewSigEvalParams pools the budget
	return logic.NewSigEvalParams([]transactions.SignedTxn{txn}, &proto, &logic.NoHeaderLedger{})
}

// be32 encodes n as a 4-byte big-endian integer, the format both the TEAL
// (`btoi`) and WASM (fib.wasm) programs read.
func be32(n int) []byte {
	buf := make([]byte, 4)
	binary.BigEndian.PutUint32(buf, uint32(n))
	return buf
}
