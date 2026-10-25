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

package transactions

import (
	"encoding/base64"
	"encoding/binary"
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/config"
	"github.com/algorand/go-algorand/crypto"
	v2 "github.com/algorand/go-algorand/daemon/algod/api/server/v2"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/data/transactions"
	"github.com/algorand/go-algorand/data/transactions/logic"
	"github.com/algorand/go-algorand/libgoal"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/framework/fixtures"
	"github.com/algorand/go-algorand/test/partitiontest"
)

// wasmFixtureDir locates the committed WASM guest fixtures from this package
// directory (test/e2e-go/features/transactions).
func wasmFixtureDir(t *testing.T) string {
	t.Helper()
	_, thisFile, _, ok := runtime.Caller(0)
	require.True(t, ok)
	root := filepath.Join(filepath.Dir(thisFile), "..", "..", "..", "..")
	return filepath.Join(root, "data", "transactions", "logic", "testdata", "wasm")
}

func readWasmFixture(t *testing.T, dir, name string) []byte {
	t.Helper()
	data, err := os.ReadFile(filepath.Join(dir, name))
	require.NoError(t, err)
	return data
}

// TestWasmContractBudgetAndSize deploys compiler-generated Rust applications
// through algod with unmodified ConsensusFuture limits, and measures the module
// size and budget pooling deployment and execution need.
//
// No benchmark cap is lifted: MaxAppProgramCost stays 700, the only extra
// budget comes from adding app-call transactions to the group (700 each), and
// the simulate requests do not ask for extra opcode budget.
//
// Note that the protocol executes the approval program during creation too, so
// deployment needs the same kind of pooled group as a call.
func TestWasmContractBudgetAndSize(t *testing.T) {
	partitiontest.PartitionTest(t)
	defer fixtures.ShutdownSynchronizedTest(t)

	a := require.New(fixtures.SynchronizedTest(t))

	proto, ok := config.Consensus[protocol.ConsensusFuture]
	a.True(ok)
	a.True(proto.EnableWasmApp)
	proto.AgreementFilterTimeoutPeriod0 = 400 * time.Millisecond
	proto.AgreementFilterTimeout = 400 * time.Millisecond

	var fixture fixtures.RestClientFixture
	fixture.SetConsensus(config.ConsensusProtocols{protocol.ConsensusFuture: proto})
	fixture.Setup(t, filepath.Join("nettemplates", "TwoNodes50EachFuture.json"))
	defer fixture.Shutdown()

	client := fixture.LibGoalClient
	wh, err := client.GetUnencryptedWalletHandle()
	a.NoError(err)
	accountList, err := fixture.GetWalletsSortedByBalance()
	a.NoError(err)
	creator := accountList[0].Address
	fee := proto.MinTxnFee

	dir := wasmFixtureDir(t)
	clearProgram := readWasmFixture(t, dir, "approve.wasm")

	sendOne := func(tx transactions.Transaction) string {
		tx, err = client.FillUnsignedTxTemplate(creator, 0, 0, fee, tx)
		a.NoError(err)
		signed, err := client.SignTransactionWithWallet(wh, nil, tx)
		a.NoError(err)
		round, err := client.CurrentRound()
		a.NoError(err)
		txid, err := client.BroadcastTransaction(signed)
		a.NoError(err)
		a.True(fixture.WaitForAllTxnsToConfirm(round+2, map[string]string{txid: creator}))
		return txid
	}

	// A trivial AVM app supplies cheap pooled-budget grants. Only app-call
	// transactions add budget; payments would not.
	dummyApproval, err := logic.AssembleString("#pragma version 8\nint 1")
	a.NoError(err)
	dummyClear, err := logic.AssembleString("#pragma version 8\nint 1")
	a.NoError(err)
	dummyCreate, err := client.MakeUnsignedAppCreateTx(transactions.NoOpOC, dummyApproval.Program, dummyClear.Program, basics.StateSchema{}, basics.StateSchema{}, nil, libgoal.RefBundle{}, 0)
	a.NoError(err)
	dummyTxID := sendOne(dummyCreate)
	dummyInfo, err := client.PendingTransactionInformation(dummyTxID)
	a.NoError(err)
	a.NotNil(dummyInfo.ApplicationIndex)
	dummyID := basics.AppIndex(*dummyInfo.ApplicationIndex)

	// makeGroup signs a group whose first transaction comes from first and whose
	// remaining n-1 transactions are cheap no-op calls to the dummy app.
	makeGroup := func(first transactions.Transaction, extra int) []transactions.SignedTxn {
		txns := make([]transactions.Transaction, 1+extra)
		txns[0] = first
		for i := 1; i < len(txns); i++ {
			txns[i], err = client.MakeUnsignedAppNoOpTx(dummyID, nil, libgoal.RefBundle{}, 0)
			a.NoError(err)
			// Distinct notes keep the dummy txids unique.
			txns[i].Note = []byte(fmt.Sprintf("wasm-pool-%d", i))
		}
		for i := range txns {
			txns[i], err = client.FillUnsignedTxTemplate(creator, 0, 0, fee, txns[i])
			a.NoError(err)
		}
		gid, err := client.GroupID(txns)
		a.NoError(err)
		signed := make([]transactions.SignedTxn, len(txns))
		for i := range txns {
			txns[i].Group = gid
			signed[i], err = client.SignTransactionWithWallet(wh, nil, txns[i])
			a.NoError(err)
		}
		return signed
	}
	simulate := func(signed []transactions.SignedTxn) v2.PreEncodedSimulateResponse {
		resp, err := client.SimulateTransactions(v2.PreEncodedSimulateRequest{
			TxnGroups: []v2.PreEncodedSimulateRequestTransactionGroup{{Txns: signed}},
		})
		a.NoError(err)
		a.Len(resp.TxnGroups, 1)
		return resp
	}

	// minGroup finds the smallest pooled group whose first transaction executes,
	// and reports the cost of that first transaction.
	minGroup := func(first func() transactions.Transaction) (int, int) {
		one := simulate(makeGroup(first(), 0))
		a.NotNil(one.TxnGroups[0].FailureMessage)
		a.Contains(*one.TxnGroups[0].FailureMessage, "fuel")
		a.NotNil(one.TxnGroups[0].AppBudgetAdded)
		a.Equal(700, *one.TxnGroups[0].AppBudgetAdded)

		var lastFailure string
		for n := 2; n <= proto.MaxTxGroupSize; n++ {
			resp := simulate(makeGroup(first(), n-1))
			if resp.TxnGroups[0].FailureMessage == nil {
				a.NotNil(resp.TxnGroups[0].AppBudgetAdded)
				a.Equal(n*proto.MaxAppProgramCost, *resp.TxnGroups[0].AppBudgetAdded)
				a.NotEmpty(resp.TxnGroups[0].Txns)
				a.NotNil(resp.TxnGroups[0].Txns[0].AppBudgetConsumed)

				// A group one call smaller must fail, so the reported cost is
				// the real requirement rather than slack.
				if n > 2 {
					below := simulate(makeGroup(first(), n-2))
					a.NotNil(below.TxnGroups[0].FailureMessage)
				}
				return n, *resp.TxnGroups[0].Txns[0].AppBudgetConsumed
			}
			lastFailure = *resp.TxnGroups[0].FailureMessage
		}
		t.Fatalf("contract did not fit the maximum pooled group: %s", lastFailure)
		return 0, 0
	}

	type contract struct {
		name     string
		file     string
		args     [][]byte
		schema   basics.StateSchema
		wantKey  string
		wantUint uint64
	}
	var amount [8]byte
	binary.BigEndian.PutUint64(amount[:], 1000)
	var seed crypto.Seed
	for i := range seed {
		seed[i] = 0x42
	}
	issuer := crypto.GenerateSignatureSecrets(seed)
	sig := issuer.SignBytes(amount[:])

	contracts := []contract{
		{
			name: "rust counter", file: "app_rust_counter.wasm",
			args: [][]byte{amount[:]}, schema: basics.StateSchema{NumUint: 2},
			wantKey: "count", wantUint: 1000,
		},
		{
			name: "rust signed claim", file: "app_rust_signed_claim.wasm",
			args: [][]byte{sig[:], amount[:]}, schema: basics.StateSchema{NumUint: 2},
			wantKey: "total", wantUint: 1000,
		},
	}

	for _, c := range contracts {
		approval := readWasmFixture(t, dir, c.file)
		pages := (len(approval) + proto.MaxAppProgramLen - 1) / proto.MaxAppProgramLen
		epp := uint32(pages - 1)
		a.LessOrEqual(epp, uint32(proto.MaxAbsoluteExtraProgramPages))

		create := func() transactions.Transaction {
			tx, err := client.MakeUnsignedAppCreateTx(transactions.NoOpOC, approval, clearProgram, c.schema, basics.StateSchema{}, nil, libgoal.RefBundle{}, epp)
			a.NoError(err)
			return tx
		}

		// A module larger than its declared pages is rejected. For the
		// single-page counter there is no smaller legal declaration.
		if epp > 0 {
			badCreate := func() transactions.Transaction {
				tx, err := client.MakeUnsignedAppCreateTx(transactions.NoOpOC, approval, clearProgram, c.schema, basics.StateSchema{}, nil, libgoal.RefBundle{}, epp-1)
				a.NoError(err)
				return tx
			}
			bad, err := client.FillUnsignedTxTemplate(creator, 0, 0, fee, badCreate())
			a.NoError(err)
			badSigned, err := client.SignTransactionWithWallet(wh, nil, bad)
			a.NoError(err)
			resp, err := client.SimulateTransactions(v2.PreEncodedSimulateRequest{
				TxnGroups: []v2.PreEncodedSimulateRequestTransactionGroup{{Txns: []transactions.SignedTxn{badSigned}}},
			})
			if err != nil {
				a.Contains(err.Error(), "approval program too long")
			} else {
				a.Len(resp.TxnGroups, 1)
				a.NotNil(resp.TxnGroups[0].FailureMessage)
				a.Contains(*resp.TxnGroups[0].FailureMessage, "approval program too long")
			}
		}

		// Creation runs the approval program, so the deploy transaction also
		// needs a pooled group.
		deployN, deployCost := minGroup(create)
		deployGroup := makeGroup(create(), deployN-1)
		createTxID := deployGroup[0].ID().String()
		round, err := client.CurrentRound()
		a.NoError(err)
		a.NoError(client.BroadcastTransactionGroup(deployGroup))
		a.True(fixture.WaitForAllTxnsToConfirm(round+2, map[string]string{createTxID: creator}))
		info, err := client.PendingTransactionInformation(createTxID)
		a.NoError(err)
		a.NotNil(info.ApplicationIndex)
		appID := basics.AppIndex(*info.ApplicationIndex)

		// Fund the app account so calls do not depend on the app account's
		// minimum balance.
		fund, err := client.SendPaymentFromWallet(wh, nil, creator, appID.Address().String(), 0, 1_000_000, nil, "", 0, 0)
		a.NoError(err)
		round, err = client.CurrentRound()
		a.NoError(err)
		a.True(fixture.WaitForAllTxnsToConfirm(round+2, map[string]string{fund.ID().String(): creator}))

		call := func() transactions.Transaction {
			tx, err := client.MakeUnsignedAppNoOpTx(appID, c.args, libgoal.RefBundle{}, 0)
			a.NoError(err)
			return tx
		}
		callN, callCost := minGroup(call)
		callGroup := makeGroup(call(), callN-1)
		round, err = client.CurrentRound()
		a.NoError(err)
		a.NoError(client.BroadcastTransactionGroup(callGroup))
		a.True(fixture.WaitForAllTxnsToConfirm(round+2, map[string]string{
			callGroup[0].ID().String(): creator,
		}))

		appInfo, err := client.ApplicationInformation(appID)
		a.NoError(err)
		a.NotNil(appInfo.Params.GlobalState)
		found := false
		for _, kv := range *appInfo.Params.GlobalState {
			if kv.Key == base64.StdEncoding.EncodeToString([]byte(c.wantKey)) {
				a.Equal(c.wantUint, kv.Value.Uint)
				found = true
			}
		}
		a.True(found, "expected global key %q", c.wantKey)

		// The sender has already claimed, so a replay is rejected (not a
		// budget failure).
		if c.file == "app_rust_signed_claim.wasm" {
			replay := simulate(makeGroup(call(), callN-1))
			a.NotNil(replay.TxnGroups[0].FailureMessage)
			a.NotContains(*replay.TxnGroups[0].FailureMessage, "fuel")
		}

		t.Logf("%s: bytes=%d pages=%d extraPages=%d deployCalls=%d deployCost=%d callCalls=%d callCost=%d maxPool=%d",
			c.name, len(approval), pages, epp, deployN, deployCost, callN, callCost,
			proto.MaxTxGroupSize*proto.MaxAppProgramCost)
	}
}
