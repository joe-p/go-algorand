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
	"bytes"

	"github.com/algorand/go-algorand/data/transactions"
)

// wasmMagic is the WebAssembly module magic number. A LogicSig whose Logic
// begins with these bytes is a WASM program rather than an AVM/TEAL one. This
// cannot collide with a valid AVM program: AVM bytecode begins with a uvarint
// program version, so the first byte 0x00 would be version 0, and `a`, `s`, `m`
// do not form a valid instruction stream.
var wasmMagic = []byte{0x00, 0x61, 0x73, 0x6d}

// IsWasmProgram reports whether program is a WASM LogicSig.
func IsWasmProgram(program []byte) bool {
	return bytes.HasPrefix(program, wasmMagic)
}

// argsReadWords is the number of uint64 words needed to record which LogicSig
// arguments a WASM program read. It matches the executor's `ARGS_READ_WORDS`
// and transactions.EvalMaxArgs.
const argsReadWords = (transactions.EvalMaxArgs + 63) / 64
