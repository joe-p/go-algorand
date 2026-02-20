// Copyright (C) 2019-2026 Algorand, Inc.
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
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/algorand/go-algorand/crypto"
	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/protocol"
	"github.com/algorand/go-algorand/test/partitiontest"
)

func TestGroupSizeWithFeePayment(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	const maxGroupSize = 16
	require.Equal(t, 17, MaxGroupSizeWithFeePayment(maxGroupSize))

	group := make([]SignedTxn, maxGroupSize+1)
	require.False(t, IsValidGroupSize(maxGroupSize, len(group), FeePaymentCount(group)))

	group[0].Txn.Type = protocol.FeePaymentTx
	require.True(t, IsValidGroupSize(maxGroupSize, len(group), FeePaymentCount(group)))

	group = append(group, SignedTxn{Txn: Transaction{Type: protocol.FeePaymentTx}})
	require.False(t, IsValidGroupSize(maxGroupSize, len(group), FeePaymentCount(group)))
}

func TestGroupIDIgnoresFeePayment(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	a := Transaction{Type: protocol.PaymentTx, Header: Header{Sender: basics.Address{1}, FirstValid: 1, LastValid: 2}, PaymentTxnFields: PaymentTxnFields{Receiver: basics.Address{2}}}
	b := Transaction{Type: protocol.PaymentTx, Header: Header{Sender: basics.Address{3}, FirstValid: 1, LastValid: 2}, PaymentTxnFields: PaymentTxnFields{Receiver: basics.Address{4}}}
	fpay := Transaction{Type: protocol.FeePaymentTx, Header: Header{Sender: basics.Address{5}, FirstValid: 1, LastValid: 2}}

	withoutFeePay := GroupID([]Transaction{a, b})
	withFeePay := GroupID([]Transaction{a, fpay, b})

	require.Equal(t, withoutFeePay, withFeePay)
	require.Len(t, TxGroupHashes([]Transaction{a, fpay, b}), 2)
}

func TestFeePaymentCompanionPair(t *testing.T) {
	partitiontest.PartitionTest(t)
	t.Parallel()

	other := Transaction{Type: protocol.PaymentTx, Header: Header{Sender: basics.Address{1}, FirstValid: 1, LastValid: 2}, PaymentTxnFields: PaymentTxnFields{Receiver: basics.Address{2}}}
	feePay := Transaction{Type: protocol.FeePaymentTx, Header: Header{Sender: basics.Address{3}, FirstValid: 1, LastValid: 2, Group: GroupID([]Transaction{other})}}

	require.False(t, IsValidFeePaymentCompanionPair(feePay, other))
	require.True(t, IsValidFeePaymentCompanionPair(other, feePay))

	otherWithGroup := other
	otherWithGroup.Group = crypto.Digest{9}
	require.False(t, IsValidFeePaymentCompanionPair(otherWithGroup, feePay))

	badFeePay := feePay
	badFeePay.Group = crypto.Digest{7}
	require.False(t, IsValidFeePaymentCompanionPair(other, badFeePay))
}
