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

	"github.com/algorand/go-algorand/data/basics"
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

// goWasmGlobalGet answers a global-state read from a WASM guest. It is called by
// the Rust executor through C while the guest runs, and must not let a panic
// cross the FFI boundary.
//
// It reports the value in buf (up to bufCap): a uint is 8 little-endian bytes,
// a byte string is copied verbatim. outLen receives the value's length,
// outIsBytes whether it is a byte string (0 = uint), and outExists whether the
// key exists (0/1). It returns 0 on success, -1 on error (with a message in
// errBuf), or -2 when buf is too small (outLen still receives the required
// length).
//
// The read goes through the same ledger accessor the AVM's global state opcodes
// use, so the two paths cannot disagree.
//
//export goWasmGlobalGet
func goWasmGlobalGet(ctx C.uintptr_t, keyPtr *C.uint8_t, keyLen C.size_t,
	buf *C.uint8_t, bufCap C.size_t, outLen *C.size_t, outIsBytes *C.uint8_t,
	outExists *C.uint8_t, errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic reading global state: %v", r))
			ret = -1
		}
	}()

	cx := cgo.Handle(ctx).Value().(*EvalContext)
	key := string(wasmCBytes(keyPtr, keyLen))

	tv, ok, err := cx.Ledger.GetGlobal(cx.appID, key)
	if err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	if outExists != nil {
		if ok {
			*outExists = 1
		} else {
			*outExists = 0
		}
	}
	if !ok {
		if outIsBytes != nil {
			*outIsBytes = 0
		}
		if outLen != nil {
			*outLen = 0
		}
		return 0
	}

	if tv.Type == basics.TealBytesType {
		if outIsBytes != nil {
			*outIsBytes = 1
		}
		n := len(tv.Bytes)
		if outLen != nil {
			*outLen = C.size_t(n)
		}
		if C.size_t(n) > bufCap {
			return -2
		}
		if n > 0 {
			copy(unsafe.Slice((*byte)(unsafe.Pointer(buf)), int(bufCap)), tv.Bytes)
		}
		return 0
	}

	// A uint value, encoded as 8 little-endian bytes.
	if outIsBytes != nil {
		*outIsBytes = 0
	}
	if outLen != nil {
		*outLen = 8
	}
	if bufCap < 8 {
		return -2
	}
	var tmp [8]byte
	binary.LittleEndian.PutUint64(tmp[:], tv.Uint)
	copy(unsafe.Slice((*byte)(unsafe.Pointer(buf)), int(bufCap)), tmp[:])
	return 0
}

// goWasmGlobalPut answers a global-state write from a WASM guest. It is called
// by the Rust executor through C while the guest runs, and must not let a panic
// cross the FFI boundary.
//
// valIsUint non-zero means the value is a uint, encoded as 8 little-endian
// bytes in val; otherwise val is a byte string. It returns 0 on success and -1
// on error (with a message in errBuf).
//
// The write goes through the same helper the AVM's app_global_put opcode uses,
// so the two paths cannot disagree.
//
//export goWasmGlobalPut
func goWasmGlobalPut(ctx C.uintptr_t, keyPtr *C.uint8_t, keyLen C.size_t,
	valPtr *C.uint8_t, valLen C.size_t, valIsUint C.uint32_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic writing global state: %v", r))
			ret = -1
		}
	}()

	cx := cgo.Handle(ctx).Value().(*EvalContext)
	key := string(wasmCBytes(keyPtr, keyLen))

	var tv basics.TealValue
	if valIsUint != 0 {
		if valLen != 8 {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("uint global value had %d bytes, expected 8", valLen))
			return -1
		}
		var raw [8]byte
		copy(raw[:], wasmCBytes(valPtr, valLen))
		tv = basics.TealValue{Type: basics.TealUintType, Uint: binary.LittleEndian.Uint64(raw[:])}
	} else {
		tv = basics.TealValue{Type: basics.TealBytesType, Bytes: string(wasmCBytes(valPtr, valLen))}
	}

	if err := cx.putAppGlobalValue(key, tv); err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	return 0
}

// goWasmGlobalDel answers a global-state delete from a WASM guest. It is called
// by the Rust executor through C while the guest runs, and must not let a panic
// cross the FFI boundary.
//
// It returns 0 on success and -1 on error (with a message in errBuf). The
// delete goes through the same helper the AVM's app_global_del opcode uses.
//
//export goWasmGlobalDel
func goWasmGlobalDel(ctx C.uintptr_t, keyPtr *C.uint8_t, keyLen C.size_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic deleting global state: %v", r))
			ret = -1
		}
	}()

	cx := cgo.Handle(ctx).Value().(*EvalContext)
	key := string(wasmCBytes(keyPtr, keyLen))

	if err := cx.delAppGlobalValue(key); err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	return 0
}
