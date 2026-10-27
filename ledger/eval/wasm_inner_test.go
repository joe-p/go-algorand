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
	"os"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/crypto"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/bookkeeping"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/data/transactions/logic"
	ledgertesting "github.com/algorand/go-algorand/ledger/testing"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

// Uses real ledger COW and minimum-balance checks, not the logic test ledger.
func TestWasmInnerAssetAtomicity(t *testing.T) {
	partitiontest.PartitionTest(t)
	for _, tc := range []struct {
		name    string
		args    int
		balance uint64
		wantErr string
		fixture string
	}{
		{"approve", 0, 1_000_000, "", "inner_asset_create.wasm"},
		{"reject", 1, 1_000_000, "rejected", "inner_asset_create.wasm"},
		{"trap", 2, 1_000_000, "unreachable", "inner_asset_create.wasm"},
		{"fuel", 3, 1_000_000, "fuel", "inner_asset_create.wasm"},
		{"minimum balance", 0, 100_000, "below min", "inner_asset_create.wasm"},
		{"log reject", 1, 1_000_000, "rejected", "app_log_decision.wasm"},
		{"log trap", 2, 1_000_000, "unreachable", "app_log_decision.wasm"},
		{"log fuel", 3, 1_000_000, "fuel", "app_log_decision.wasm"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			genesis, addrs, _ := ledgertesting.Genesis(10)
			program, err := os.ReadFile("../../data/transactions/logic/testdata/wasm/" + tc.fixture)
			require.NoError(t, err)
			wasmClear, err := os.ReadFile("../../data/transactions/logic/testdata/wasm/approve.wasm")
			require.NoError(t, err)
			approve, err := logic.AssembleString("#pragma version 6\nint 1")
			require.NoError(t, err)
			creator := genesis.Accounts[addrs[0]]
			creator.AppParams = map[basics.AppIndex]basics.AppParams{
				888: {ApprovalProgram: program, ClearStateProgram: wasmClear},
				889: {ApprovalProgram: approve.Program, ClearStateProgram: approve.Program},
			}
			genesis.Accounts[addrs[0]] = creator
			appAddr := basics.AppIndex(888).Address()
			genesis.Accounts[appAddr] = basics.AccountData{MicroAlgos: basics.MicroAlgos{Raw: tc.balance}}
			l := newTestLedger(t, bookkeeping.GenesisBalances{
				Balances: genesis.Accounts, FeeSink: testSinkAddr, RewardsPool: testPoolAddr,
			})
			bh, err := l.BlockHdr(0)
			require.NoError(t, err)
			block := bookkeeping.MakeBlock(bh)
			eval, err := l.StartEvaluator(block.BlockHeader, 0, 0, nil)
			require.NoError(t, err)
			eval.generate = true
			eval.validate = false
			group := make([]transactions.SignedTxnWithAD, 8)
			for i := range group {
				group[i].Txn = transactions.Transaction{
					Type:                     protocol.ApplicationCallTx,
					Header:                   transactions.Header{Sender: addrs[0], Fee: basics.MicroAlgos{Raw: 2000}, FirstValid: block.Round(), LastValid: block.Round(), GenesisHash: l.GenesisHash(), Note: []byte{byte(i)}},
					ApplicationCallTxnFields: transactions.ApplicationCallTxnFields{ApplicationID: 889},
				}
			}
			group[0].Txn.ApplicationID = 888
			group[0].Txn.ApplicationArgs = make([][]byte, tc.args)
			var tg transactions.TxGroup
			for _, tx := range group {
				tg.TxGroupHashes = append(tg.TxGroupHashes, crypto.HashObj(tx.Txn))
			}
			for i := range group {
				group[i].Txn.Group = crypto.HashObj(tg)
			}
			before, err := eval.state.lookup(appAddr)
			require.NoError(t, err)
			counter := eval.TestingTxnCounter()
			err = eval.TransactionGroup(group...)
			after, lookupErr := eval.state.lookup(appAddr)
			require.NoError(t, lookupErr)
			if tc.wantErr != "" {
				require.ErrorContains(t, err, tc.wantErr)
				require.Equal(t, before, after)
				require.Equal(t, counter, eval.TestingTxnCounter())
				require.Empty(t, eval.block.Payset)
			} else {
				require.NoError(t, err)
				require.Equal(t, uint64(1), after.TotalAssetParams)
				require.Equal(t, uint64(1), after.TotalAssets)
				require.Len(t, eval.block.Payset, 8)
				require.Len(t, eval.block.Payset[0].EvalDelta.InnerTxns, 1)
				require.NotZero(t, eval.block.Payset[0].EvalDelta.InnerTxns[0].ConfigAsset)
			}
		})
	}
}
