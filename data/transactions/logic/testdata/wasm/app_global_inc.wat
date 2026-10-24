;; A WASM application program that increments the uint64 global "counter".
;; A missing key reads as 0, so the first call stores 1.
(module
  (import "env" "app_global_get_uint" (func $get (param i32 i32) (result i64)))
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "counter")
  (func (export "program") (result i32)
    (call $put (i32.const 0) (i32.const 7)
      (i64.add (call $get (i32.const 0) (i32.const 7)) (i64.const 1)))
    (i32.const 1)))
