package main

import (
	"strconv"
	"unsafe"
)

// Import the external functions from the "algorand" WASM module
//
//go:wasmimport algorand host_hello
func hostHello(message *byte)

//go:wasmimport algorand host_get_global_uint
func hostGetGlobalUint(app uint64, key *byte, length int32) uint64

//go:wasmimport algorand host_set_global_uint
func hostSetGlobalUint(app uint64, key *byte, length int32, value uint64)

// Helper function to convert Go string to C-style null-terminated byte pointer
func stringToCPtr(s string) *byte {
	if len(s) == 0 {
		return nil
	}
	// Create null-terminated byte slice
	bytes := make([]byte, len(s)+1)
	copy(bytes, s)
	bytes[len(s)] = 0 // null terminator
	return (*byte)(unsafe.Pointer(&bytes[0]))
}

// Public wrapper functions (equivalent to your Rust functions)
func GetGlobalUint(app uint64, key string) uint64 {
	keyPtr := stringToCPtr(key)
	return hostGetGlobalUint(app, keyPtr, int32(len(key)))
}

func SetGlobalUint(app uint64, key string, value uint64) {
	keyPtr := stringToCPtr(key)
	hostSetGlobalUint(app, keyPtr, int32(len(key)), value)
}

func Hello(message string) {
	messagePtr := stringToCPtr(message)
	hostHello(messagePtr)
}

//export program
func Program() uint64 {
	key := "counter"

	counter := GetGlobalUint(888, key)
	SetGlobalUint(888, key, counter+1)
	Hello("Hello from TinyGo! Counter: " + strconv.FormatUint(GetGlobalUint(888, key), 10))

	return 1
}
