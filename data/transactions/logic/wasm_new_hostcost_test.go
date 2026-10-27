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
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/data/basics"
)

// TestWasmNewHostCallCost calibrates the new imports through EvalContract, not
// mock Rust callbacks. Requires cargo for WAT compilation (outside timing):
//
// WASM_OPCOST=1 go test ./data/transactions/logic -run '^TestWasmNewHostCallCost$' -v -count=3
//
// The same module runs with N=0 and N>0, cancelling compile/instantiate costs.
// Time and fuel include loop scaffolding, as in TestWasmHostCallCost. Small
// bounded operations use more evaluations; results still need repeated runs.
// Only the application budget is lifted. Log/inner limits remain in force.
// The ledger is the in-memory logic test ledger, NOT production ledger/eval.
func TestWasmNewHostCallCost(t *testing.T) {
	if os.Getenv("WASM_OPCOST") == "" {
		t.Skip("set WASM_OPCOST=1 to run the new host-call cost table (requires cargo)")
	}
	previous := runtime.GOMAXPROCS(1)
	defer runtime.GOMAXPROCS(previous)
	dir := t.TempDir()
	build := exec.Command("cargo", "build", "--quiet", "--locked", "--manifest-path", "../../../wasm/Cargo.toml", "--example", "wat2wasm")
	out, err := build.CombinedOutput()
	require.NoError(t, err, "%s", out)
	compiler, err := filepath.Abs("../../../wasm/target/debug/examples/wat2wasm")
	require.NoError(t, err)

	const imports = `
 (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
 (import "env" "txn_uint" (func $txn (param i32) (result i64)))
 (import "env" "gtxn_uint" (func $gtxnu (param i32 i32) (result i64)))
 (import "env" "gtxn_bytes" (func $gtxnb (param i32 i32 i32 i32) (result i32)))
 (import "env" "global_uint" (func $globalu (param i32) (result i64)))
 (import "env" "global_bytes" (func $globalb (param i32 i32 i32) (result i32)))
 (import "env" "asset_balance" (func $balance (param i32 i64 i32) (result i32)))
 (import "env" "asset_unit_name" (func $name (param i64 i32 i32) (result i32)))
 (import "env" "log" (func $log (param i32 i32)))
 (import "env" "itxn_begin" (func $begin))
 (import "env" "itxn_next" (func $next))
 (import "env" "itxn_field_uint" (func $uint (param i32 i64)))
 (import "env" "itxn_field_bytes" (func $bytes (param i32 i32 i32)))
 (import "env" "itxn_submit" (func $submit))
 (import "env" "itxn_created_asset_id" (func $id (result i64)))`
	uintField := func(field TxnField, value uint64) string {
		return fmt.Sprintf("(call $uint (i32.const %d) (i64.const %d))\n", field, value)
	}
	bytesField := func(field TxnField, size int) string {
		return fmt.Sprintf("(call $bytes (i32.const %d) (i32.const 1024) (i32.const %d))\n", field, size)
	}
	create := uintField(TypeEnum, 3) + uintField(Fee, 0) + uintField(ConfigAssetTotal, 1000)
	transfer := uintField(TypeEnum, 4) + uintField(Fee, 0) + uintField(XferAsset, 777) +
		uintField(AssetAmount, 0) + bytesField(AssetReceiver, 32)
	type costCase struct {
		name, setup, body string
		n, iters          int
		charge            uint64 // Current host fuel per iteration, excluding guest instructions.
	}
	cases := []costCase{
		{"control_empty", "", "", 2000, 300, 0},
		{"control_txn_uint", "", fmt.Sprintf("(drop (call $txn (i32.const %d)))", FirstValid), 2000, 300, 125},
		{"gtxn_uint", "", fmt.Sprintf("(drop (call $gtxnu (i32.const 1) (i32.const %d)))", FirstValid), 2000, 300, 125},
		{"gtxn_bytes_32", "", fmt.Sprintf("(drop (call $gtxnb (i32.const 1) (i32.const %d) (i32.const 2048) (i32.const 32)))", Sender), 2000, 300, 225},
		{"gtxn_bytes_550", "", fmt.Sprintf("(drop (call $gtxnb (i32.const 1) (i32.const %d) (i32.const 2048) (i32.const 550)))", Note), 2000, 300, 225},
		{"global_uint", "", fmt.Sprintf("(drop (call $globalu (i32.const %d)))", GroupSize), 2000, 300, 125},
		{"global_bytes_32", "", fmt.Sprintf("(drop (call $globalb (i32.const %d) (i32.const 2048) (i32.const 32)))", CurrentApplicationAddress), 2000, 300, 190},
		{"asset_balance", "", "(drop (call $balance (i32.const 1024) (i64.const 777) (i32.const 2048)))", 2000, 300, 900},
		{"asset_balance_absent", "", "(drop (call $balance (i32.const 1024) (i64.const 778) (i32.const 2048)))", 2000, 300, 900},
		{"asset_unit_name_8", "", "(drop (call $name (i64.const 777) (i32.const 2048) (i32.const 8)))", 2000, 300, 350},
		{"asset_unit_name_len", "", "(drop (call $name (i64.const 777) (i32.const 0) (i32.const 0)))", 2000, 300, 350},
		{"asset_unit_name_absent", "", "(drop (call $name (i64.const 779) (i32.const 2048) (i32.const 8)))", 2000, 300, 350},
		{"itxn_begin", "", "(call $begin)", 1, 40000, 10},
		{"itxn_next", "(call $begin)", "(call $next)", 7, 4000, 10},
		{"itxn_field_uint", "(call $begin)", uintField(ConfigAssetTotal, 1000), 2000, 300, 200},
		{"itxn_result", "(call $begin)" + create + "(call $submit)", "(drop (call $id))", 2000, 300, 125},
	}
	for _, size := range []int{0, 4, 32, 256, 1024} {
		n := 32
		if size > 32 {
			n = 1024 / size
		}
		cases = append(cases, costCase{fmt.Sprintf("log_%d", size), "", fmt.Sprintf("(call $log (i32.const 1024) (i32.const %d))", size), n, 3000, uint64(200 + size)})
		if size <= 550 {
			cases = append(cases, costCase{fmt.Sprintf("itxn_bytes_%d", size), "(call $begin)", bytesField(Note, size), 2000, 300, uint64(200 + size)})
		}
	}
	cases = append(cases, costCase{"itxn_bytes_550", "(call $begin)", bytesField(Note, 550), 2000, 300, 750})
	// Submission is a paired comparison with exactly the same prepared group:
	// N=0 leaves it pending, N=1 submits it. Measure multiple widths because
	// group hashing and ledger work need not scale like a single transaction.
	for _, kind := range []string{"create", "transfer"} {
		for _, width := range []int{1, 2, 4, 8} {
			fields := create
			if kind == "transfer" {
				fields = transfer
			}
			setup := "(call $begin)" + fields + strings.Repeat("(call $next)"+fields, width-1)
			cases = append(cases, costCase{fmt.Sprintf("submit_%s_%d", kind, width), setup, "(call $submit)", 1, 2000, 10})
		}
	}

	fmt.Printf("\n%-25s %9s %11s %10s %13s\n", "new host call", "fuel/iter", "ns/iter", "ns/fuel", "host estimate")
	for i, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			addr := basics.AppIndex(888).Address()
			data := ""
			for _, b := range addr {
				data += fmt.Sprintf("\\%02x", b)
			}
			wat := fmt.Sprintf(`(module %s
 (memory (export "memory") 1)
 (data (i32.const 1024) "%s")
 (func (export "program") (result i32) (local $n i32)
  (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 0) (i32.const 4)))
  (local.set $n (i32.load (i32.const 0)))
  %s
  (block $done (loop $loop
   (br_if $done (i32.eqz (local.get $n)))
   %s
   (local.set $n (i32.sub (local.get $n) (i32.const 1))) (br $loop)))
  (i32.const 1)))`, imports, data, c.setup, c.body)
			input := filepath.Join(dir, fmt.Sprintf("%d.wat", i))
			output := filepath.Join(dir, fmt.Sprintf("%d.wasm", i))
			require.NoError(t, os.WriteFile(input, []byte(wat), 0600))
			out, err := exec.Command(compiler, input, output).CombinedOutput()
			require.NoError(t, err, "%s", out)
			program, err := os.ReadFile(output)
			require.NoError(t, err)

			measure := func(n int) hostCostMeasurement {
				ep := wasmInnerParams()
				ep.Trace = nil
				ep.Proto.MaxAppProgramCost = 1_000_000_000
				*ep.PooledApplicationBudget = 1_000_000_000
				ep.TxnGroup[0].Txn.ApplicationArgs = [][]byte{le32(n)}
				ep.TxnGroup[0].Txn.ForeignAssets = append(ep.TxnGroup[0].Txn.ForeignAssets, 779)
				ep.TxnGroup[1].Txn.Note = make([]byte, 550)
				l := ep.Ledger.(*Ledger)
				l.NewAsset(ep.TxnGroup[0].Txn.Sender, 777, basics.AssetParams{Total: 1000, UnitName: "12345678"})
				l.NewHolding(addr, 777, 1000, false)
				start := time.Now()
				pass, cx, err := EvalContract(program, 0, 888, ep)
				total := time.Since(start).Nanoseconds()
				require.NoError(t, err)
				require.True(t, pass)
				if strings.HasPrefix(c.name, "submit_") {
					if n == 0 {
						require.Empty(t, cx.txn.EvalDelta.InnerTxns)
					} else {
						require.NotEmpty(t, cx.txn.EvalDelta.InnerTxns)
					}
				}
				return hostCostMeasurement{fuel: float64(cx.wasmFuelUsed), ns: float64(total)}
			}
			measure(0)
			measure(c.n)
			var m hostCostMeasurement
			// Interleave baseline and work to reduce drift. Alternate order
			// so neither side consistently pays allocation/GC effects.
			for j := 0; j < c.iters; j++ {
				var base, work hostCostMeasurement
				if j%2 == 0 {
					base, work = measure(0), measure(c.n)
				} else {
					work, base = measure(c.n), measure(0)
				}
				m.ns += work.ns - base.ns
				m.fuel = work.fuel - base.fuel
			}
			m.ns /= float64(c.iters)
			fuel := m.fuel / float64(c.n)
			ns := m.ns / float64(c.n)
			// Match the established calibration: assume non-host guest fuel
			// takes 1 ns/fuel. This is an estimate, not isolated host timing.
			estimate := ns - (fuel - float64(c.charge))
			fmt.Printf("%-25s %9.1f %11.1f %10.3f %13.1f\n", c.name, fuel, ns, ns/fuel, estimate)
		})
	}
}
