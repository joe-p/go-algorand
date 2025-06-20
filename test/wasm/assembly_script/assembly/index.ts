// The entry file of your WebAssembly module.
// External function declarations imported from "algorand" module
@external("algorand", "host_get_global_uint") 
declare function host_get_global_uint(app: u64, key: i32, len: i32): u64;

@external("algorand", "host_set_global_uint")
declare function host_set_global_uint(app: u64, key: i32, len: i32, value: u64): void;

@external("algorand", "host_get_current_application_id")
declare function host_get_current_application_id(): u64;

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

export function getCurrentAppId(): u64 {
  return host_get_current_application_id();
}

function getCounter(): u64 {
  return getGlobalUint(getCurrentAppId(), "counter");
}

function incrementCounter(): void {
  setGlobalUint(getCurrentAppId(), "counter", getCounter() + 1);
}


export function program(): u64 {

  while (getCounter() < 10) {
    incrementCounter();
  }

  return getCounter();
}
