;; A WASM application program that calls `app_global_del` N times, where N is
;; ApplicationArgs[0] (4-byte little-endian). The key is absent, so the delete is
;; a no-op past the lookup. Used by the host-call cost table.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_del" (func $del (param i32 i32)))
  (memory (export "memory") 1)
  (data (i32.const 0) "d")
  (func (export "program") (result i32)
    (local $n i32)
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (call $del (i32.const 0) (i32.const 1))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
