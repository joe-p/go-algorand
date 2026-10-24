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
	"sync"
	"sync/atomic"
	"unsafe"
)

// wasmCtxs maps an opaque handle to the evaluation context of an in-flight WASM
// execution. A raw *EvalContext cannot be handed to C directly: it is Go
// memory containing Go pointers. A handle is valid only for the duration of one
// runWasmProgram call.
var (
	wasmCtxs   sync.Map // uintptr -> *EvalContext
	wasmCtxSeq atomic.Uintptr
)

func registerWasmCtx(cx *EvalContext) uintptr {
	handle := wasmCtxSeq.Add(1)
	wasmCtxs.Store(handle, cx)
	return handle
}

func unregisterWasmCtx(handle uintptr) {
	wasmCtxs.Delete(handle)
}

// goWasmTxnField answers a `txn` field read from a WASM guest. It is called by
// the Rust executor through C while the guest runs, and must not let a panic
// cross the FFI boundary.
//
// It writes the field's bytes into buf (up to buf_cap), reports the required
// length in out_len and whether the field is a byte string in is_bytes (0 =
// uint, non-zero = bytes), and returns 0 on success, -1 on error (with a
// message in err_buf), or -2 when buf is too small.
//
// Field selection and semantics are delegated to opTxnImpl, so the wasm and AVM
// code paths cannot disagree about what a `txn` field is.
//
//export goWasmTxnField
func goWasmTxnField(ctx C.uintptr_t, field C.uint32_t, buf *C.uint8_t, bufCap C.size_t,
	outLen *C.size_t, isBytes *C.uint8_t, errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic reading txn field: %v", r))
			ret = -1
		}
	}()

	val, ok := wasmCtxs.Load(uintptr(ctx))
	if !ok {
		writeWasmErr(errBuf, errCap, "unknown wasm evaluation context")
		return -1
	}
	cx := val.(*EvalContext)

	sv, err := cx.opTxnImpl(uint64(cx.groupIndex), srcGroup, TxnField(field), 0, false)
	if err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
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
