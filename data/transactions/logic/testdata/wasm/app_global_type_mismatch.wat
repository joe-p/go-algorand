;; A WASM application program that stores a uint global and then reads it as a
;; byte string, which must trap (the value type does not match).
(module
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (import "env" "app_global_get_bytes" (func $getb (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "x")
  (func (export "program") (result i32)
    (call $put (i32.const 0) (i32.const 1) (i64.const 5))
    (drop (call $getb (i32.const 0) (i32.const 1) (i32.const 16) (i32.const 8)))
    (i32.const 1)))
