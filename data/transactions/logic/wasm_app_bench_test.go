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
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/protocol"
)

// tealGlobalIncSrc is a TEAL application program that increments the uint64
// global "counter" N times, where N is ApplicationArgs[0] (big-endian). It is
// paired with app_global_loop.wasm, which does the same under the wasmi
// executor, so the two can be compared at the same loop count.
const tealGlobalIncSrc = `#pragma version 8
txn ApplicationArgs 0
btoi
store 0
byte "counter"
app_global_get
store 1
loop:
  load 0
  int 0
  ==
  bnz done
  load 1
  int 1
  +
  store 1
  byte "counter"
  load 1
  app_global_put
  load 0
  int 1
  -
  store 0
  b loop
done:
  int 1
`

// tealFibAppSrc is a TEAL application program that computes fib(N) in a loop,
// where N is ApplicationArgs[0] (big-endian). It is the compute-bound
// counterpart to tealGlobalIncSrc, paired with app_fib.wasm. Values are masked
// to 2^62 so `+` cannot overflow.
const tealFibAppSrc = `#pragma version 8
int 0
int 1
txn ApplicationArgs 0
btoi
fib_loop:
  dup
  int 0
  ==
  bnz fib_done
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

// tealFibRecAppSrc is a TEAL application program that computes fib(N)
// recursively with `callsub`, where N is ApplicationArgs[0] (big-endian). It is
// the AVM counterpart to app_fib_rec.wasm, which does the same recursion with
// WASM calls. The result is written to the uint global "fib".
const tealFibRecAppSrc = `#pragma version 8
b main

fib:
  dup
  int 2
  <
  bnz fib_base
  dup
  int 1
  -
  callsub fib
  swap
  int 2
  -
  callsub fib
  +
  retsub
fib_base:
  retsub

main:
  txn ApplicationArgs 0
  btoi
  callsub fib
  store 0
  byte "fib"
  load 0
  app_global_put
  int 1
`

// tealRecDepthAppSrc is a TEAL application program that recurses to depth N
// with `callsub`, where N is ApplicationArgs[0] (big-endian), doing O(1) work
// per frame. It pairs with app_rec_depth.wasm; unlike recursive fib, the call
// count equals the depth, so it measures call-stack growth itself.
const tealRecDepthAppSrc = `#pragma version 8
b main

rec:
  dup
  bz rec_base
  int 1
  -
  callsub rec
  int 1
  +
  retsub
rec_base:
  pop
  int 0
  retsub

main:
  txn ApplicationArgs 0
  btoi
  callsub rec
  store 0
  byte "depth"
  load 0
  app_global_put
  int 1
`

// tealDepthLoopAppSrc is the iterative TEAL baseline for tealRecDepthAppSrc:
// the same N iterations of O(1) work, without nested calls. Its loop body is a
// slightly different instruction mix than the recursive body, so the rec/loop
// gap is an approximate frame-cost estimate rather than an exact subtraction.
const tealDepthLoopAppSrc = `#pragma version 8
txn ApplicationArgs 0
btoi
store 0
int 0
store 1
loop:
  load 0
  bz done
  load 1
  int 1
  +
  store 1
  load 0
  int 1
  -
  store 0
  b loop
done:
  byte "depth"
  load 1
  app_global_put
  int 1
`

// BenchmarkAppGlobalLoop compares the execution cost of a TEAL and a WASM
// application program that each increment the same uint64 global the same
// number of times, run for the largest N that fits a two-app-sized allowance in
// both engines. The cap is lifted only for this microbenchmark: setup plus the
// zero-iteration WASM fixture no longer fits a single 700-unit app budget.
// It drives EvalContract directly on the in-memory test ledger.
func BenchmarkAppGlobalLoop(b *testing.B) {
	proto := makeTestProto(wasmAppProto)
	proto.MaxAppProgramCost *= 2
	tealOps, err := AssembleString(tealGlobalIncSrc)
	require.NoError(b, err)
	wasm := loadWasm(b, "app_global_loop.wasm")

	n := maxGlobalLoops(b, proto, tealOps.Program, wasm)
	b.Run("teal-global-loop", func(b *testing.B) {
		benchmarkAppGlobalLoop(b, tealOps.Program, n, false, proto)
	})
	b.Run("wasm-global-loop", func(b *testing.B) {
		benchmarkAppGlobalLoop(b, wasm, n, true, proto)
	})
}

// maxGlobalLoops returns the largest N that both programs can run within the
// protocol's app budget, and logs each engine's maximum and budget use.
func maxGlobalLoops(b *testing.B, proto *config.ConsensusParams, teal, wasm []byte) int {
	b.Helper()

	maxN := func(program []byte, isWasm bool) int {
		fits := func(n int) bool {
			pass, _, err := EvalContract(program, 0, 888, appLoopParams(proto, n, isWasm))
			return err == nil && pass
		}
		require.True(b, fits(0), "zero-iteration fixture must fit the benchmark budget")
		lo, hi := 0, 1
		for fits(hi) {
			lo = hi
			hi *= 2
			if hi > 1<<20 {
				b.Fatalf("no loop count exceeded the app budget %d", proto.MaxAppProgramCost)
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

	tealMax := maxN(teal, false)
	wasmMax := maxN(wasm, true)
	n := min(tealMax, wasmMax)

	_, tealCx, err := EvalContract(teal, 0, 888, appLoopParams(proto, n, false))
	require.NoError(b, err)
	_, wasmCx, err := EvalContract(wasm, 0, 888, appLoopParams(proto, n, true))
	require.NoError(b, err)
	b.Logf("global loop N=%d: teal max=%d cost=%d/%d, wasm max=%d fuel=%d/%d",
		n, tealMax, tealCx.Cost(), proto.MaxAppProgramCost, wasmMax, wasmCx.wasmFuelUsed, uint64(proto.MaxAppProgramCost)*wasmFuelPerCost)
	return n
}

// benchmarkAppGlobalLoop times one EvalContract of the program at loop count N.
func benchmarkAppGlobalLoop(b *testing.B, program []byte, n int, isWasm bool, proto *config.ConsensusParams) {
	ep := appLoopParams(proto, n, isWasm)
	ledger := ep.Ledger.(*Ledger)

	// Confirm the program actually incremented the global N times before timing.
	pass, _, err := EvalContract(program, 0, 888, ep)
	require.NoError(b, err)
	require.True(b, pass)
	tv, ok, err := ledger.GetGlobal(888, "counter")
	require.NoError(b, err)
	if n == 0 {
		require.False(b, ok)
	} else {
		require.True(b, ok)
		require.Equal(b, basics.TealUintType, tv.Type)
		require.Equal(b, uint64(n), tv.Uint)
	}
	ep.reset()
	ledger.Reset()

	var cost int
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		ep.reset()
		ledger.Reset()
		pass, cx, err := EvalContract(program, 0, 888, ep)
		require.NoError(b, err)
		require.True(b, pass)
		cost = cx.Cost()
	}
	b.StopTimer()
	b.ReportMetric(float64(cost), "cost")
}

// BenchmarkAppFibCrossover compares a TEAL and a WASM application program that
// compute fib(N) in a loop, at several N. Unlike BenchmarkAppGlobalLoop it
// deliberately lifts the app budget, because the point is to locate the
// wall-clock crossover where wasmi's much faster per-iteration execution
// overtakes the AVM's lower fixed per-evaluation overhead. These N values are
// not all reachable within one group's pooled budget, so this is a
// microbenchmark of engine cost, not a projection of valid transactions.
func BenchmarkAppFibCrossover(b *testing.B) {
	tealOps, err := AssembleString(tealFibAppSrc)
	require.NoError(b, err)
	wasm := loadWasm(b, "app_fib.wasm")

	proto := makeTestProto(wasmAppProto)
	proto.MaxAppProgramCost = 1_000_000_000 // lift the cap; see the comment above

	for _, n := range []int{0, 50, 100, 300, 1000} {
		n := n
		b.Run(fmt.Sprintf("teal-fib/N%d", n), func(b *testing.B) {
			benchmarkAppAtN(b, tealOps.Program, n, false, proto)
		})
		b.Run(fmt.Sprintf("wasm-fib/N%d", n), func(b *testing.B) {
			benchmarkAppAtN(b, wasm, n, true, proto)
		})
	}
}

// BenchmarkAppFibRecursive compares a TEAL and a WASM application program that
// compute fib(N) recursively, at several N. Recursive fib has depth N but an
// exponential number of calls, so this shows what deep call stacks cost on a
// realistic divide-and-conquer shape. As with BenchmarkAppFibCrossover the
// budget is lifted: these N values are engine-cost measurements, not valid
// transactions.
func BenchmarkAppFibRecursive(b *testing.B) {
	tealOps, err := AssembleString(tealFibRecAppSrc)
	require.NoError(b, err)
	wasm := loadWasm(b, "app_fib_rec.wasm")

	proto := makeTestProto(wasmAppProto)
	proto.MaxAppProgramCost = 1_000_000_000 // lift the cap; see the comment above

	for _, n := range []int{5, 10, 15, 20} {
		n := n
		b.Run(fmt.Sprintf("teal-fib-rec/N%d", n), func(b *testing.B) {
			benchmarkAppAtN(b, tealOps.Program, n, false, proto)
		})
		b.Run(fmt.Sprintf("wasm-fib-rec/N%d", n), func(b *testing.B) {
			benchmarkAppAtN(b, wasm, n, true, proto)
		})
	}
}

// BenchmarkAppCallDepth isolates how a growing call stack affects performance.
// Every variant does N iterations of O(1) work on the same engine; only the
// shape differs: `*-rec` nests N calls, `*-loop` runs a loop. The gap between
// rec and loop at each N is the approximate per-frame cost (frame push/pop plus
// the call and return happening at all); the bodies are not identical
// instruction sets, so it is not an exact subtraction. The slope of rec across N
// is the marginal cost of depth. Running both engines shows whether AVM
// `callsub` frames and WASMI call frames scale the same way.
//
// WASMI is configured with a recursion limit of 1024 (wasm/src/lib.rs), so
// depths above that are a clean trap rather than a measurement.
func BenchmarkAppCallDepth(b *testing.B) {
	tealRecOps, err := AssembleString(tealRecDepthAppSrc)
	require.NoError(b, err)
	tealLoopOps, err := AssembleString(tealDepthLoopAppSrc)
	require.NoError(b, err)

	proto := makeTestProto(wasmAppProto)
	proto.MaxAppProgramCost = 1_000_000_000 // lift the cap; see the comment above

	variants := []struct {
		name    string
		program []byte
		wasm    bool
	}{
		{"teal-rec", tealRecOps.Program, false},
		{"teal-loop", tealLoopOps.Program, false},
		{"wasm-rec", loadWasm(b, "app_rec_depth.wasm"), true},
		{"wasm-loop", loadWasm(b, "app_depth_loop.wasm"), true},
	}
	for _, n := range []int{16, 64, 256, 512} {
		n := n
		for _, v := range variants {
			v := v
			b.Run(fmt.Sprintf("%s/N%d", v.name, n), func(b *testing.B) {
				benchmarkAppAtN(b, v.program, n, v.wasm, proto)
			})
		}
	}
}

// benchmarkAppAtN times one EvalContract of a program driven by the 4-byte
// input N, reporting its AVM cost (and RAW fuel for WASM programs).
func benchmarkAppAtN(b *testing.B, program []byte, n int, isWasm bool, proto *config.ConsensusParams) {
	ep := appLoopParams(proto, n, isWasm)
	ledger := ep.Ledger.(*Ledger)
	var cost int
	var fuel uint64
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		ep.reset()
		ledger.Reset()
		pass, cx, err := EvalContract(program, 0, 888, ep)
		require.NoError(b, err)
		require.True(b, pass)
		cost = cx.Cost()
		fuel = cx.wasmFuelUsed
	}
	b.StopTimer()
	b.ReportMetric(float64(cost), "cost")
	if isWasm {
		b.ReportMetric(float64(fuel), "fuel")
	}
}

// appLoopParams builds ModeApp EvalParams for an app 888 with room for one
// uint global, carrying the loop count N as ApplicationArgs[0].
func appLoopParams(proto *config.ConsensusParams, n int, isWasm bool) *EvalParams {
	return appLoopParamsSchema(proto, n, isWasm, basics.StateSchema{NumUint: 1})
}

// appLoopParamsSchema is appLoopParams with a caller-chosen global schema.
func appLoopParamsSchema(proto *config.ConsensusParams, n int, isWasm bool, schema basics.StateSchema) *EvalParams {
	var args [][]byte
	if isWasm {
		args = [][]byte{le32(n)}
	} else {
		args = [][]byte{be32(n)}
	}
	return appParamsSchema(proto, args, schema)
}

// appParamsSchema builds ModeApp EvalParams for app 888 with a caller-chosen
// global schema and ApplicationArgs.
func appParamsSchema(proto *config.ConsensusParams, args [][]byte, schema basics.StateSchema) *EvalParams {
	txn := makeSampleTxn()
	txn.Txn.Type = protocol.ApplicationCallTx
	txn.Txn.ApplicationID = 888
	txn.Txn.ApplicationArgs = args
	ep := NewAppEvalParams(transactions.WrapSignedTxnsWithAD([]transactions.SignedTxn{txn}), proto, &transactions.SpecialAddresses{})
	ledger := NewLedger(nil)
	// The creator is not consulted by global state access, so a zero address is
	// fine here.
	ledger.NewApp(basics.Address{}, 888, basics.AppParams{StateSchemas: basics.StateSchemas{
		GlobalStateSchema: schema,
	}})
	ep.Ledger = ledger
	ep.SigLedger = ledger
	return ep
}

// be32 encodes n as a 4-byte big-endian integer, the format TEAL's btoi reads.
func be32(n int) []byte {
	buf := make([]byte, 4)
	binary.BigEndian.PutUint32(buf, uint32(n))
	return buf
}

// le32 encodes n as a 4-byte little-endian integer, the format
// app_global_loop.wasm reads with i32.load.
func le32(n int) []byte {
	buf := make([]byte, 4)
	binary.LittleEndian.PutUint32(buf, uint32(n))
	return buf
}
