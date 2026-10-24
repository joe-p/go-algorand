;; A WASM application program that writes the byte string "hi" to the global
;; key "b" and reads it back, approving iff the value is two bytes long.
(module
  (import "env" "app_global_put_bytes" (func $putb (param i32 i32 i32 i32)))
  (import "env" "app_global_get_bytes" (func $getb (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "b")
  (data (i32.const 8) "hi")
  (func (export "program") (result i32)
    (call $putb (i32.const 0) (i32.const 1) (i32.const 8) (i32.const 2))
    (i32.eq (call $getb (i32.const 0) (i32.const 1) (i32.const 16) (i32.const 2))
            (i32.const 2))))
