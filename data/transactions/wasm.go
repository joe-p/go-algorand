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

import "bytes"

// wasmMagic is the WebAssembly module magic number. A program whose bytes begin
// with these is a WASM program rather than an AVM/TEAL one. This cannot collide
// with a valid AVM program: AVM bytecode begins with a uvarint program version,
// so the first byte 0x00 would be version 0, and `a`, `s`, `m` do not form a
// valid instruction stream.
//
// It lives here, rather than in the logic package, so that both the logic
// evaluator and the version checks in this package can recognize WASM programs
// without an import cycle.
var wasmMagic = []byte{0x00, 0x61, 0x73, 0x6d}

// IsWasmProgram reports whether program is a WASM program (a LogicSig's Logic or
// an application's approval/clear-state program).
func IsWasmProgram(program []byte) bool {
	return bytes.HasPrefix(program, wasmMagic)
}
