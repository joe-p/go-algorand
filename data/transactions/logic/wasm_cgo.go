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

/*
#cgo CFLAGS: -Wall
#cgo LDFLAGS: ${SRCDIR}/../../../wasm/target/release/libwasm_lsig.a
#cgo LDFLAGS: -lpthread -ldl -lm
#include <stdint.h>
#include <stdlib.h>

typedef struct {
	const uint8_t *ptr;
	size_t len;
} WasmLsigArg;

int32_t wasm_lsig_run(
	const uint8_t *code, size_t code_len,
	const WasmLsigArg *args, size_t args_len,
	uint64_t fuel,
	uint64_t *fuel_used,
	uint64_t *args_read,
	uint8_t *err_buf, size_t err_buf_len);

int32_t wasm_lsig_validate(
	const uint8_t *code, size_t code_len,
	uint8_t *err_buf, size_t err_buf_len);
*/
import "C"

import (
	"errors"
	"fmt"
	"unsafe"
)

const wasmErrBufLen = 256

// validateWasmProgram checks that program is a structurally valid WASM
// LogicSig: a well-formed module that links against the args-only host and
// exports `program`. It does not execute the program.
func validateWasmProgram(program []byte) error {
	if len(program) == 0 {
		return errors.New("wasm program is empty")
	}
	errBuf := make([]byte, wasmErrBufLen)
	ret := C.wasm_lsig_validate(
		(*C.uint8_t)(unsafe.Pointer(&program[0])),
		C.size_t(len(program)),
		(*C.uint8_t)(unsafe.Pointer(&errBuf[0])),
		C.size_t(len(errBuf)),
	)
	if ret != 0 {
		return fmt.Errorf("invalid wasm LogicSig: %s", cString(errBuf))
	}
	return nil
}

// runWasmProgram executes a WASM LogicSig with the given arguments, consuming
// at most fuel units. It reports whether the program approved, how much fuel
// was used, and which arguments were read.
func runWasmProgram(program []byte, args [][]byte, fuel uint64) (bool, uint64, [argsReadWords]uint64, error) {
	var accessed [argsReadWords]uint64
	if len(program) == 0 {
		return false, 0, accessed, errors.New("wasm program is empty")
	}

	argArr, freeArgs, err := cArgs(args)
	if err != nil {
		return false, 0, accessed, err
	}
	defer freeArgs()

	var fuelUsed C.uint64_t
	var errBuf [wasmErrBufLen]byte

	ret := C.wasm_lsig_run(
		(*C.uint8_t)(unsafe.Pointer(&program[0])),
		C.size_t(len(program)),
		argArr,
		C.size_t(len(args)),
		C.uint64_t(fuel),
		&fuelUsed,
		(*C.uint64_t)(unsafe.Pointer(&accessed[0])),
		(*C.uint8_t)(unsafe.Pointer(&errBuf[0])),
		C.size_t(len(errBuf)),
	)

	switch ret {
	case 1:
		return true, uint64(fuelUsed), accessed, nil
	case 0:
		return false, uint64(fuelUsed), accessed, nil
	default:
		return false, uint64(fuelUsed), accessed, fmt.Errorf("wasm LogicSig error: %s", cString(errBuf[:]))
	}
}

// cArgs copies args into C memory as an array of WasmLsigArg. The returned
// cleanup function releases every allocation.
func cArgs(args [][]byte) (*C.WasmLsigArg, func(), error) {
	noop := func() {}
	if len(args) == 0 {
		return nil, noop, nil
	}

	argArr := (*C.WasmLsigArg)(C.malloc(C.size_t(len(args)) * C.size_t(unsafe.Sizeof(C.WasmLsigArg{}))))
	if argArr == nil {
		return nil, noop, errors.New("out of memory allocating wasm args")
	}
	cleanup := func() {
		C.free(unsafe.Pointer(argArr))
	}

	entries := unsafe.Slice(argArr, len(args))
	bufs := make([]unsafe.Pointer, 0, len(args))
	for i, arg := range args {
		if len(arg) == 0 {
			entries[i].ptr = nil
			entries[i].len = 0
			continue
		}
		buf := C.CBytes(arg)
		if buf == nil {
			for _, b := range bufs {
				C.free(b)
			}
			cleanup()
			return nil, noop, errors.New("out of memory copying wasm args")
		}
		bufs = append(bufs, buf)
		entries[i].ptr = (*C.uint8_t)(buf)
		entries[i].len = C.size_t(len(arg))
	}

	freeAll := func() {
		for _, b := range bufs {
			C.free(b)
		}
		cleanup()
	}
	return argArr, freeAll, nil
}

func cString(buf []byte) string {
	for i, b := range buf {
		if b == 0 {
			return string(buf[:i])
		}
	}
	return string(buf)
}
