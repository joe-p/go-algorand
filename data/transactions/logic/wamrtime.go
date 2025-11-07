package logic

/*
#cgo LDFLAGS: ${SRCDIR}/../../../wamrtime/target/debug/libwamrtime_avm.a
#include <stdint.h>
#include <stdlib.h>

// The function exposed by the Rust library to run the test.
void test_run();
void test_avm_instrument_wasm();
uint64_t test_avm_run_program(uint8_t* err_buf, uint64_t err_buf_len);
void avm_set_exception(void* exec_env, const char* msg);

void avm_set_ctx(void *ctx);

// WAMR_BINDGEN SECTION_START
typedef uint64_t (*AvmGetGlobalUintFn)(void* exec_env, void* ctx, uint64_t app, const uint8_t* key_ptr, uint32_t key_len);

extern uint64_t goAvmGetGlobalUint(void* exec_env, void* ctx, uint64_t app, uint8_t* key_ptr, uint32_t key_len);

static inline AvmGetGlobalUintFn getGoAvmGetGlobalUint() {
	return (AvmGetGlobalUintFn)goAvmGetGlobalUint;
}

typedef void (*AvmSetGlobalUintFn)(void* exec_env, void* ctx, uint64_t app, const uint8_t* key_ptr, uint32_t key_len, uint64_t value);

extern void goAvmSetGlobalUint(void* exec_env, void* ctx, uint64_t app, uint8_t* key_ptr, uint32_t key_len, uint64_t value);

static inline AvmSetGlobalUintFn getGoAvmSetGlobalUint() {
	return (AvmSetGlobalUintFn)goAvmSetGlobalUint;
}

typedef int32_t (*AvmGetGlobalBytesFn)(void* exec_env, void* ctx, uint64_t app, const uint8_t* key_ptr, uint32_t key_len, uint8_t* dest_ptr, uint32_t dest_len);

extern int32_t goAvmGetGlobalBytes(void* exec_env, void* ctx, uint64_t app, uint8_t* key_ptr, uint32_t key_len, uint8_t* dest_ptr, uint32_t dest_len);

static inline AvmGetGlobalBytesFn getGoAvmGetGlobalBytes() {
	return (AvmGetGlobalBytesFn)goAvmGetGlobalBytes;
}

typedef void (*AvmSetGlobalBytesFn)(void* exec_env, void* ctx, uint64_t app, const uint8_t* key_ptr, uint32_t key_len, const uint8_t* src_ptr, uint32_t src_len);

extern void goAvmSetGlobalBytes(void* exec_env, void* ctx, uint64_t app, uint8_t* key_ptr, uint32_t key_len, uint8_t* src_ptr, uint32_t src_len);

static inline AvmSetGlobalBytesFn getGoAvmSetGlobalBytes() {
	return (AvmSetGlobalBytesFn)goAvmSetGlobalBytes;
}

typedef uint64_t (*AvmGetGlobalVarUintFn)(void* exec_env, void* ctx, uint64_t field_index);

extern uint64_t goAvmGetGlobalVarUint(void* exec_env, void* ctx, uint64_t field_index);

static inline AvmGetGlobalVarUintFn getGoAvmGetGlobalVarUint() {
	return (AvmGetGlobalVarUintFn)goAvmGetGlobalVarUint;
}

void avm_init(AvmGetGlobalUintFn avm_get_global_uint_impl, AvmSetGlobalUintFn avm_set_global_uint_impl, AvmGetGlobalBytesFn avm_get_global_bytes_impl, AvmSetGlobalBytesFn avm_set_global_bytes_impl, AvmGetGlobalVarUintFn avm_get_global_var_uint_impl);
// WAMR_BINDGEN SECTION_END

*/
import "C"

import (
	"fmt"
	"runtime"
	"runtime/cgo"
	"unsafe"

	"github.com/algorand/go-algorand/data/basics"
)

func wamrtimeTestInstrumentWasm() {
	C.test_avm_instrument_wasm()
}

func wamrtimeCallProgram(evalCtx *EvalContext) (uint64, error) {
	runtime.LockOSThread()
	defer runtime.UnlockOSThread()

	handle := cgo.NewHandle(evalCtx)
	defer handle.Delete()

	C.avm_set_ctx(unsafe.Pointer(&handle))

	const errBufLen = 128
	errBuf := make([]byte, errBufLen)
	ret := C.test_avm_run_program((*C.uint8_t)(unsafe.Pointer(&errBuf[0])), C.uint64_t(errBufLen))

	if errBuf[0] != 0 {
		return 0, fmt.Errorf("WAMR AVM error: %s", C.GoString((*C.char)(unsafe.Pointer(&errBuf[0]))))
	}

	return uint64(ret), nil

}

func wamrtimeInit() {
	C.avm_init(C.getGoAvmGetGlobalUint(), C.getGoAvmSetGlobalUint(), C.getGoAvmGetGlobalBytes(), C.getGoAvmSetGlobalBytes(), C.getGoAvmGetGlobalVarUint())
}

func wamrtimeException(exec_env unsafe.Pointer, err interface{}) {
	cString := C.CString(fmt.Sprintf("go-algorand panic: %v", err))
	defer C.free(unsafe.Pointer(cString))
	C.avm_set_exception(exec_env, cString)
}

//export goAvmGetGlobalUint
func goAvmGetGlobalUint(exec_env unsafe.Pointer, ctx unsafe.Pointer, app uint64, keyPtr *C.uint8_t, keyLen C.uint32_t) uint64 {
	defer func() {
		if err := recover(); err != nil {
			wamrtimeException(exec_env, err)
		}
	}()

	handle := *(*cgo.Handle)(ctx)
	cx := handle.Value().(*EvalContext)

	if !cx.availableApp(basics.AppIndex(app)) {
		panic(fmt.Sprintf("unavailable app %d", app))
	}

	key := string(C.GoBytes(unsafe.Pointer(keyPtr), C.int(keyLen)))

	val, exists, err := cx.Ledger.GetGlobal(basics.AppIndex(cx.appID), key)

	if err != nil {
		panic(err)
	}

	if !exists {
		return 0
	}

	return val.Uint
}

//export goAvmSetGlobalUint
func goAvmSetGlobalUint(exec_env unsafe.Pointer, ctx unsafe.Pointer, app uint64, keyPtr *C.uint8_t, keyLen C.uint32_t, value uint64) {
	defer func() {
		if err := recover(); err != nil {
			wamrtimeException(exec_env, err)
		}
	}()

	handle := *(*cgo.Handle)(ctx)
	cx := handle.Value().(*EvalContext)

	if !cx.availableApp(basics.AppIndex(app)) {
		panic(fmt.Sprintf("unavailable app %d", app))
	}

	key := string(C.GoBytes(unsafe.Pointer(keyPtr), C.int(keyLen)))

	tv := basics.TealValue{Uint: value, Type: basics.TealUintType}
	err := cx.Ledger.SetGlobal(basics.AppIndex(cx.appID), key, tv)

	if err != nil {
		panic(err)
	}

}

//export goAvmGetGlobalBytes
func goAvmGetGlobalBytes(exec_env unsafe.Pointer, ctx unsafe.Pointer, app uint64, keyPtr *C.uint8_t, keyLen C.uint32_t, dstPtr *C.uint8_t, dstLen C.uint32_t) C.int32_t {
	defer func() {
		if err := recover(); err != nil {
			wamrtimeException(exec_env, err)
		}
	}()

	handle := *(*cgo.Handle)(ctx)
	cx := handle.Value().(*EvalContext)

	if !cx.availableApp(basics.AppIndex(app)) {
		panic(fmt.Sprintf("unavailable app %d", app))
	}

	key := string(C.GoBytes(unsafe.Pointer(keyPtr), C.int(keyLen)))

	val, exists, err := cx.Ledger.GetGlobal(basics.AppIndex(cx.appID), key)

	if err != nil {
		panic(err)
	}

	if !exists || len(val.Bytes) == 0 {
		return 0
	}

	bytes := []byte(val.Bytes)
	if len(bytes) > int(dstLen) {
		panic("buffer too small for global bytes")
	}

	if dstPtr == nil {
		panic("nil destination for global bytes")
	}

	dstSlice := unsafe.Slice((*byte)(unsafe.Pointer(dstPtr)), len(bytes))
	copy(dstSlice, bytes)

	return C.int32_t(len(bytes))

}

//export goAvmSetGlobalBytes
func goAvmSetGlobalBytes(exec_env unsafe.Pointer, ctx unsafe.Pointer, app uint64, keyPtr *C.uint8_t, keyLen C.uint32_t, valuePtr *C.uint8_t, valueLen C.uint32_t) {
	defer func() {
		if err := recover(); err != nil {
			wamrtimeException(exec_env, err)
		}
	}()

	handle := *(*cgo.Handle)(ctx)
	cx := handle.Value().(*EvalContext)

	if !cx.availableApp(basics.AppIndex(app)) {
		panic(fmt.Sprintf("unavailable app %d", app))
	}

	key := string(C.GoBytes(unsafe.Pointer(keyPtr), C.int(keyLen)))
	value := C.GoBytes(unsafe.Pointer(valuePtr), C.int(valueLen))

	tv := basics.TealValue{Bytes: string(value), Type: basics.TealBytesType}
	err := cx.Ledger.SetGlobal(basics.AppIndex(cx.appID), key, tv)

	if err != nil {
		panic(err)
	}

}

//export goAvmGetGlobalVarUint
func goAvmGetGlobalVarUint(exec_env unsafe.Pointer, ctx unsafe.Pointer, field_index uint64) uint64 {
	defer func() {
		if err := recover(); err != nil {
			wamrtimeException(exec_env, err)
		}
	}()

	handle := *(*cgo.Handle)(ctx)
	cx := handle.Value().(*EvalContext)

	value, err := cx.globalFieldToValue(globalFieldSpecs[field_index])

	if err != nil {
		panic(err)
	}

	return value.Uint
}
