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
	"crypto/ed25519"
	"fmt"
	"os"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/data/basics"
)

// hostCostCase describes one host-function cost measurement: the fixture loops a
// single host call `loopN` times (reading the count from ApplicationArgs[0] for
// app mode, or LogicSig arg 0 for signature mode). `args` overrides the
// ApplicationArgs for fixtures that need more than the loop count.
type hostCostCase struct {
	name  string
	file  string
	loopN int
	iters int
	avm   string
	args  func(n int) [][]byte
}

// TestWasmHostCallCost prints a wall-clock/fuel table for every WASM host
// function, measured through the real Go/cgo path. It is the harness for
// calibrating host-call metering: each host function charges a fixed fuel cost,
// so the question is how much wall-clock that fuel buys. Skipped unless
// WASM_OPCOST is set:
//
//	WASM_OPCOST=1 go test ./data/transactions/logic/ -run TestWasmHostCallCost -v
//
// For each fixture the per-call cost is the difference between running the loop
// N times and not at all, so the per-evaluation setup and the loop scaffolding
// are the same in both and the residual is the host call plus the enclosing
// loop. Compare ns/fuel against the instruction baseline from the Rust
// `opcode_cost_table` (~1 ns/fuel) and against `ed25519verify_bare`, which is
// charged 28,500 fuel (2850 AVM budget units) and is the alignment reference.
func TestWasmHostCallCost(t *testing.T) {
	if os.Getenv("WASM_OPCOST") == "" {
		t.Skip("set WASM_OPCOST=1 to run the host-call cost table")
	}

	// A real keypair so ed25519verify_bare does full verification work.
	pub, priv, err := ed25519.GenerateKey(nil)
	require.NoError(t, err)
	data := []byte("0123456789abcdef0123456789abcdef") // 32 bytes, matching the fixture
	sig := ed25519.Sign(priv, data)
	edArgs := func(n int) [][]byte { return [][]byte{le32(n), data, sig, pub} }

	// Wide enough for both a uint and a bytes global.
	schema := basics.StateSchema{NumUint: 1, NumByteSlice: 1}
	proto := makeTestProto(wasmAppProto)
	proto.MaxAppProgramCost = 1_000_000_000

	appCases := []hostCostCase{
		{"txn_uint", "host_txn_uint_loop.wasm", 2000, 200, "txn (1)", nil},
		{"txn_bytes", "host_txn_bytes_loop.wasm", 2000, 200, "txn (1)", nil},
		{"txna_bytes", "host_txna_bytes_loop.wasm", 2000, 200, "txna (1)", nil},
		{"app_global_get_uint", "host_global_get_loop.wasm", 2000, 200, "app_global_get (1)", nil},
		{"app_global_get_bytes", "host_global_get_bytes_loop.wasm", 2000, 200, "app_global_get (1)", nil},
		{"app_global_put_uint", "host_global_put_loop.wasm", 2000, 200, "app_global_put (1)", nil},
		{"app_global_put_bytes", "host_global_put_bytes_loop.wasm", 2000, 200, "app_global_put (1)", nil},
		{"app_global_del", "host_global_del_loop.wasm", 2000, 200, "app_global_del (1)", nil},
		{"ed25519verify_bare", "host_ed25519_loop.wasm", 20, 20, "ed25519verify_bare (1900)", edArgs},
	}

	fmt.Printf("\n%-22s %12s %12s %12s  %s\n", "app host call", "fuel/call", "ns/call", "ns/fuel", "AVM equivalent")
	for _, c := range appCases {
		program := loadWasm(t, c.file)
		appArgs := func(n int) [][]byte {
			if c.args != nil {
				return c.args(n)
			}
			return [][]byte{le32(n)}
		}
		fixedFuel, fixedNs := evalTimedApp(t, program, appArgs(0), proto, schema, c.iters)
		loopFuel, loopNs := evalTimedApp(t, program, appArgs(c.loopN), proto, schema, c.iters)
		fuelPer := (loopFuel - fixedFuel) / float64(c.loopN)
		nsPer := (loopNs - fixedNs) / float64(c.loopN)
		fmt.Printf("%-22s %12.3f %12.1f %12.4f  %s\n", c.name, fuelPer, nsPer, nsPer/fuelPer, c.avm)
	}

	// Signature-mode host functions run under a LogicSig, not an app call.
	sigProto := func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 1_000_000_000
	}
	sigCases := []hostCostCase{
		{"arg", "host_arg_loop.wasm", 2000, 200, "arg (1)", nil},
		{"arg_count", "host_arg_count_loop.wasm", 2000, 200, "args (1)", nil},
	}

	fmt.Printf("\n%-22s %12s %12s %12s  %s\n", "sig host call", "fuel/call", "ns/call", "ns/fuel", "AVM equivalent")
	for _, c := range sigCases {
		program := loadWasm(t, c.file)
		fixedFuel, fixedNs := evalTimedSig(t, program, 0, sigProto, c.iters)
		loopFuel, loopNs := evalTimedSig(t, program, c.loopN, sigProto, c.iters)
		fuelPer := (loopFuel - fixedFuel) / float64(c.loopN)
		nsPer := (loopNs - fixedNs) / float64(c.loopN)
		fmt.Printf("%-22s %12.3f %12.1f %12.4f  %s\n", c.name, fuelPer, nsPer, nsPer/fuelPer, c.avm)
	}
}

// evalTimedApp runs one EvalContract `iters` times with the given
// ApplicationArgs, returning the program's fuel cost and the average
// nanoseconds per evaluation.
func evalTimedApp(t *testing.T, program []byte, args [][]byte, proto *config.ConsensusParams, schema basics.StateSchema, iters int) (fuel, ns float64) {
	t.Helper()
	ep := appParamsSchema(proto, args, schema)
	ledger := ep.Ledger.(*Ledger)
	var totalNs int64
	var lastFuel uint64
	for i := 0; i < iters; i++ {
		ep.reset()
		ledger.Reset()
		start := time.Now()
		pass, cx, err := EvalContract(program, 0, 888, ep)
		totalNs += time.Since(start).Nanoseconds()
		require.NoError(t, err)
		require.True(t, pass)
		lastFuel = cx.wasmFuelUsed
	}
	return float64(lastFuel), float64(totalNs) / float64(iters)
}

// evalTimedSig runs one EvalSignatureProgram `iters` times at loop count n,
// returning the program's fuel cost and the average nanoseconds per evaluation.
func evalTimedSig(t *testing.T, program []byte, n int, opt protoOpt, iters int) (fuel, ns float64) {
	t.Helper()
	var totalNs int64
	var lastFuel uint64
	for i := 0; i < iters; i++ {
		ep := optSigParams(opt)
		start := time.Now()
		pass, cx, err := EvalSignatureProgram(program, [][]byte{le32(n)}, 0, ep)
		totalNs += time.Since(start).Nanoseconds()
		require.NoError(t, err)
		require.True(t, pass)
		lastFuel = cx.wasmFuelUsed
	}
	return float64(lastFuel), float64(totalNs) / float64(iters)
}
