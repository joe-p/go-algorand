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

package logic

import (
	"github.com/algorand/go-algorand/data/transactions"
)

// IsWasmProgram reports whether program is a WASM program (a LogicSig's Logic or
// an application's approval/clear-state program). It is a thin wrapper over
// transactions.IsWasmProgram, which lives in the transactions package so the
// version checks there can recognize WASM without an import cycle.
func IsWasmProgram(program []byte) bool {
	return transactions.IsWasmProgram(program)
}

// argsReadWords is the number of uint64 words needed to record which LogicSig
// arguments a WASM program read. It matches the executor's `ARGS_READ_WORDS`
// and transactions.EvalMaxArgs.
const argsReadWords = (transactions.EvalMaxArgs + 63) / 64
