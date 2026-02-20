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

import "github.com/algorand/go-algorand/protocol"

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
