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

package eval

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
	"github.com/algorand/go-algorand/data/bookkeeping"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/data/transactions/logic"
	ledgertesting "github.com/algorand/go-algorand/ledger/testing"
	"github.com/algorand/go-algorand/protocol"
)

// TestWasmInnerSubmitCost pairs identical prepared groups with/without submit,
// using production COW/Perform rather than the logic package's mock ledger.
// The child is discarded after EVERY evaluation; asset creation/transfer cannot
// accumulate between samples. Timing excludes child/params construction and
// recycling, but includes the full Rust->C->Go submit and ledger apply path.
// Requires cargo to generate fixtures, outside timing. Run after make wasmlib:
//
// WASM_OPCOST=1 go test ./ledger/eval -run '^TestWasmInnerSubmitCost$' -v -count=3
func TestWasmInnerSubmitCost(t *testing.T) {
	if os.Getenv("WASM_OPCOST") == "" {
		t.Skip("set WASM_OPCOST=1 to measure production-ledger WASM inner submits")
	}
	previous := runtime.GOMAXPROCS(1)
	defer runtime.GOMAXPROCS(previous)
	dir := t.TempDir()
	out, err := exec.Command("cargo", "build", "--quiet", "--locked", "--manifest-path", "../../wasm/Cargo.toml", "--example", "wat2wasm").CombinedOutput()
	require.NoError(t, err, "%s", out)
	compiler, err := filepath.Abs("../../wasm/target/debug/examples/wat2wasm")
	require.NoError(t, err)
	genesis, addrs, _ := ledgertesting.Genesis(10)
	creator := genesis.Accounts[addrs[0]]
	creator.AppParams = map[basics.AppIndex]basics.AppParams{888: {}}
	creator.AssetParams = map[basics.AssetIndex]basics.AssetParams{777: {Total: 1_000_000}, 778: {Total: 1_000_000}}
	genesis.Accounts[addrs[0]] = creator
	receiver := genesis.Accounts[addrs[1]]
	receiver.Assets = map[basics.AssetIndex]basics.AssetHolding{777: {}}
	genesis.Accounts[addrs[1]] = receiver
	addr := basics.AppIndex(888).Address()
	genesis.Accounts[addr] = basics.AccountData{
		MicroAlgos: basics.MicroAlgos{Raw: 1_000_000_000},
		Assets:     map[basics.AssetIndex]basics.AssetHolding{777: {Amount: 1000}},
	}
	l := newTestLedger(t, bookkeeping.GenesisBalances{Balances: genesis.Accounts, FeeSink: testSinkAddr, RewardsPool: testPoolAddr})
	bh, err := l.BlockHdr(0)
	require.NoError(t, err)
	block := bookkeeping.MakeBlock(bh)
	evaluator, err := l.StartEvaluator(block.BlockHeader, 0, 0, nil)
	require.NoError(t, err)
	proto := evaluator.proto
	proto.EnableWasmApp = true
	proto.MaxAppProgramCost = 1_000_000_000
	fmt.Printf("\n%-22s %10s %12s %12s\n", "production submit", "AVM cost", "ns/submit", "ns/txn")
	for _, kind := range []string{"create", "transfer", "optin"} {
		for _, width := range []int{1, 2, 4, 8, 16} {
			if width > proto.MaxTxGroupSize {
				continue
			}
			t.Run(fmt.Sprintf("%s_%d", kind, width), func(t *testing.T) {
				destination := addr
				if kind == "transfer" {
					destination = addrs[1]
				}
				data := ""
				for _, b := range destination {
					data += fmt.Sprintf("\\%02x", b)
				}
				fields := fmt.Sprintf("(call $uint (i32.const %d) (i64.const 3)) (call $uint (i32.const %d) (i64.const 1000))", logic.TypeEnum, logic.ConfigAssetTotal)
				if kind != "create" {
					asset, amount := 777, 1
					if kind == "optin" {
						asset, amount = 778, 0
					}
					fields = fmt.Sprintf(`(call $uint (i32.const %d) (i64.const 4))
 (call $uint (i32.const %d) (i64.const %d))
 (call $uint (i32.const %d) (i64.const %d))
 (call $bytes (i32.const %d) (i32.const 1024) (i32.const 32))`, logic.TypeEnum, logic.XferAsset, asset, logic.AssetAmount, amount, logic.AssetReceiver)
				}
				fields += fmt.Sprintf("(call $uint (i32.const %d) (i64.const 0))", logic.Fee)
				setup := "(call $begin)" + fields + strings.Repeat("(call $next)"+fields, width-1)
				wat := fmt.Sprintf(`(module
 (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
 (import "env" "itxn_begin" (func $begin))
 (import "env" "itxn_next" (func $next))
 (import "env" "itxn_field_uint" (func $uint (param i32 i64)))
 (import "env" "itxn_field_bytes" (func $bytes (param i32 i32 i32)))
 (import "env" "itxn_submit" (func $submit))
 (memory (export "memory") 1) (data (i32.const 1024) "%s")
 (func (export "program") (result i32)
 (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 0) (i32.const 1)))
 %s
 (if (i32.load8_u (i32.const 0)) (then (call $submit)))
 (i32.const 1)))`, data, setup)
				input, output := filepath.Join(dir, "submit.wat"), filepath.Join(dir, "submit.wasm")
				require.NoError(t, os.WriteFile(input, []byte(wat), 0600))
				out, err := exec.Command(compiler, input, output).CombinedOutput()
				require.NoError(t, err, "%s", out)
				program, err := os.ReadFile(output)
				require.NoError(t, err)
				measure := func(submit byte) (int64, int) {
					group := []transactions.SignedTxnWithAD{{SignedTxn: transactions.SignedTxn{Txn: transactions.Transaction{
						Type:                     protocol.ApplicationCallTx,
						Header:                   transactions.Header{Sender: addrs[0], Fee: basics.MicroAlgos{Raw: 100_000}, FirstValid: block.Round(), LastValid: block.Round()},
						ApplicationCallTxnFields: transactions.ApplicationCallTxnFields{ApplicationID: 888, ForeignAssets: []basics.AssetIndex{777, 778}, Accounts: []basics.Address{addrs[1]}, ApplicationArgs: [][]byte{{submit}}},
					}}}}
					ep := logic.NewAppEvalParams(group, &proto, &evaluator.specials)
					child := evaluator.state.child(1)
					ep.Ledger, ep.SigLedger = child, child
					start := time.Now()
					pass, cx, err := logic.EvalContract(program, 0, 888, ep)
					ns := time.Since(start).Nanoseconds()
					require.NoError(t, err)
					require.True(t, pass)
					require.Len(t, ep.TxnGroup[0].EvalDelta.InnerTxns, int(submit)*width)
					if submit == 1 && kind == "transfer" {
						holding, err := child.AssetHolding(addrs[1], 777)
						require.NoError(t, err)
						require.Equal(t, uint64(width), holding.Amount)
					}
					cost := cx.Cost()
					child.recycle()
					return ns, cost
				}
				measure(0)
				measure(1)
				const iters = 4000
				var total int64
				var cost int
				for i := 0; i < iters; i++ {
					var base, work int64
					var baseCost, workCost int
					if i%2 == 0 {
						base, baseCost = measure(0)
						work, workCost = measure(1)
					} else {
						work, workCost = measure(1)
						base, baseCost = measure(0)
					}
					total += work - base
					cost = workCost - baseCost
				}
				ns := float64(total) / iters
				fmt.Printf("%-22s %10d %12.1f %12.1f\n", fmt.Sprintf("%s_%d", kind, width), cost, ns, ns/float64(width))
			})
		}
	}
}
