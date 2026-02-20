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
	"github.com/algorand/go-algorand/crypto"
	"github.com/algorand/go-algorand/protocol"
)

// MaxGroupSizeWithFeePayment returns the physical transaction-group size limit,
// allowing one additional slot when fee-payment transactions are present.
func MaxGroupSizeWithFeePayment(maxTxGroupSize int) int {
	return maxTxGroupSize + 1
}

// IsValidGroupSize reports whether a group size is allowed when FeePayment
// transactions do not count towards MaxTxGroupSize.
func IsValidGroupSize(maxTxGroupSize int, groupSize int, feePaymentCount int) bool {
	if groupSize > MaxGroupSizeWithFeePayment(maxTxGroupSize) {
		return false
	}
	return groupSize-feePaymentCount <= maxTxGroupSize
}

// FeePaymentCount returns the number of FeePayment transactions in a group.
func FeePaymentCount(txgroup []SignedTxn) int {
	count := 0
	for i := range txgroup {
		if txgroup[i].Txn.Type == protocol.FeePaymentTx {
			count++
		}
	}
	return count
}

// FeePaymentCountWithAD returns the number of FeePayment transactions in a
// group with apply data.
func FeePaymentCountWithAD(txgroup []SignedTxnWithAD) int {
	count := 0
	for i := range txgroup {
		if txgroup[i].Txn.Type == protocol.FeePaymentTx {
			count++
		}
	}
	return count
}

// TxGroupHashes excludes FeePayment transactions from GroupID construction.
func TxGroupHashes(txgroup []Transaction) []crypto.Digest {
	hashes := make([]crypto.Digest, 0, len(txgroup))
	for i := range txgroup {
		if txgroup[i].Type == protocol.FeePaymentTx {
			continue
		}
		txWithoutGroup := txgroup[i]
		txWithoutGroup.Group = crypto.Digest{}
		hashes = append(hashes, crypto.Digest(txWithoutGroup.ID()))
	}
	return hashes
}

// TxGroupHashesFromSigned excludes FeePayment transactions from GroupID construction.
func TxGroupHashesFromSigned(txgroup []SignedTxn) []crypto.Digest {
	txns := make([]Transaction, len(txgroup))
	for i := range txgroup {
		txns[i] = txgroup[i].Txn
	}
	return TxGroupHashes(txns)
}

// TxGroupHashesFromSignedWithAD excludes FeePayment transactions from GroupID construction.
func TxGroupHashesFromSignedWithAD(txgroup []SignedTxnWithAD) []crypto.Digest {
	txns := make([]Transaction, len(txgroup))
	for i := range txgroup {
		txns[i] = txgroup[i].Txn
	}
	return TxGroupHashes(txns)
}

// GroupID computes GroupID while excluding FeePayment transactions.
func GroupID(txgroup []Transaction) crypto.Digest {
	return crypto.HashObj(TxGroup{TxGroupHashes: TxGroupHashes(txgroup)})
}

// IsValidFeePaymentCompanionPair reports whether a 2-transaction group follows
// the companion format: a non-FeePayment txn with zero Group followed by a
// FeePayment txn with non-zero Group where the fee-payment Group matches the
// non-fee transaction's GroupID.
func IsValidFeePaymentCompanionPair(a, b Transaction) bool {
	if a.Type == protocol.FeePaymentTx || b.Type != protocol.FeePaymentTx {
		return false
	}

	feePay := b
	other := a

	if feePay.Group.IsZero() {
		return false
	}
	if !other.Group.IsZero() {
		return false
	}

	return feePay.Group == GroupID([]Transaction{other})
}

// IsValidFeePaymentCompanionGroupSigned checks the companion format for a
// signed two-transaction group.
func IsValidFeePaymentCompanionGroupSigned(txgroup []SignedTxn) bool {
	return len(txgroup) == 2 && IsValidFeePaymentCompanionPair(txgroup[0].Txn, txgroup[1].Txn)
}

// IsValidFeePaymentCompanionGroupSignedWithAD checks the companion format for a
// signed-with-apply-data two-transaction group.
func IsValidFeePaymentCompanionGroupSignedWithAD(txgroup []SignedTxnWithAD) bool {
	return len(txgroup) == 2 && IsValidFeePaymentCompanionPair(txgroup[0].Txn, txgroup[1].Txn)
}
