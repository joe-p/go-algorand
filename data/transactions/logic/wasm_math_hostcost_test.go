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
	"fmt"
	"math/big"
	"math/bits"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
)

// TestWasmMathHostCallCost measures the real release-executor host path. It
// alternates same-module N=0/N>0 evaluations so setup cancels, retaining loop,
// operand, dispatch, and result-handling work. No Rust/Go callback is involved
// inside these arithmetic imports. Fuel estimates use the existing 1 ns/fuel
// guest-scaffolding assumption, not an isolated measurement of host time.
//
// Build with make wasmlib, then run:
// WASM_OPCOST=1 go test ./data/transactions/logic -run '^TestWasmMathHostCallCost$' -v -count=3
func TestWasmMathHostCallCost(t *testing.T) {
	if os.Getenv("WASM_OPCOST") == "" {
		t.Skip("set WASM_OPCOST=1 to calibrate arithmetic host calls (requires cargo)")
	}
	previous := runtime.GOMAXPROCS(1)
	defer runtime.GOMAXPROCS(previous)
	dir := t.TempDir()
	out, err := exec.Command("cargo", "build", "--quiet", "--locked", "--manifest-path", "../../../wasm/Cargo.toml", "--example", "wat2wasm").CombinedOutput()
	require.NoError(t, err, "%s", out)
	compiler, err := filepath.Abs("../../../wasm/target/debug/examples/wat2wasm")
	require.NoError(t, err)

	type costCase struct {
		name, body, check string
		charge            uint64 // Keep in sync with wasm/src/math.rs.
	}
	cases := []costCase{{name: "control_empty"}}
	wordChecks := func(ptr int, words ...uint64) string {
		result := ""
		for i, word := range words {
			result += fmt.Sprintf("(if (i64.ne (i64.load (i32.const %d)) (i64.const %d)) (then unreachable))\n", ptr+8*i, int64(word))
		}
		return result
	}
	product := func(name string, a, b uint64, ptr int) {
		hi, lo := bits.Mul64(a, b)
		cases = append(cases, costCase{name,
			fmt.Sprintf("(call $mulw (i64.const %d) (i64.const %d) (i32.const %d))", int64(a), int64(b), ptr),
			wordChecks(ptr, hi, lo), 15})
	}
	wide := func(hi, lo uint64) *big.Int {
		return new(big.Int).Or(new(big.Int).Lsh(new(big.Int).SetUint64(hi), 64), new(big.Int).SetUint64(lo))
	}
	divide := func(name string, ah, al, bh, bl uint64, ptr int) {
		q, r := new(big.Int), new(big.Int)
		q.QuoRem(wide(ah, al), wide(bh, bl), r)
		checks := wordChecks(ptr, new(big.Int).Rsh(new(big.Int).Set(q), 64).Uint64(), q.Uint64(),
			new(big.Int).Rsh(new(big.Int).Set(r), 64).Uint64(), r.Uint64())
		cases = append(cases, costCase{name,
			fmt.Sprintf("(call $divmodw (i64.const %d) (i64.const %d) (i64.const %d) (i64.const %d) (i32.const %d))", int64(ah), int64(al), int64(bh), int64(bl), ptr), checks, 25})
	}
	root := func(name string, hi, lo uint64, full bool) {
		call := fmt.Sprintf("(call $sqrt (i64.const %d))", int64(lo))
		charge := uint64(10)
		if full {
			call = fmt.Sprintf("(call $sqrtw (i64.const %d) (i64.const %d))", int64(hi), int64(lo))
			charge = 25
		}
		expected := new(big.Int).Sqrt(wide(hi, lo)).Uint64()
		cases = append(cases, costCase{name, "(local.set $result " + call + ")",
			fmt.Sprintf("(if (i64.ne (local.get $result) (i64.const %d)) (then unreachable))", int64(expected)), charge})
	}
	const max = ^uint64(0)
	product("mulw_zero", 0, max, 1024)
	product("mulw_full", max, max, 1024)
	product("mulw_unaligned", max, max, 1025)
	product("mulw_end", max, max, 65520)
	divide("divmodw_small", 0, 7, 0, 3, 1024)
	divide("divmodw_128_by_64", max, max, 0, max-58, 1024)
	divide("divmodw_128_by_16", max, max, 0, 65521, 1024)
	divide("divmodw_128_by_63", max, max, 0, (1<<62)+57, 1024)
	divide("divmodw_by_one", max, max, 0, 1, 1024)
	divide("divmodw_full", max, max, 1, 1, 1024)
	divide("divmodw_high_divisor", max, max, 1<<63, 1, 1024)
	divide("divmodw_small_dividend", 1, 1, max, max, 1024)
	divide("divmodw_unaligned", max, max, 1, 1, 1025)
	divide("divmodw_end", max, max, 1, 1, 65504)
	for _, n := range []uint64{0, 255, 65535, 1 << 32, max - (1 << 33), max} {
		root(fmt.Sprintf("sqrt_%x", n), 0, n, false)
	}
	root("sqrtw_zero", 0, 0, true)
	root("sqrtw_64", 0, max, true)
	root("sqrtw_65", 1, 0, true)
	root("sqrtw_96", (1<<32)-1, max, true)
	root("sqrtw_near_square", max-1, 0, true)
	root("sqrtw_full", max, max, true)

	const n, samples = 4000, 200
	fmt.Printf("\nmath calibration: %s/%s, %s, GOMAXPROCS=1, N=%d, pairs=%d\n", runtime.GOOS, runtime.GOARCH, runtime.Version(), n, samples)
	fmt.Printf("%-26s %9s %11s %10s %13s\n", "host case", "fuel/iter", "ns/iter", "ns/fuel", "host estimate")
	for i, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			wat := fmt.Sprintf(`(module
 (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
 (import "env" "mulw" (func $mulw (param i64 i64 i32)))
 (import "env" "divmodw" (func $divmodw (param i64 i64 i64 i64 i32)))
 (import "env" "sqrt" (func $sqrt (param i64) (result i64)))
 (import "env" "sqrtw" (func $sqrtw (param i64 i64) (result i64)))
 (memory (export "memory") 1)
 (func (export "program") (result i32) (local $n i32) (local $count i32) (local $result i64)
  (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 0) (i32.const 4)))
  (local.set $count (i32.load (i32.const 0)))
  (local.set $n (local.get $count))
  (block $done (loop $loop
   (br_if $done (i32.eqz (local.get $n)))
   %s
   (local.set $n (i32.sub (local.get $n) (i32.const 1))) (br $loop)))
  (if (local.get $count) (then %s))
  (i32.const 1)))`, c.body, c.check)
			input, output := filepath.Join(dir, fmt.Sprintf("%d.wat", i)), filepath.Join(dir, fmt.Sprintf("%d.wasm", i))
			require.NoError(t, os.WriteFile(input, []byte(wat), 0600))
			out, err := exec.Command(compiler, input, output).CombinedOutput()
			require.NoError(t, err, "%s", out)
			program, err := os.ReadFile(output)
			require.NoError(t, err)
			measure := func(count int) hostCostMeasurement {
				ep := wasmAppParams()
				ep.Trace = nil
				ep.Proto.MaxAppProgramCost = 1_000_000_000
				*ep.PooledApplicationBudget = 1_000_000_000
				ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{le32(count)}
				start := time.Now()
				pass, cx, err := EvalContract(program, 0, 888, ep)
				ns := time.Since(start).Nanoseconds()
				require.NoError(t, err)
				require.True(t, pass)
				return hostCostMeasurement{fuel: float64(cx.wasmFuelUsed), ns: float64(ns)}
			}
			measure(0)
			measure(n)
			var delta hostCostMeasurement
			for j := 0; j < samples; j++ {
				var base, work hostCostMeasurement
				if j%2 == 0 {
					base, work = measure(0), measure(n)
				} else {
					work, base = measure(n), measure(0)
				}
				delta.ns += work.ns - base.ns
				delta.fuel = work.fuel - base.fuel
			}
			fuel, ns := delta.fuel/n, delta.ns/(samples*n)
			// Catch stale static-library links or charge assumptions rather than
			// silently reporting a bogus host estimate. These guest prices include
			// the 10-fuel call operator. Final result checks add <0.02 fuel/iter.
			guestFuel := uint64(10)
			switch {
			case strings.HasPrefix(c.name, "mulw_"), strings.HasPrefix(c.name, "sqrtw_"):
				guestFuel = 23
			case strings.HasPrefix(c.name, "divmodw_"):
				guestFuel = 25
			case strings.HasPrefix(c.name, "sqrt_"):
				guestFuel = 22
			}
			require.InDelta(t, float64(c.charge+guestFuel), fuel, 0.02, "rebuild wasmlib; check host and guest fuel assumptions")
			estimate := ns - (fuel - float64(c.charge))
			fmt.Printf("%-26s %9.3f %11.3f %10.3f %13.3f\n", c.name, fuel, ns, ns/fuel, estimate)
		})
	}
}
