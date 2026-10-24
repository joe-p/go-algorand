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
#include <stdint.h>
#include <stddef.h>

// goWasmTxnField is implemented in Go below, via //export.
extern int32_t goWasmTxnField(
	uintptr_t ctx,
	uint32_t field,
	uint32_t index,
	uint32_t expect_array,
	uint8_t *buf,
	size_t buf_cap,
	size_t *out_len,
	uint8_t *is_bytes,
	uint8_t *err_buf,
	size_t err_buf_len);
*/
import "C"

import (
	"encoding/binary"
	"fmt"
	"runtime/cgo"
	"unsafe"
)

// goWasmTxnField answers a `txn` (or `txna`) field read from a WASM guest. It
// is called by the Rust executor through C while the guest runs, and must not
// let a panic cross the FFI boundary.
//
// It writes the field's bytes into buf (up to buf_cap), reports the required
// length in out_len and whether the field is a byte string in is_bytes (0 =
// uint, non-zero = bytes), and returns 0 on success, -1 on error (with a
// message in err_buf), or -2 when buf is too small.
//
// field selects the field; index selects the element for an array field and is
// meaningful only when expectArray is non-zero.
//
// Field selection and semantics are delegated to opTxnImpl, so the wasm and AVM
// code paths cannot disagree about what a `txn`/`txna` field is.
//
//export goWasmTxnField
func goWasmTxnField(ctx C.uintptr_t, field C.uint32_t, index C.uint32_t, expectArray C.uint32_t,
	buf *C.uint8_t, bufCap C.size_t, outLen *C.size_t, isBytes *C.uint8_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic reading txn field: %v", r))
			ret = -1
		}
	}()

	cx := cgo.Handle(ctx).Value().(*EvalContext)

	sv, err := cx.opTxnImpl(uint64(cx.groupIndex), srcGroup, TxnField(field), uint64(index), expectArray != 0)
	if err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	// The AVM normally checks this after the opcode returns. WASM bypasses
	// that stack check, so enforce the same limit at the host boundary.
	if len(sv.Bytes) > maxStringSize {
		writeWasmErr(errBuf, errCap, fmt.Sprintf("txn produced a too big (%d) byte-array", len(sv.Bytes)))
		return -1
	}

	if sv.Bytes == nil {
		// A uint64 field, encoded as 8 little-endian bytes.
		if isBytes != nil {
			*isBytes = 0
		}
		if outLen != nil {
			*outLen = 8
		}
		if bufCap < 8 {
			return -2
		}
		var tmp [8]byte
		binary.LittleEndian.PutUint64(tmp[:], sv.Uint)
		copy(unsafe.Slice((*byte)(unsafe.Pointer(buf)), int(bufCap)), tmp[:])
		return 0
	}

	if isBytes != nil {
		*isBytes = 1
	}
	if outLen != nil {
		*outLen = C.size_t(len(sv.Bytes))
	}
	if C.size_t(len(sv.Bytes)) > bufCap {
		return -2
	}
	if len(sv.Bytes) > 0 {
		copy(unsafe.Slice((*byte)(unsafe.Pointer(buf)), int(bufCap)), sv.Bytes)
	}
	return 0
}

// goWasmEd25519VerifyBare answers an `ed25519verify_bare` opcode from a WASM
// guest. The Rust executor copies the three operands out of guest memory and
// calls this through C while the guest runs, and it must not let a panic cross
// the FFI boundary.
//
// It returns 1 when the signature is valid, 0 when it is not, and -1 on error
// (with a message in err_buf).
//
// Verification is delegated to ed25519VerifyBare, the helper the AVM opcode
// uses, so the wasm and AVM code paths cannot disagree about the opcode.
//
//export goWasmEd25519VerifyBare
func goWasmEd25519VerifyBare(dataPtr *C.uint8_t, dataLen C.size_t,
	sigPtr *C.uint8_t, sigLen C.size_t, pkPtr *C.uint8_t, pkLen C.size_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic verifying ed25519 signature: %v", r))
			ret = -1
		}
	}()

	data := wasmCBytes(dataPtr, dataLen)
	// The AVM can only push values up to maxStringSize, and the Rust side
	// rejects longer ranges, but enforce the same bound here as the txn path.
	if len(data) > maxStringSize {
		writeWasmErr(errBuf, errCap, fmt.Sprintf("ed25519verify_bare data is too big (%d)", len(data)))
		return -1
	}
	sig := wasmCBytes(sigPtr, sigLen)
	pk := wasmCBytes(pkPtr, pkLen)

	ok, err := ed25519VerifyBare(data, sig, pk)
	if err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	if ok {
		return 1
	}
	return 0
}

// wasmCBytes views a C byte range as a Go slice. The slice aliases C memory and
// is valid only for the duration of the host call. A zero length yields an
// empty slice regardless of the pointer.
func wasmCBytes(ptr *C.uint8_t, length C.size_t) []byte {
	if length == 0 {
		return nil
	}
	return unsafe.Slice((*byte)(unsafe.Pointer(ptr)), int(length))
}

// writeWasmErr copies msg into a NUL-terminated C buffer, truncating if needed.
func writeWasmErr(errBuf *C.uint8_t, errCap C.size_t, msg string) {
	if errBuf == nil || errCap == 0 {
		return
	}
	dst := unsafe.Slice((*byte)(unsafe.Pointer(errBuf)), int(errCap))
	n := copy(dst, msg)
	if n >= len(dst) {
		n = len(dst) - 1
	}
	dst[n] = 0
}
