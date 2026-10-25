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
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/crypto"
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

// tealBitNotSrc is representative of the strict worst-case TEAL family found
// by the opcode sweep: bitwise-not a 4096-byte value. b~ has a fixed cost of 4,
// but copies and complements 4 KiB, so its wall time per charged cost is far
// above the fib loop. bzero and concat land in the same family; the block-level
// comparison in TestTEALPatternBlocks checks all three. The loop stops when the
// remaining budget can no longer afford another iteration.
const tealBitNotSrc = `#pragma version 8
int 4096
bzero
loop:
  dup
  b~
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
pop
int 1
`

// tealCheapSrc is a minimal approving LogicSig used only to find how many
// transactions fit in a block without paying the worst-case execution cost
// during block construction.
const tealCheapSrc = `#pragma version 8
int 1
`

// TestTEALWorstCase measures the strict TEAL ceiling on this machine: the
// largest legal block of worst-case LogicSigs. It is deliberately not a normal
// unit test; run it explicitly:
//
//	go test ./ledger -run '^TestTEALWorstCase$' -v -timeout=0
func TestTEALWorstCase(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run adversarial block measurements")
	}
	if testing.Short() {
		t.Skip("adversarial block measurement skipped in short mode")
	}

	proto := config.Consensus[protocol.ConsensusFuture]
	require.True(t, proto.EnableWasmLogicSig, "vFuture must enable WASM LogicSigs")

	cheapOps, err := logic.AssembleString(tealCheapSrc)
	require.NoError(t, err)
	worstOps, err := logic.AssembleString(tealBitNotSrc)
	require.NoError(t, err)
	t.Logf("program bytes: cheap=%d worst=%d", len(cheapOps.Program), len(worstOps.Program))

	// Find the maximum transaction count with the cheap program. The worst
	// program is only slightly larger, so this is an upper bound on the count.
	maxN, maxBytes := maxLsigBlockSize(t, cheapOps.Program, proto)
	t.Logf("max block with cheap lsig: txns=%d bytes=%d (limit=%d) rss=%s",
		maxN, maxBytes, proto.MaxTxnBytesPerBlock, peakRSS())

	// Measure one worst-case evaluation in isolation.
	perTxn, cost := measureOneSig(t, worstOps.Program, proto)
	t.Logf("worst TEAL per txn: %s cost=%d ns/cost=%.2f", perTxn, cost, float64(perTxn.Nanoseconds())/float64(cost))

	// Build and validate a moderate block to check parallel scaling and GC
	// behavior, then extrapolate to the max transaction count.
	scaled := 1000
	if scaled > maxN {
		scaled = maxN
	}
	blockTime := measureLsigBlock(t, worstOps.Program, proto, scaled)
	t.Logf("worst TEAL block: txns=%d wall=%s extrapolated-max=%s rss=%s",
		scaled, blockTime, time.Duration(int64(blockTime)/int64(scaled)*int64(maxN)), peakRSS())

	// If the full block is feasible, measure it too. It may take a long time.
	if os.Getenv("TEAL_MAX_FULL") != "" {
		l, fullN, fullBlk, buildTime := buildFullLsigBlock(t, worstOps.Program, proto)
		defer l.Close()
		t.Logf("built worst TEAL full block: txns=%d build=%s bytes=%d rss=%s",
			fullN, buildTime, paysetBytes(fullBlk), peakRSS())
		fullTime := validateBlock(t, l, fullBlk)
		t.Logf("worst TEAL full block: txns=%d wall=%s ns/txn=%.0f rss=%s",
			fullN, fullTime, float64(fullTime.Nanoseconds())/float64(fullN), peakRSS())
	}
}

// maxLsigBlockSize returns the largest number of minimal LogicSig payments that
// fit a block and the encoded payset size.
func maxLsigBlockSize(t testing.TB, program []byte, proto config.ConsensusParams) (int, int) {
	t.Helper()
	l := openAdversarialLedger(t, program)
	defer l.Close()

	bev, err := l.StartEvaluator(nextHeader(t, l), 0, 0, nil)
	require.NoError(t, err)

	addr := basics.Address(logic.HashProgram(program))
	minFee := basics.MicroAlgos{Raw: proto.MinTxnFee}

	n := 0
	for {
		txn := transactions.Transaction{
			Type: protocol.PaymentTx,
			Header: transactions.Header{
				Sender:      addr,
				Fee:         minFee,
				FirstValid:  bev.Round(),
				LastValid:   bev.Round(),
				GenesisHash: l.GenesisHash(),
			},
			PaymentTxnFields: transactions.PaymentTxnFields{
				Receiver: addr,
				Amount:   basics.MicroAlgos{Raw: uint64(n)},
			},
		}
		stxn := transactions.SignedTxn{Txn: txn, Lsig: transactions.LogicSig{Logic: program}}
		err = bev.TransactionGroup(stxn.WithAD())
		if errors.Is(err, ledgercore.ErrNoSpace) {
			break
		}
		require.NoError(t, err)
		n++
		if n > 1_000_000 {
			t.Fatalf("block size limit not reached")
		}
	}

	ub, err := bev.GenerateBlock(nil)
	require.NoError(t, err)
	blk := ub.UnfinishedBlock()
	txBytes := 0
	for _, txib := range blk.Payset {
		txBytes += txib.GetEncodedLength()
	}
	return n, txBytes
}

// measureOneSig times one LogicSig evaluation with the protocol's full
// unpooled budget and returns its wall time and charged cost.
func measureOneSig(t testing.TB, program []byte, proto config.ConsensusParams) (time.Duration, int) {
	t.Helper()
	const runs = 5
	var cost int
	start := time.Now()
	for i := 0; i < runs; i++ {
		pass, cx, err := logic.EvalSignatureProgram(program, nil, 0, adversarialSigParams(proto))
		require.NoError(t, err)
		require.True(t, pass)
		cost = cx.Cost()
	}
	return time.Since(start) / runs, cost
}

// measureLsigBlock builds and validates a block of n worst-case LogicSigs,
// returning the wall time of the validation pass.
func measureLsigBlock(t testing.TB, program []byte, proto config.ConsensusParams, n int) time.Duration {
	t.Helper()
	l := openAdversarialLedger(t, program)
	defer l.Close()

	blk := buildMaxLsigBlock(t, l, program, proto, n)
	return validateBlock(t, l, blk)
}

// validateBlock times one validation pass over blk on the ledger that built it.
func validateBlock(t testing.TB, l *Ledger, blk bookkeeping.Block) time.Duration {
	t.Helper()
	backlogPool := execpool.MakeBacklog(nil, 0, execpool.LowPriority, nil)
	defer backlogPool.Shutdown()
	cache := verify.GetMockedCache(false)

	start := time.Now()
	_, err := eval.Eval(context.Background(), l, blk, true, cache, backlogPool, nil)
	require.NoError(t, err)
	return time.Since(start)
}

// buildFullLsigBlock builds the largest block of program transactions the
// ledger will accept, using the real execution path for every transaction.
func buildFullLsigBlock(t testing.TB, program []byte, proto config.ConsensusParams) (*Ledger, int, bookkeeping.Block, time.Duration) {
	t.Helper()
	l := openAdversarialLedger(t, program)

	bev, err := l.StartEvaluator(nextHeader(t, l), 0, 0, nil)
	require.NoError(t, err)
	addr := basics.Address(logic.HashProgram(program))
	minFee := basics.MicroAlgos{Raw: proto.MinTxnFee}

	start := time.Now()
	n := 0
	for {
		txn := transactions.Transaction{
			Type: protocol.PaymentTx,
			Header: transactions.Header{
				Sender:      addr,
				Fee:         minFee,
				FirstValid:  bev.Round(),
				LastValid:   bev.Round(),
				GenesisHash: l.GenesisHash(),
			},
			PaymentTxnFields: transactions.PaymentTxnFields{
				Receiver: addr,
				Amount:   basics.MicroAlgos{Raw: uint64(n)},
			},
		}
		stxn := transactions.SignedTxn{Txn: txn, Lsig: transactions.LogicSig{Logic: program}}
		err = bev.TransactionGroup(stxn.WithAD())
		if errors.Is(err, ledgercore.ErrNoSpace) {
			break
		}
		require.NoError(t, err)
		n++
		if n > 1_000_000 {
			t.Fatalf("block size limit not reached")
		}
	}
	ub, err := bev.GenerateBlock(nil)
	require.NoError(t, err)
	blk := ub.UnfinishedBlock()
	proposer := blk.BlockHeader.FeeSink
	blk = blk.WithProposer(committee.Seed(proposer), proposer, true)
	return l, n, blk, time.Since(start)
}

func paysetBytes(blk bookkeeping.Block) int {
	n := 0
	for _, txib := range blk.Payset {
		n += txib.GetEncodedLength()
	}
	return n
}

func peakRSS() string {
	data, err := os.ReadFile("/proc/self/status")
	if err != nil {
		return "unknown"
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "VmHWM:") {
			return strings.TrimSpace(strings.TrimPrefix(line, "VmHWM:"))
		}
	}
	return "unknown"
}

type tealAdversarialProgram struct {
	name string
	src  string
}

// tealAdversarialPrograms returns budget-filling TEAL programs, one per
// candidate worst opcode. The list is shared by the single-evaluation sweep and
// the block-level comparison.
func tealAdversarialPrograms() []tealAdversarialProgram {
	return []tealAdversarialProgram{
		{"bitnot-4KiB", `#pragma version 8
int 4096
bzero
loop:
  dup
  b~
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
pop
int 1
`},
		{"bitand-4KiB", `#pragma version 8
int 4096
bzero
int 4096
bzero
loop:
  dup
  dup
  b&
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
pop
pop
int 1
`},
		{"bzero-4KiB", `#pragma version 8
loop:
  int 4096
  bzero
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
int 1
`},
		{"concat-4KiB", `#pragma version 8
int 2048
bzero
int 2048
bzero
loop:
  dup
  dup
  concat
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
pop
pop
int 1
`},
		{"substring3-4KiB", `#pragma version 8
int 4096
bzero
loop:
  dup
  int 0
  int 4096
  substring3
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
pop
int 1
`},
		{"extract3-4KiB", `#pragma version 8
int 4096
bzero
loop:
  dup
  int 0
  int 4096
  extract3
  pop
  global OpcodeBudget
  int 20
  >
  bnz loop
pop
int 1
`},
		{"sha256-4KiB", `#pragma version 8
int 4096
bzero
loop:
  dup
  sha256
  pop
  global OpcodeBudget
  int 40
  >
  bnz loop
pop
int 1
`},
		{"sha512_256-4KiB", `#pragma version 8
int 4096
bzero
loop:
  dup
  sha512_256
  pop
  global OpcodeBudget
  int 50
  >
  bnz loop
pop
int 1
`},
		{"keccak256-4KiB", `#pragma version 8
int 4096
bzero
loop:
  dup
  keccak256
  pop
  global OpcodeBudget
  int 140
  >
  bnz loop
pop
int 1
`},
	}
}

// TestTEALOpcodeWorst ranks budget-filling TEAL patterns by time per charged
// cost. It exists to justify using b~ as the strict TEAL ceiling rather than
// the fib loop or a hash.
//
//	WASM_ADVERSARIAL=1 go test ./ledger -run '^TestTEALOpcodeWorst$' -v
func TestTEALOpcodeWorst(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run the TEAL opcode sweep")
	}
	proto := config.Consensus[protocol.ConsensusFuture]

	sources := tealAdversarialPrograms()

	fmt.Printf("\n%-18s %10s %10s %12s\n", "teal pattern", "cost", "time", "ns/cost")
	for _, c := range sources {
		ops, err := logic.AssembleString(c.src)
		require.NoError(t, err, c.name)
		per, cost := measureOneSig(t, ops.Program, proto)
		fmt.Printf("%-18s %10d %10s %12.2f\n", c.name, cost, per, float64(per.Nanoseconds())/float64(cost))
	}
}

// TestTEALPatternBlocks compares the candidate worst TEAL patterns at block
// level. A 1,000-transaction block is used for each, then extrapolated to that
// program's exact max transaction count.
//
//	WASM_ADVERSARIAL=1 go test ./ledger -run '^TestTEALPatternBlocks$' -v -timeout=0
func TestTEALPatternBlocks(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run TEAL block comparisons")
	}
	proto := config.Consensus[protocol.ConsensusFuture]
	fmt.Printf("\n%-18s %10s %12s %14s\n", "teal pattern", "wall/1000", "ns/txn", "extrapolated-max")
	for _, c := range tealAdversarialPrograms() {
		ops, err := logic.AssembleString(c.src)
		require.NoError(t, err, c.name)
		maxN, _ := maxLsigBlockSize(t, ops.Program, proto)
		wall := measureLsigBlock(t, ops.Program, proto, 1000)
		per := float64(wall.Nanoseconds()) / 1000
		fmt.Printf("%-18s %10s %12.0f %14s\n", c.name, wall, per,
			time.Duration(int64(wall.Nanoseconds())/1000*int64(maxN)))
	}
}

// TestWASMAdversarialBlocks measures the strict max WASM blocks on this
// machine and compares them with the TEAL ceiling. Run:
//
//	WASM_ADVERSARIAL=1 go test ./ledger -run '^TestWASMAdversarialBlocks$' -v -timeout=0
func TestWASMAdversarialBlocks(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run adversarial block measurements")
	}

	proto := config.Consensus[protocol.ConsensusFuture]
	payload := make([]byte, 4096)
	for i := range payload {
		payload[i] = byte(i)
	}

	t.Run("arg-4KiB-grouped", func(t *testing.T) {
		program := loadAdversarialWasm(t, "adv_arg_loop.wasm")
		n := maxPooledArgLoop(t, program, payload, proto, 16)
		l, blk, groups, build := buildMaxArgGroupBlock(t, program, payload, proto, n, 16)
		defer l.Close()
		t.Logf("built: groups=%d txns=%d bytes=%d n=%d build=%s rss=%s",
			groups, len(blk.Payset), paysetBytes(blk), n, build, peakRSS())
		wall := validateBlock(t, l, blk)
		t.Logf("validated: groups=%d txns=%d wall=%s ns/group=%.0f ns/txn=%.0f rss=%s",
			groups, len(blk.Payset), wall,
			float64(wall.Nanoseconds())/float64(groups),
			float64(wall.Nanoseconds())/float64(len(blk.Payset)), peakRSS())
	})

	for _, c := range []struct {
		name string
		file string
		max  int
	}{
		{"memcpy-2MiB", "adv_memcpy_loop.wasm", 5},
		{"memfill-4MiB", "adv_memfill_loop.wasm", 2},
	} {
		t.Run(c.name, func(t *testing.T) {
			program := loadAdversarialWasm(t, c.file)
			args := [][]byte{le32b(c.max)}
			l, blk, build := buildFullSingleBlock(t, program, proto, args)
			defer l.Close()
			t.Logf("built: txns=%d bytes=%d build=%s rss=%s",
				len(blk.Payset), paysetBytes(blk), build, peakRSS())
			wall := validateBlock(t, l, blk)
			t.Logf("validated: txns=%d wall=%s ns/txn=%.0f rss=%s",
				len(blk.Payset), wall,
				float64(wall.Nanoseconds())/float64(len(blk.Payset)), peakRSS())
		})
	}
}

// maxPooledArgLoop finds the largest arg-loop count that fits a pooled
// groupSize-transaction budget with one LogicSig.
func maxPooledArgLoop(t *testing.T, program, payload []byte, proto config.ConsensusParams, groupSize int) int {
	t.Helper()
	fits := func(n int) bool {
		txns := make([]transactions.SignedTxn, groupSize)
		txns[0].Lsig.Logic = program
		ep := logic.NewSigEvalParams(txns, &proto, &logic.NoHeaderLedger{})
		pass, _, err := logic.EvalSignatureProgram(program, [][]byte{le32b(n), payload}, 0, ep)
		return err == nil && pass
	}
	require.True(t, fits(0))
	lo, hi := 0, 1
	for fits(hi) {
		lo = hi
		hi *= 2
		if hi > 1_000_000 {
			t.Fatalf("no loop count exceeded the pooled budget")
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

// buildMaxArgGroupBlock fills a block with groups of groupSize transactions.
// Each group has one 4 KiB-argument WASM LogicSig and groupSize-1 plain
// payments, so the LogicSig gets the group-pooled budget.
func buildMaxArgGroupBlock(t *testing.T, program, payload []byte, proto config.ConsensusParams, n, groupSize int) (*Ledger, bookkeeping.Block, int, time.Duration) {
	t.Helper()
	l, addrs, keys := openAdversarialLedgerAccounts(t, program)
	contract := basics.Address(logic.HashProgram(program))

	bev, err := l.StartEvaluator(nextHeader(t, l), 0, 0, nil)
	require.NoError(t, err)
	minFee := basics.MicroAlgos{Raw: proto.MinTxnFee}

	start := time.Now()
	groups := 0
	for {
		base := groups * groupSize
		txns := make([]transactions.Transaction, groupSize)
		for i := range txns {
			sender := addrs[0]
			amount := uint64(base + i)
			if i == 0 {
				sender = contract
				amount = uint64(base)
			}
			txns[i] = transactions.Transaction{
				Type: protocol.PaymentTx,
				Header: transactions.Header{
					Sender:      sender,
					Fee:         minFee,
					FirstValid:  bev.Round(),
					LastValid:   bev.Round(),
					GenesisHash: l.GenesisHash(),
				},
				PaymentTxnFields: transactions.PaymentTxnFields{
					Receiver: sender,
					Amount:   basics.MicroAlgos{Raw: amount},
				},
			}
		}
		var tg transactions.TxGroup
		for i := range txns {
			tg.TxGroupHashes = append(tg.TxGroupHashes, crypto.Digest(txns[i].ID()))
		}
		gh := crypto.HashObj(tg)
		for i := range txns {
			txns[i].Group = gh
		}

		stxns := make([]transactions.SignedTxn, groupSize)
		for i := range txns {
			if i == 0 {
				stxns[i] = transactions.SignedTxn{
					Txn:  txns[i],
					Lsig: transactions.LogicSig{Logic: program, Args: [][]byte{le32b(n), payload}},
				}
				continue
			}
			stxns[i] = txns[i].Sign(keys[0])
		}
		withAD := make([]transactions.SignedTxnWithAD, groupSize)
		for i := range stxns {
			withAD[i] = stxns[i].WithAD()
		}
		err = bev.TransactionGroup(withAD...)
		if errors.Is(err, ledgercore.ErrNoSpace) {
			break
		}
		require.NoError(t, err)
		groups++
		if groups > 100_000 {
			t.Fatalf("block size limit not reached")
		}
	}
	ub, err := bev.GenerateBlock(nil)
	require.NoError(t, err)
	blk := ub.UnfinishedBlock()
	proposer := blk.BlockHeader.FeeSink
	blk = blk.WithProposer(committee.Seed(proposer), proposer, true)
	return l, blk, groups, time.Since(start)
}

// buildFullSingleBlock builds the largest block of single-transaction WASM
// LogicSigs with the given args.
func buildFullSingleBlock(t *testing.T, program []byte, proto config.ConsensusParams, args [][]byte) (*Ledger, bookkeeping.Block, time.Duration) {
	t.Helper()
	l := openAdversarialLedger(t, program)
	bev, err := l.StartEvaluator(nextHeader(t, l), 0, 0, nil)
	require.NoError(t, err)
	addr := basics.Address(logic.HashProgram(program))
	minFee := basics.MicroAlgos{Raw: proto.MinTxnFee}

	start := time.Now()
	n := 0
	for {
		txn := transactions.Transaction{
			Type: protocol.PaymentTx,
			Header: transactions.Header{
				Sender:      addr,
				Fee:         minFee,
				FirstValid:  bev.Round(),
				LastValid:   bev.Round(),
				GenesisHash: l.GenesisHash(),
			},
			PaymentTxnFields: transactions.PaymentTxnFields{
				Receiver: addr,
				Amount:   basics.MicroAlgos{Raw: uint64(n)},
			},
		}
		stxn := transactions.SignedTxn{Txn: txn, Lsig: transactions.LogicSig{Logic: program, Args: args}}
		err = bev.TransactionGroup(stxn.WithAD())
		if errors.Is(err, ledgercore.ErrNoSpace) {
			break
		}
		require.NoError(t, err)
		n++
		if n > 1_000_000 {
			t.Fatalf("block size limit not reached")
		}
	}
	ub, err := bev.GenerateBlock(nil)
	require.NoError(t, err)
	blk := ub.UnfinishedBlock()
	proposer := blk.BlockHeader.FeeSink
	blk = blk.WithProposer(committee.Seed(proposer), proposer, true)
	return l, blk, time.Since(start)
}

func openAdversarialLedgerAccounts(t testing.TB, program []byte) (*Ledger, []basics.Address, []*crypto.SignatureSecrets) {
	t.Helper()
	genesis, addrs, keys := ledgertesting.GenesisWithProto(10, protocol.ConsensusFuture)
	addr := basics.Address(logic.HashProgram(program))
	genesis.Accounts[addr] = basics.AccountData{
		MicroAlgos: basics.MicroAlgos{Raw: 1_000_000_000_000},
		Status:     basics.NotParticipating,
	}
	log := logging.TestingLog(t)
	log.SetLevel(logging.Error)
	cfg := config.GetDefaultLocal()
	cfg.Archival = true
	dbName := filepath.Join(t.TempDir(), "ledger")
	l, err := OpenLedger(log, dbName, true, genesis, cfg)
	require.NoError(t, err)
	return l, addrs, keys
}

func loadAdversarialWasm(t testing.TB, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join("..", "data", "transactions", "logic", "testdata", "wasm", name))
	require.NoError(t, err)
	return data
}

func le32b(n int) []byte {
	b := make([]byte, 4)
	b[0] = byte(n)
	b[1] = byte(n >> 8)
	b[2] = byte(n >> 16)
	b[3] = byte(n >> 24)
	return b
}

// buildMaxLsigBlock builds a block containing up to n minimal LogicSig
// payments. It fails if the block fills before n transactions are added.
func buildMaxLsigBlock(t testing.TB, l *Ledger, program []byte, proto config.ConsensusParams, n int) bookkeeping.Block {
	t.Helper()
	bev, err := l.StartEvaluator(nextHeader(t, l), n, 0, nil)
	require.NoError(t, err)

	addr := basics.Address(logic.HashProgram(program))
	minFee := basics.MicroAlgos{Raw: proto.MinTxnFee}
	for i := 0; i < n; i++ {
		txn := transactions.Transaction{
			Type: protocol.PaymentTx,
			Header: transactions.Header{
				Sender:      addr,
				Fee:         minFee,
				FirstValid:  bev.Round(),
				LastValid:   bev.Round(),
				GenesisHash: l.GenesisHash(),
			},
			PaymentTxnFields: transactions.PaymentTxnFields{
				Receiver: addr,
				Amount:   basics.MicroAlgos{Raw: uint64(i)},
			},
		}
		stxn := transactions.SignedTxn{Txn: txn, Lsig: transactions.LogicSig{Logic: program}}
		err = bev.TransactionGroup(stxn.WithAD())
		require.NoError(t, err)
	}
	ub, err := bev.GenerateBlock(nil)
	require.NoError(t, err)
	blk := ub.UnfinishedBlock()
	proposer := blk.BlockHeader.FeeSink
	return blk.WithProposer(committee.Seed(proposer), proposer, true)
}

func adversarialSigParams(proto config.ConsensusParams) *logic.EvalParams {
	var txn transactions.SignedTxn
	txn.Lsig.Logic = []byte{1} // non-empty so the pooled budget is allocated
	return logic.NewSigEvalParams([]transactions.SignedTxn{txn}, &proto, &logic.NoHeaderLedger{})
}

func nextHeader(t testing.TB, l *Ledger) bookkeeping.BlockHeader {
	t.Helper()
	prevHdr, err := l.BlockHdr(l.Latest())
	require.NoError(t, err)
	return bookkeeping.MakeBlock(prevHdr).BlockHeader
}

func openAdversarialLedger(t testing.TB, program []byte) *Ledger {
	t.Helper()
	genesis, _, _ := ledgertesting.GenesisWithProto(10, protocol.ConsensusFuture)
	addr := basics.Address(logic.HashProgram(program))
	genesis.Accounts[addr] = basics.AccountData{
		MicroAlgos: basics.MicroAlgos{Raw: 1_000_000_000_000},
		Status:     basics.NotParticipating,
	}
	log := logging.TestingLog(t)
	log.SetLevel(logging.Error)
	cfg := config.GetDefaultLocal()
	cfg.Archival = true
	dbName := filepath.Join(t.TempDir(), "ledger")
	l, err := OpenLedger(log, dbName, true, genesis, cfg)
	require.NoError(t, err)
	return l
}
