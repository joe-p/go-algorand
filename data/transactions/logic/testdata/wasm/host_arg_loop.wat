;; A WASM LogicSig that calls `arg` N times, where N is read from arg 0 (4-byte
;; little-endian). Used by the host-call cost table.
(module
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $n i32)
    (drop (call $arg (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (drop (call $arg (i32.const 0) (i32.const 32) (i32.const 4)))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
