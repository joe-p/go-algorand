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
*/
import "C"

import (
	"encoding/binary"
	"fmt"
	"runtime/cgo"
	"unsafe"

	"github.com/algorand/go-algorand/data/basics"
	"github.com/algorand/go-algorand/protocol"
)

// goWasmTxnField answers a `txn`, `txna`, `gtxn`, or context field read. It
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
// Field selection and semantics are delegated to opTxnImpl/readGlobalField, so
// the WASM and AVM paths share typing, version, mode, and group-index checks.
//
//export goWasmTxnField
func goWasmTxnField(ctx C.uintptr_t, source C.uint32_t, groupIndex C.uint32_t, field C.uint32_t, index C.uint32_t, expectArray C.uint32_t,
	buf *C.uint8_t, bufCap C.size_t, outLen *C.size_t, isBytes *C.uint8_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic reading txn field: %v", r))
			ret = -1
		}
	}()

	cx := cgo.Handle(ctx).Value().(*EvalContext)

	sv, err := cx.wasmReadField(uint32(source), uint32(groupIndex), uint32(field), uint32(index), expectArray != 0)
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

// wasmReadField shares AVM field validation and reads without using its operand
// stack or interpreting WASM bytes as AVM immediates. Source values match
// FieldSource in wasm/src/lib.rs: current transaction, group transaction, global.
func (cx *EvalContext) wasmReadField(source, groupIndex, field, index uint32, array bool) (stackValue, error) {
	switch source {
	case 0:
		return cx.opTxnImpl(uint64(cx.groupIndex), srcGroup, TxnField(field), uint64(index), array)
	case 1:
		return cx.opTxnImpl(uint64(groupIndex), srcGroup, TxnField(field), uint64(index), array)
	case 2:
		if array {
			return stackValue{}, fmt.Errorf("global fields are not arrays")
		}
		// This first cut exposes only the context fields required by the AMM.
		// In particular, OpcodeBudget needs live WASM fuel accounting before it
		// can be exposed; cx.remainingBudget alone would return a stale value.
		switch GlobalField(field) {
		case GroupSize, CurrentApplicationID, CurrentApplicationAddress:
			return cx.readGlobalField(GlobalField(field))
		default:
			return stackValue{}, fmt.Errorf("unsupported wasm global field %d", field)
		}
	default:
		return stackValue{}, fmt.Errorf("invalid wasm field source %d", source)
	}
}

//export goWasmLog
func goWasmLog(ctx C.uintptr_t, data *C.uint8_t, dataLen C.size_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic logging: %v", r))
			ret = -1
		}
	}()
	cx := cgo.Handle(ctx).Value().(*EvalContext)
	if cx.runMode != ModeApp || cx.version < 5 {
		writeWasmErr(errBuf, errCap, "log requires application mode and AVM v5")
		return -1
	}
	if dataLen > maxStringSize {
		writeWasmErr(errBuf, errCap, "log exceeds AVM size limit")
		return -1
	}
	if err := cx.appendLog(wasmCBytes(data, dataLen)); err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	return 0
}

// goWasmInnerTxn delegates inner asset transactions to the AVM state machine.
// Operation codes match the Rust executor. Data is borrowed only during this
// synchronous callback; stackIntoTxnField copies any bytes retained by a txn.
//
//export goWasmInnerTxn
func goWasmInnerTxn(ctx C.uintptr_t, op C.uint32_t, field C.uint32_t, value C.uint64_t,
	data *C.uint8_t, dataLen C.size_t, result *C.uint64_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic executing inner transaction: %v", r))
			ret = -1
		}
	}()
	cx := cgo.Handle(ctx).Value().(*EvalContext)
	if dataLen > maxStringSize {
		writeWasmErr(errBuf, errCap, "inner field exceeds AVM size limit")
		return -1
	}
	v, err := cx.wasmInnerTxn(uint32(op), TxnField(field), uint64(value), wasmCBytes(data, dataLen))
	if err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	*result = C.uint64_t(v)
	return 0
}

func (cx *EvalContext) wasmInnerTxn(op uint32, field TxnField, value uint64, data []byte) (uint64, error) {
	if cx.runMode != ModeApp {
		return 0, fmt.Errorf("inner transactions are not available in signature mode")
	}
	if cx.version < 5 {
		return 0, fmt.Errorf("inner transactions require AVM v5")
	}
	switch op {
	case 0:
		return 0, opItxnBegin(cx)
	case 1:
		if cx.version < 6 {
			return 0, fmt.Errorf("itxn_next requires AVM v6")
		}
		return 0, opItxnNext(cx)
	case 2, 3:
		// This cut deliberately excludes inner app calls: allowing them would
		// require live synchronization of WASM fuel with shared AVM budgets.
		switch field {
		case Type, TypeEnum, Sender, Fee, Note,
			XferAsset, AssetAmount, AssetReceiver, AssetSender, AssetCloseTo,
			ConfigAsset, ConfigAssetTotal, ConfigAssetDecimals, ConfigAssetDefaultFrozen,
			ConfigAssetUnitName, ConfigAssetName, ConfigAssetURL, ConfigAssetMetadataHash,
			ConfigAssetManager, ConfigAssetReserve, ConfigAssetFreeze, ConfigAssetClawback:
		default:
			return 0, fmt.Errorf("unsupported wasm inner field %s", field)
		}
		fs, ok := txnFieldSpecByField(field)
		if !ok || fs.itxVersion == 0 || fs.itxVersion > cx.version {
			return 0, fmt.Errorf("invalid itxn_field %s", field)
		}
		sv := stackValue{Uint: value}
		if op == 3 {
			// A zero-length byte field must not become the uint zero.
			sv = stackValue{Bytes: append([]byte{}, data...)}
		}
		if sv.avmType() != fs.ftype.AVMType {
			return 0, fmt.Errorf("wrong type for inner field %s", field)
		}
		if (field == TypeEnum && value != 3 && value != 4) ||
			(field == Type && string(data) != string(protocol.AssetConfigTx) && string(data) != string(protocol.AssetTransferTx)) {
			return 0, fmt.Errorf("WASM inner transactions support only acfg and axfer")
		}
		return 0, cx.setInnerTxnField(field, sv)
	case 4:
		for _, tx := range cx.subtxns {
			if tx.Txn.Type != protocol.AssetConfigTx && tx.Txn.Type != protocol.AssetTransferTx {
				return 0, fmt.Errorf("WASM inner transactions support only acfg and axfer")
			}
		}
		return 0, opItxnSubmit(cx)
	case 5:
		sv, err := cx.opTxnImpl(0, srcInner, CreatedAssetID, 0, false)
		return sv.Uint, err
	default:
		return 0, fmt.Errorf("invalid wasm inner operation %d", op)
	}
}

// goWasmAssetGet supplies the AMM's two asset queries through shared AVM
// primitives. Account memory belongs to Rust and is used only during this call.
// Rust validates the fixed 32-byte address before entering Go. Return values
// are 0=success, -1=evaluation error, -2=buffer too small (length still reported).
//
//export goWasmAssetGet
func goWasmAssetGet(ctx C.uintptr_t, kind C.uint32_t, asset C.uint64_t, account *C.uint8_t,
	buf *C.uint8_t, bufCap C.size_t, outLen *C.size_t, outExists *C.uint8_t,
	errBuf *C.uint8_t, errCap C.size_t) (ret C.int32_t) {
	defer func() {
		if r := recover(); r != nil {
			writeWasmErr(errBuf, errCap, fmt.Sprintf("panic reading asset: %v", r))
			ret = -1
		}
	}()
	cx := cgo.Handle(ctx).Value().(*EvalContext)
	if cx.runMode != ModeApp {
		writeWasmErr(errBuf, errCap, "asset queries are not available in signature mode")
		return -1
	}
	var value stackValue
	var exists bool
	var err error
	switch kind {
	case 0:
		if account == nil {
			writeWasmErr(errBuf, errCap, "missing asset holding account")
			return -1
		}
		value, exists, err = cx.readAssetHolding(stackValue{Bytes: wasmCBytes(account, 32)}, uint64(asset), AssetBalance)
	case 1:
		value, exists, err = cx.readAssetParams(uint64(asset), AssetUnitName)
	default:
		err = fmt.Errorf("invalid wasm asset query %d", kind)
	}
	if err != nil {
		writeWasmErr(errBuf, errCap, err.Error())
		return -1
	}
	*outExists = 0
	*outLen = 0
	if !exists {
		return 0
	}
	*outExists = 1
	var raw [8]byte
	data := value.Bytes
	if kind == 0 {
		binary.LittleEndian.PutUint64(raw[:], value.Uint)
		data = raw[:]
	}
	if len(data) > maxStringSize {
		writeWasmErr(errBuf, errCap, "asset value exceeds AVM size limit")
		return -1
	}
	*outLen = C.size_t(len(data))
	if C.size_t(len(data)) > bufCap {
		return -2
	}
	if len(data) > 0 {
		copy(unsafe.Slice((*byte)(unsafe.Pointer(buf)), int(bufCap)), data)
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
