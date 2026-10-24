;; A WASM application program that writes a uint global, deletes it, and
;; approves iff reading it back yields the absent default (0).
(module
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (import "env" "app_global_get_uint" (func $get (param i32 i32) (result i64)))
  (import "env" "app_global_del" (func $del (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "d")
  (func (export "program") (result i32)
    (call $put (i32.const 0) (i32.const 1) (i64.const 5))
    (call $del (i32.const 0) (i32.const 1))
    (i64.eqz (call $get (i32.const 0) (i32.const 1)))))
