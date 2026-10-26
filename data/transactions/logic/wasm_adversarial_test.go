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
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/crypto"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
)

// TestWasmAdversarialCost measures the worst WASM host-payload and bulk-memory
// programs under a realistic 20,000-unit LogicSig budget. It is the input to
// the block-level comparison against the strict TEAL ceiling.
//
//	WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmAdversarialCost$' -v -count=1
func TestWasmAdversarialCost(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run the adversarial cost table")
	}

	big := make([]byte, 4096)
	for i := range big {
		big[i] = byte(i)
	}

	type tc struct {
		name string
		file string
		args func(n int) [][]byte
		maxN int
	}
	cases := []tc{
		{"arg-4KiB", "adv_arg_loop.wasm", func(n int) [][]byte { return [][]byte{le32(n), big} }, 200_000},
		{"memcpy-2MiB", "adv_memcpy_loop.wasm", func(n int) [][]byte { return [][]byte{le32(n)} }, 100},
		{"memfill-4MiB", "adv_memfill_loop.wasm", func(n int) [][]byte { return [][]byte{le32(n)} }, 100},
	}

	proto := makeTestProto(func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 20_000
	})
	proto.LogicSigMaxCost = 20_000

	fmt.Printf("\n%-16s %10s %10s %10s %12s %10s\n",
		"wasm case", "max N", "cost", "fuel", "ns/eval", "ns/cost")
	for _, c := range cases {
		program := loadWasm(t, c.file)
		maxN := maxLoop(t, program, c.args, proto, c.maxN)
		atMax := timedSig(t, program, c.args(maxN), proto, 3)
		nsPerCost := atMax.ns / float64(atMax.cost)
		fmt.Printf("%-16s %10d %10d %10d %12.1f %10.2f\n",
			c.name, maxN, atMax.cost, atMax.fuel, atMax.ns, nsPerCost)
		require.Greater(t, maxN, 0)
	}
}

// TestWasmStaticValidationCost measures the compile/validate work performed by
// CheckSignature on a max-complexity module. This work is not charged to the
// evaluation budget, so its block-level bound comes only from the block byte
// limit.
//
//	WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmStaticValidationCost$' -v
func TestWasmStaticValidationCost(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run static validation measurements")
	}
	program := loadWasm(t, "adv_many_funcs.wasm")
	ep := optSigParams(wasmProto)
	require.NoError(t, CheckSignature(program, ep))

	// Show that the module is structurally valid but unaffordable to execute.
	pass, cx, err := EvalSignatureProgram(program, nil, 0, optSigParams(func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 20_000
	}))
	require.Error(t, err)
	require.False(t, pass)
	t.Logf("module bytes=%d execution-cost=%d err=%v", len(program), cx.Cost(), err)

	const runs = 200
	start := time.Now()
	for i := 0; i < runs; i++ {
		require.NoError(t, CheckSignature(program, optSigParams(wasmProto)))
	}
	per := time.Since(start) / runs

	proto := config.Consensus[protocol.ConsensusFuture]
	maxPrograms := proto.MaxTxnBytesPerBlock / len(program)
	t.Logf("static validation: bytes=%d per-module=%s per-block-max=%d worst-block-static=%s",
		len(program), per, maxPrograms, per*time.Duration(maxPrograms))
}

// TestWasmFailureAndGrowthCost measures failed executions and maximum memory
// growth. Failed LogicSigs consume their full budget before trapping; failed
// clear-state programs are ignored by the caller but still paid for.
//
//	WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmFailureAndGrowthCost$' -v
func TestWasmFailureAndGrowthCost(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run failure and growth measurements")
	}
	sigProto := makeTestProto(func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 20_000
	})
	sigProto.LogicSigMaxCost = 20_000

	runaway := loadWasm(t, "runaway.wasm")
	const runs = 5
	start := time.Now()
	var cost int
	for i := 0; i < runs; i++ {
		pass, cx, err := EvalSignatureProgram(runaway, nil, 0, sigParams(sigProto))
		require.Error(t, err)
		require.False(t, pass)
		cost = cx.Cost()
	}
	sigNS := float64(time.Since(start).Nanoseconds()) / runs
	t.Logf("failed LSig: cost=%d time=%.0fns ns/cost=%.2f", cost, sigNS, sigNS/float64(cost))

	// Clear-state failure: error is ignored, but the program still consumes
	// its isolated budget.
	txn := makeSampleTxn()
	txn.Txn.Type = protocol.ApplicationCallTx
	txn.Txn.OnCompletion = transactions.ClearStateOC
	start = time.Now()
	for i := 0; i < runs; i++ {
		ep := wasmAppParams(txn)
		pass, cx, err := EvalContract(runaway, 0, 888, ep)
		require.Error(t, err)
		require.False(t, pass)
		cost = cx.Cost()
		require.Equal(t, 700, cost)
	}
	appNS := float64(time.Since(start).Nanoseconds()) / runs
	t.Logf("failed clear-state: cost=%d time=%.0fns ns/cost=%.2f", cost, appNS, appNS/float64(cost))

	// memory.grow from one page to the 4 MiB cap.
	grow := loadWasm(t, "adv_grow.wasm")
	start = time.Now()
	pass, cx, err := EvalSignatureProgram(grow, nil, 0, sigParams(sigProto))
	require.NoError(t, err)
	require.True(t, pass)
	growNS := float64(time.Since(start).Nanoseconds())
	t.Logf("memory.grow to 4 MiB: cost=%d fuel=%d time=%.0fns ns/cost=%.2f",
		cx.Cost(), cx.wasmFuelUsed, growNS, growNS/float64(cx.Cost()))
}

// TestWasmUsefulContractBudget reports whether representative programs fit the
// normal budgets, and how much pooling they need when they do not.
//
//	WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmUsefulContractBudget$' -v
func TestWasmUsefulContractBudget(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run useful-contract budget measurements")
	}
	fmt.Printf("\n%-24s %10s %10s %10s %10s\n", "contract", "cost", "fuel", "budget", "pool calls")
	print := func(name string, cost int, fuel uint64, budget int) {
		pool := (cost + budget - 1) / budget
		fmt.Printf("%-24s %10d %10d %10d %10d\n", name, cost, fuel, budget, pool)
	}

	// Signature-mode ed25519 verifier.
	c := crypto.GenerateSignatureSecrets(randSeed())
	data := []byte("representative ed25519 verification input")
	sig := c.SignBytes(data)
	proto := makeTestProto(func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 20_000
	})
	pass, cx, err := EvalSignatureProgram(loadWasm(t, "ed25519verify_bare.wasm"),
		[][]byte{data, sig[:], c.SignatureVerifier[:]}, 0, sigParams(proto))
	require.NoError(t, err)
	require.True(t, pass)
	print("WASM ed25519 LSig", cx.Cost(), cx.wasmFuelUsed, 20_000)

	tealEd, err := AssembleString(`#pragma version 8
arg 0
arg 1
arg 2
ed25519verify_bare
`)
	require.NoError(t, err)
	pass, tealCx, err := EvalSignatureProgram(tealEd.Program,
		[][]byte{data, sig[:], c.SignatureVerifier[:]}, 0, sigParams(makeTestProto()))
	require.NoError(t, err)
	require.True(t, pass)
	print("TEAL ed25519 LSig", tealCx.Cost(), 0, 20_000)

	// Application programs at a large budget, to expose setup cost separately
	// from the default 700-unit allowance.
	appProto := makeTestProto(wasmAppProto)
	appProto.MaxAppProgramCost = 1_000_000
	schema := basics.StateSchema{NumUint: 1, NumByteSlice: 1}
	for _, c := range []struct {
		name string
		file string
		args [][]byte
	}{
		{"app_global_inc", "app_global_inc.wasm", nil},
		{"app_fib N=0", "app_fib.wasm", [][]byte{le32(0)}},
	} {
		ep := appParamsSchema(appProto, c.args, schema)
		pass, cx, err := EvalContract(loadWasm(t, c.file), 0, 888, ep)
		require.NoError(t, err)
		require.True(t, pass)
		print("WASM "+c.name, cx.Cost(), cx.wasmFuelUsed, 700)
	}

	tealInc, err := AssembleString(`#pragma version 8
byte "counter"
app_global_get
int 1
+
byte "counter"
swap
app_global_put
int 1
`)
	require.NoError(t, err)
	ep := appParamsSchema(makeTestProto(), nil, schema)
	pass, tealCx, err = EvalContract(tealInc.Program, 0, 888, ep)
	require.NoError(t, err)
	require.True(t, pass)
	print("TEAL app_global_inc", tealCx.Cost(), 0, 700)
}

// maxLoop binary-searches the largest loop count whose program still accepts
// within the budget.
func maxLoop(t *testing.T, program []byte, args func(int) [][]byte, proto *config.ConsensusParams, hiMax int) int {
	t.Helper()
	fits := func(n int) bool {
		pass, _, err := EvalSignatureProgram(program, args(n), 0, sigParams(proto))
		return err == nil && pass
	}
	require.True(t, fits(0), "zero-iteration program must fit")
	lo, hi := 0, 1
	for fits(hi) {
		lo = hi
		hi *= 2
		if hi > hiMax {
			return hiMax
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

type sigCost struct {
	cost int
	fuel uint64
	ns   float64
}

// timedSig runs one program count times and returns average wall time and the
// last evaluation's cost/fuel. A warm-up evaluation keeps first-call cgo/Rust
// initialization out of the measurement.
func timedSig(t *testing.T, program []byte, args [][]byte, proto *config.ConsensusParams, count int) sigCost {
	t.Helper()
	_, _, err := EvalSignatureProgram(program, args, 0, sigParams(proto))
	require.NoError(t, err)
	var total int64
	var cost int
	var fuel uint64
	for i := 0; i < count; i++ {
		start := time.Now()
		pass, cx, err := EvalSignatureProgram(program, args, 0, sigParams(proto))
		total += time.Since(start).Nanoseconds()
		require.NoError(t, err)
		require.True(t, pass)
		cost = cx.Cost()
		fuel = cx.wasmFuelUsed
	}
	return sigCost{cost: cost, fuel: fuel, ns: float64(total) / float64(count)}
}

func sigParams(proto *config.ConsensusParams) *EvalParams {
	var txn transactions.SignedTxn
	txn.Lsig.Logic = []byte{1}
	return NewSigEvalParams([]transactions.SignedTxn{txn}, proto, &NoHeaderLedger{})
}

// TestWasmRustContractBudget measures the compiler-generated Rust application
// fixtures at normal protocol limits: 700 units per app call, pooled across a
// real transaction group. It reports setup plus execution under the largest
// legal pool (16 app calls, 11,200 units) and shows that a single call cannot
// pay even the setup charge.
//
//	WASM_ADVERSARIAL=1 go test ./data/transactions/logic -run '^TestWasmRustContractBudget$' -v -count=1
func TestWasmRustContractBudget(t *testing.T) {
	if os.Getenv("WASM_ADVERSARIAL") == "" {
		t.Skip("set WASM_ADVERSARIAL=1 to run the useful-contract budget measurements")
	}

	var amount [8]byte
	binary.BigEndian.PutUint64(amount[:], 1000)
	var seed crypto.Seed
	for i := range seed {
		seed[i] = 0x42
	}
	sig := crypto.GenerateSignatureSecrets(seed).SignBytes(amount[:])

	cases := []struct {
		name   string
		file   string
		args   [][]byte
		schema basics.StateSchema
	}{
		{"rust counter", "app_rust_counter.wasm", [][]byte{amount[:]}, basics.StateSchema{NumUint: 2}},
		{"rust signed claim", "app_rust_signed_claim.wasm", [][]byte{sig[:], amount[:]}, basics.StateSchema{NumUint: 2}},
		{"avm-sdk counter", "app_avm_sdk_counter.wasm", [][]byte{amount[:]}, basics.StateSchema{NumUint: 2}},
	}

	params := func(n int, args [][]byte, schema basics.StateSchema) *EvalParams {
		txns := make([]transactions.SignedTxn, n)
		for i := range txns {
			txns[i].Txn.Type = protocol.ApplicationCallTx
			txns[i].Txn.ApplicationID = 888
		}
		txns[0].Txn.ApplicationArgs = args
		proto := makeTestProto(wasmAppProto)
		ep := NewAppEvalParams(transactions.WrapSignedTxnsWithAD(txns), proto, &transactions.SpecialAddresses{})
		ledger := NewLedger(nil)
		ledger.NewApp(basics.Address{}, 888, basics.AppParams{StateSchemas: basics.StateSchemas{
			GlobalStateSchema: schema,
		}})
		ep.Ledger = ledger
		ep.SigLedger = ledger
		return ep
	}

	fmt.Printf("\n%-20s %8s %10s %10s %8s\n", "contract", "bytes", "cost", "fuel", "app calls")
	for _, c := range cases {
		program := loadWasm(t, c.file)

		// One unpooled call cannot pay the setup charge.
		pass, cx, err := EvalContract(program, 0, 888, params(1, c.args, c.schema))
		require.Error(t, err)
		require.False(t, pass)
		require.Contains(t, err.Error(), "fuel")
		require.Equal(t, 700, cx.Cost())

		// The full legal pool (16 app calls) executes the contract.
		pass, cx, err = EvalContract(program, 0, 888, params(16, c.args, c.schema))
		require.NoError(t, err)
		require.True(t, pass)
		calls := (cx.Cost() + 699) / 700
		require.LessOrEqual(t, calls, 16)
		fmt.Printf("%-20s %8d %10d %10d %8d\n", c.name, len(program), cx.Cost(), cx.wasmFuelUsed, calls)
	}
}
