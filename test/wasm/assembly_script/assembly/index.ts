// The entry file of your WebAssembly module.
// External function declarations imported from "algorand" module
@external("algorand", "host_hello")
declare function host_hello(message: i32): void;

@external("algorand", "host_get_global_uint") 
declare function host_get_global_uint(app: u64, key: i32, len: i32): u64;

@external("algorand", "host_set_global_uint")
declare function host_set_global_uint(app: u64, key: i32, len: i32, value: u64): void;

// Helper function to convert string to C string pointer
function stringToCString(str: string): i32 {
  const utf8Bytes = String.UTF8.encode(str, true); // null-terminated
  const ptr = heap.alloc(utf8Bytes.byteLength);
  memory.copy(ptr, changetype<i32>(utf8Bytes), utf8Bytes.byteLength);
  return ptr as i32;
}

// Wrapper functions
export function getGlobalUint(app: u64, key: string): u64 {
  const keyPtr = stringToCString(key);
  const keyLen = String.UTF8.byteLength(key);
  const result = host_get_global_uint(app, keyPtr, keyLen);
  heap.free(keyPtr); // Clean up allocated memory
  return result;
}

export function setGlobalUint(app: u64, key: string, value: u64): void {
  const keyPtr = stringToCString(key);
  const keyLen = String.UTF8.byteLength(key);
  host_set_global_uint(app, keyPtr, keyLen, value);
  heap.free(keyPtr); // Clean up allocated memory
}

export function hello(message: string): void {
  const messagePtr = stringToCString(message);
  host_hello(messagePtr);
  heap.free(messagePtr); // Clean up allocated memory
}

export function program(): u64 {
  const key = "counter";

  const counter = getGlobalUint(888, key);

  setGlobalUint(888, key, counter + 1);

  hello("Hello from AssemblyScript! Counter: " + getGlobalUint(888, key).toString());

  return 1
}
