;; Adversarial LogicSig: copy 2 MiB within a 4 MiB linear memory N times.
;; Argument 0 is the little-endian loop count. This tests the calibrated
;; bulk-memory charge at maximum permitted size.
(module
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (memory (export "memory") 64)
  (func (export "program") (result i32)
    (local $n i32)
    (drop (call $arg (i32.const 0) (i32.const 0) (i32.const 4)))
    (local.set $n (i32.load (i32.const 0)))
    (block $done
      (loop $loop
        (br_if $done (i32.eqz (local.get $n)))
        (memory.copy (i32.const 0) (i32.const 2097152) (i32.const 2097152))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $loop)))
    (i32.const 1)))
