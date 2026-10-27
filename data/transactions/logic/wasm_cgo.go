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

// wasm_txn_field_cb is implemented in Go (see wasm_host.go). Rust calls it to
// fetch transaction or context fields while the guest runs. `source` selects
// current txn (0), group txn (1, using group_index), or global context (2).
// `index` selects the element for an array field (`txna`), and `expect_array`
// says whether an array field is expected.
typedef int32_t (*wasm_txn_field_cb)(
	uintptr_t ctx,
	uint32_t source,
	uint32_t group_index,
	uint32_t field,
	uint32_t index,
	uint32_t expect_array,
	uint8_t *buf,
	size_t buf_cap,
	size_t *out_len,
	uint8_t *is_bytes,
	uint8_t *err_buf,
	size_t err_buf_len);

// wasm_ed25519verify_bare_cb is implemented in Go (see wasm_host.go). Rust
// calls it to verify an ed25519 signature over three byte ranges.
typedef int32_t (*wasm_ed25519verify_bare_cb)(
	const uint8_t *data,
	size_t data_len,
	const uint8_t *sig,
	size_t sig_len,
	const uint8_t *pk,
	size_t pk_len,
	uint8_t *err_buf,
	size_t err_buf_len);

// wasm_global_get_cb is implemented in Go (see wasm_host.go). Rust calls it to
// read a key from the current app's global state. `out_is_bytes` and
// `out_exists` report the value's type and whether the key exists.
typedef int32_t (*wasm_global_get_cb)(
	uintptr_t ctx,
	const uint8_t *key,
	size_t key_len,
	uint8_t *buf,
	size_t buf_cap,
	size_t *out_len,
	uint8_t *out_is_bytes,
	uint8_t *out_exists,
	uint8_t *err_buf,
	size_t err_buf_len);

// Asset query kind 0 is balance (account points to 32 bytes), kind 1 is unit
// name (account ignored). Missing resources set out_exists=0, distinct from
// unavailable resources, which fail. Uint results are 8 little-endian bytes.
typedef int32_t (*wasm_asset_get_cb)(
	uintptr_t ctx, uint32_t kind, uint64_t asset, const uint8_t *account,
	uint8_t *buf, size_t buf_cap, size_t *out_len, uint8_t *out_exists,
	uint8_t *err_buf, size_t err_buf_len);

// Operations: 0=begin, 1=next, 2=uint field, 3=bytes field, 4=submit,
// 5=last inner CreatedAssetID. Asset transactions only, never inner apps.
typedef int32_t (*wasm_inner_txn_cb)(
	uintptr_t ctx, uint32_t op, uint32_t field, uint64_t value,
	const uint8_t *data, size_t data_len, uint64_t *result,
	uint8_t *err_buf, size_t err_buf_len);

typedef int32_t (*wasm_log_cb)(uintptr_t ctx, const uint8_t *data, size_t data_len,
	uint8_t *err_buf, size_t err_buf_len);
extern int32_t goWasmLog(uintptr_t ctx, const uint8_t *data, size_t data_len,
	uint8_t *err_buf, size_t err_buf_len);

// wasm_global_put_cb is implemented in Go (see wasm_host.go). Rust calls it to
// write key/value to the current app's global state. `val_is_uint` selects a
// uint (8 little-endian bytes) or byte-string value.
typedef int32_t (*wasm_global_put_cb)(
	uintptr_t ctx,
	const uint8_t *key,
	size_t key_len,
	const uint8_t *val,
	size_t val_len,
	uint32_t val_is_uint,
	uint8_t *err_buf,
	size_t err_buf_len);

// wasm_global_del_cb is implemented in Go (see wasm_host.go). Rust calls it to
// delete a key from the current app's global state.
typedef int32_t (*wasm_global_del_cb)(
	uintptr_t ctx,
	const uint8_t *key,
	size_t key_len,
	uint8_t *err_buf,
	size_t err_buf_len);

extern int32_t goWasmTxnField(
	uintptr_t ctx,
	uint32_t source,
	uint32_t group_index,
	uint32_t field,
	uint32_t index,
	uint32_t expect_array,
	uint8_t *buf,
	size_t buf_cap,
	size_t *out_len,
	uint8_t *is_bytes,
	uint8_t *err_buf,
	size_t err_buf_len);

extern int32_t goWasmEd25519VerifyBare(
	const uint8_t *data,
	size_t data_len,
	const uint8_t *sig,
	size_t sig_len,
	const uint8_t *pk,
	size_t pk_len,
	uint8_t *err_buf,
	size_t err_buf_len);

extern int32_t goWasmGlobalGet(
	uintptr_t ctx,
	const uint8_t *key, size_t key_len,
	uint8_t *buf, size_t buf_cap, size_t *out_len,
	uint8_t *out_is_bytes, uint8_t *out_exists,
	uint8_t *err_buf, size_t err_buf_len);

extern int32_t goWasmGlobalPut(
	uintptr_t ctx,
	const uint8_t *key, size_t key_len,
	const uint8_t *val, size_t val_len, uint32_t val_is_uint,
	uint8_t *err_buf, size_t err_buf_len);

extern int32_t goWasmGlobalDel(
	uintptr_t ctx,
	const uint8_t *key, size_t key_len,
	uint8_t *err_buf, size_t err_buf_len);

extern int32_t goWasmAssetGet(
	uintptr_t ctx, uint32_t kind, uint64_t asset, const uint8_t *account,
	uint8_t *buf, size_t buf_cap, size_t *out_len, uint8_t *out_exists,
	uint8_t *err_buf, size_t err_buf_len);

extern int32_t goWasmInnerTxn(
	uintptr_t ctx, uint32_t op, uint32_t field, uint64_t value,
	const uint8_t *data, size_t data_len, uint64_t *result,
	uint8_t *err_buf, size_t err_buf_len);

// Go cannot take the address of an exported function, so expose it through a
// C helper that returns the function pointer.
static wasm_txn_field_cb get_wasm_txn_field_cb(void) {
	return goWasmTxnField;
}

static wasm_ed25519verify_bare_cb get_wasm_ed25519verify_bare_cb(void) {
	return goWasmEd25519VerifyBare;
}

static wasm_global_get_cb get_wasm_global_get_cb(void) {
	return goWasmGlobalGet;
}

static wasm_global_put_cb get_wasm_global_put_cb(void) {
	return goWasmGlobalPut;
}

static wasm_global_del_cb get_wasm_global_del_cb(void) {
	return goWasmGlobalDel;
}

static wasm_asset_get_cb get_wasm_asset_get_cb(void) {
	return goWasmAssetGet;
}

static wasm_inner_txn_cb get_wasm_inner_txn_cb(void) {
	return goWasmInnerTxn;
}
static wasm_log_cb get_wasm_log_cb(void) { return goWasmLog; }

int32_t wasm_lsig_run(
	const uint8_t *code, size_t code_len,
	const WasmLsigArg *args, size_t args_len,
	uint64_t fuel,
	uint64_t *fuel_used,
	uint64_t *args_read,
	uint8_t *err_buf, size_t err_buf_len,
	wasm_txn_field_cb txn_cb,
	wasm_ed25519verify_bare_cb ed25519verify_bare_cb,
	wasm_global_get_cb global_get_cb,
	wasm_global_put_cb global_put_cb,
	wasm_global_del_cb global_del_cb,
	wasm_asset_get_cb asset_get_cb,
	wasm_inner_txn_cb inner_txn_cb,
	wasm_log_cb log_cb,
	uintptr_t ctx,
	uint32_t is_app);

int32_t wasm_lsig_validate(
	const uint8_t *code, size_t code_len,
	uint8_t *err_buf, size_t err_buf_len);
*/
import "C"

import (
	"errors"
	"fmt"
	"runtime/cgo"
	"unsafe"
)

const wasmErrBufLen = 256

// validateWasmProgram checks that program is a structurally valid WASM
// LogicSig: a well-formed module that links against the args, txn, and crypto
// host functions and exports `program`. It does not execute the program.
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
		return fmt.Errorf("invalid wasm program: %s", cString(errBuf))
	}
	return nil
}

// runWasmProgram executes a WASM LogicSig with the given arguments, consuming
// at most fuel units. It reports whether the program approved, how much fuel
// was used, and which arguments were read. The evaluation context answers the
// program's `txn` field reads, and crypto opcodes are verified by the same
// helpers the AVM uses, for the duration of the call.
func runWasmProgram(program []byte, args [][]byte, fuel uint64, cx *EvalContext) (bool, uint64, [argsReadWords]uint64, error) {
	var accessed [argsReadWords]uint64
	if len(program) == 0 {
		return false, 0, accessed, errors.New("wasm program is empty")
	}

	argArr, freeArgs, err := cArgs(args)
	if err != nil {
		return false, 0, accessed, err
	}
	defer freeArgs()

	// Keep the Go context alive without passing Go memory containing pointers
	// through C. Rust may use the handle only during this synchronous call.
	handle := cgo.NewHandle(cx)
	defer handle.Delete()

	var fuelUsed C.uint64_t
	var errBuf [wasmErrBufLen]byte

	isApp := C.uint32_t(0)
	if cx != nil && cx.runMode == ModeApp {
		isApp = 1
	}

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
		C.get_wasm_txn_field_cb(),
		C.get_wasm_ed25519verify_bare_cb(),
		C.get_wasm_global_get_cb(),
		C.get_wasm_global_put_cb(),
		C.get_wasm_global_del_cb(),
		C.get_wasm_asset_get_cb(),
		C.get_wasm_inner_txn_cb(),
		C.get_wasm_log_cb(),
		C.uintptr_t(handle),
		isApp,
	)

	switch ret {
	case 1:
		return true, uint64(fuelUsed), accessed, nil
	case 0:
		return false, uint64(fuelUsed), accessed, nil
	default:
		return false, uint64(fuelUsed), accessed, fmt.Errorf("wasm program error: %s", cString(errBuf[:]))
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
