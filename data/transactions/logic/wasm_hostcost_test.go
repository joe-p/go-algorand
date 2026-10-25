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
	"encoding/binary"
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

// tealHostLoop matches the work and payloads in the host_*.wat fixtures. Loop
// counts use big-endian encoding for TEAL's btoi, rather than WASM's i32.load.
// Both engines retain their own loop overhead; these are not isolated op times.
func tealHostLoop(t *testing.T, name string, sig bool) []byte {
	t.Helper()
	bodies := map[string]string{
		"txn_uint":             "txn FirstValid\nstore 1",
		"txn_bytes":            "txn Sender\npop",
		"txna_bytes":           "txna ApplicationArgs 0\npop",
		"app_global_get_uint":  "byte \"counter\"\napp_global_get\nstore 1",
		"app_global_get_bytes": "byte \"b\"\napp_global_get\npop",
		"app_global_put_uint":  "load 1\nint 1\n+\nstore 1\nbyte \"counter\"\nload 1\napp_global_put",
		"app_global_put_bytes": "byte \"b\"\nbyte \"x\"\napp_global_put",
		"app_global_del":       "byte \"d\"\napp_global_del",
		"ed25519verify_bare":   "load 2\nload 3\nload 4\ned25519verify_bare\npop",
		"arg":                  "arg 0\npop",
	}
	body, ok := bodies[name]
	require.True(t, ok, "no matched TEAL fixture for %s", name)
	setup := "txna ApplicationArgs 0"
	if sig {
		setup = "arg 0"
	}
	setup += "\nbtoi\nstore 0\n"
	if name == "ed25519verify_bare" {
		setup += "txna ApplicationArgs 1\nstore 2\ntxna ApplicationArgs 2\nstore 3\ntxna ApplicationArgs 3\nstore 4\n"
	}
	ops, err := AssembleString("#pragma version 8\n" + setup + `loop:
load 0
int 0
==
bnz done
` + body + `
load 0
int 1
-
store 0
b loop
done:
int 1
`)
	require.NoError(t, err)
	return ops.Program
}

func hostCostBE(n int) []byte {
	b := make([]byte, 4)
	binary.BigEndian.PutUint32(b, uint32(n))
	return b
}

func printHostCost(name string, wasm, teal hostCostMeasurement, n int, avm string) {
	fuel := wasm.fuel / float64(n)
	ns := wasm.ns / float64(n)
	fmt.Printf("%-22s %10.3f %10.1f %10.4f %10.2f %10.1f %10.2f  %s\n",
		name, fuel, ns, ns/fuel, teal.cost/float64(n), teal.ns/float64(n), wasm.ns/teal.ns, avm)
}

type hostCostMeasurement struct {
	fuel float64
	cost float64
	ns   float64
}

func (m hostCostMeasurement) subtract(base hostCostMeasurement) hostCostMeasurement {
	return hostCostMeasurement{m.fuel - base.fuel, m.cost - base.cost, m.ns - base.ns}
}

// Keep fixture correctness covered even when the timing table is disabled.
func TestTealHostCostFixtures(t *testing.T) {
	pub, priv, err := ed25519.GenerateKey(nil)
	require.NoError(t, err)
	data := []byte("0123456789abcdef0123456789abcdef")
	sig := ed25519.Sign(priv, data)
	proto := makeTestProto(wasmAppProto)
	proto.MaxAppProgramCost = 1_000_000_000
	schema := basics.StateSchema{NumUint: 1, NumByteSlice: 1}
	for _, c := range []struct {
		name string
		cost float64
	}{
		{"txn_uint", 11}, {"txn_bytes", 11}, {"txna_bytes", 11},
		{"app_global_get_uint", 12}, {"app_global_get_bytes", 12},
		{"app_global_put_uint", 16}, {"app_global_put_bytes", 12},
		{"app_global_del", 11}, {"ed25519verify_bare", 1913},
	} {
		t.Run(c.name, func(t *testing.T) {
			program := tealHostLoop(t, c.name, false)
			args := func(n int) [][]byte { return [][]byte{hostCostBE(n), data, sig, pub} }
			base := evalTimedApp(t, program, args(0), proto, schema, 1)
			loop := evalTimedApp(t, program, args(3), proto, schema, 1)
			require.Equal(t, 3*c.cost, loop.cost-base.cost)
			require.Zero(t, loop.fuel)
		})
	}
	t.Run("arg", func(t *testing.T) {
		program := tealHostLoop(t, "arg", true)
		base := evalTimedSig(t, program, hostCostBE(0), nil, 1)
		loop := evalTimedSig(t, program, hostCostBE(3), nil, 1)
		require.Equal(t, float64(33), loop.cost-base.cost)
		require.Zero(t, loop.fuel)
	})
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
// N times and not at all, so fixed per-evaluation setup cancels and the residual
// is the host call plus the enclosing loop. Matched TEAL fixtures use the same
// payloads and work, but their own loop scaffolding. The reported time ratio is
// WASM/TEAL for whole iterations, not isolated opcodes or boundary overhead.
// Global gets and deletes target absent keys. Compare ns/fuel against the
// instruction baseline from the Rust `opcode_cost_table` (~1 ns/fuel) and
// against `ed25519verify_bare`, which is
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

	fmt.Printf("\n%-22s %10s %10s %10s %10s %10s %10s  %s\n", "app host call", "WASM fuel", "WASM ns", "ns/fuel", "TEAL cost", "TEAL ns", "time ratio", "AVM equivalent")
	for _, c := range appCases {
		program := loadWasm(t, c.file)
		appArgs := func(n int) [][]byte {
			if c.args != nil {
				return c.args(n)
			}
			return [][]byte{le32(n)}
		}
		fixed := evalTimedApp(t, program, appArgs(0), proto, schema, c.iters)
		loop := evalTimedApp(t, program, appArgs(c.loopN), proto, schema, c.iters)
		teal := tealHostLoop(t, c.name, false)
		tealArgs := func(n int) [][]byte {
			args := appArgs(n)
			args[0] = hostCostBE(n)
			return args
		}
		tealFixed := evalTimedApp(t, teal, tealArgs(0), proto, schema, c.iters)
		tealLoop := evalTimedApp(t, teal, tealArgs(c.loopN), proto, schema, c.iters)
		printHostCost(c.name, loop.subtract(fixed), tealLoop.subtract(tealFixed), c.loopN, c.avm)
	}

	// Signature-mode host functions run under a LogicSig, not an app call.
	sigProto := func(p *config.ConsensusParams) {
		p.EnableWasmLogicSig = true
		p.LogicSigMaxCost = 1_000_000_000
	}
	sigCases := []hostCostCase{
		{"arg", "host_arg_loop.wasm", 2000, 200, "arg (1)", nil},
		{"arg_count", "host_arg_count_loop.wasm", 2000, 200, "no TEAL equivalent (args indexes an argument)", nil},
	}

	fmt.Printf("\n%-22s %10s %10s %10s %10s %10s %10s  %s\n", "sig host call", "WASM fuel", "WASM ns", "ns/fuel", "TEAL cost", "TEAL ns", "time ratio", "AVM equivalent")
	for _, c := range sigCases {
		program := loadWasm(t, c.file)
		fixed := evalTimedSig(t, program, le32(0), sigProto, c.iters)
		loop := evalTimedSig(t, program, le32(c.loopN), sigProto, c.iters)
		if c.name == "arg_count" {
			m := loop.subtract(fixed)
			fmt.Printf("%-22s %10.3f %10.1f %10.4f %10s %10s %10s  %s\n", c.name,
				m.fuel/float64(c.loopN), m.ns/float64(c.loopN), m.ns/m.fuel, "—", "—", "—", c.avm)
			continue
		}
		teal := tealHostLoop(t, c.name, true)
		tealFixed := evalTimedSig(t, teal, hostCostBE(0), sigProto, c.iters)
		tealLoop := evalTimedSig(t, teal, hostCostBE(c.loopN), sigProto, c.iters)
		printHostCost(c.name, loop.subtract(fixed), tealLoop.subtract(tealFixed), c.loopN, c.avm)
	}
}

// evalTimedApp runs one EvalContract `iters` times with the given
// ApplicationArgs, returning fuel, AVM cost, and average nanoseconds per
// evaluation. TEAL evaluations have zero WASM fuel.
func evalTimedApp(t *testing.T, program []byte, args [][]byte, proto *config.ConsensusParams, schema basics.StateSchema, iters int) hostCostMeasurement {
	t.Helper()
	ep := appParamsSchema(proto, args, schema)
	ledger := ep.Ledger.(*Ledger)
	var totalNs int64
	var lastFuel uint64
	var lastCost int
	for i := 0; i < iters; i++ {
		ep.reset()
		ledger.Reset()
		start := time.Now()
		pass, cx, err := EvalContract(program, 0, 888, ep)
		totalNs += time.Since(start).Nanoseconds()
		require.NoError(t, err)
		require.True(t, pass)
		lastFuel = cx.wasmFuelUsed
		lastCost = cx.Cost()
	}
	return hostCostMeasurement{float64(lastFuel), float64(lastCost), float64(totalNs) / float64(iters)}
}

// evalTimedSig runs one EvalSignatureProgram `iters` times with the encoded loop
// count in arg, returning fuel, AVM cost, and average nanoseconds per evaluation.
func evalTimedSig(t *testing.T, program []byte, arg []byte, opt protoOpt, iters int) hostCostMeasurement {
	t.Helper()
	var totalNs int64
	var lastFuel uint64
	var lastCost int
	for i := 0; i < iters; i++ {
		ep := optSigParams(opt)
		ep.Trace = nil // Do not charge TEAL timing for test-helper debug tracing.
		start := time.Now()
		pass, cx, err := EvalSignatureProgram(program, [][]byte{arg}, 0, ep)
		totalNs += time.Since(start).Nanoseconds()
		require.NoError(t, err)
		require.True(t, pass)
		lastFuel = cx.wasmFuelUsed
		lastCost = cx.Cost()
	}
	return hostCostMeasurement{float64(lastFuel), float64(lastCost), float64(totalNs) / float64(iters)}
}
