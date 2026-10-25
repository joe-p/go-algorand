;; Adversarial LogicSig: read a 4096-byte argument N times. Argument 0 is the
;; little-endian loop count; argument 1 is a 4096-byte payload. The host `arg`
;; charge is fixed (30 fuel) regardless of payload length, so this measures the
;; worst host-call payload case.
(module
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $n i32)
    (drop (call $arg (i32.const 0) (i32.const 0) (i32.const 4)))
    (local.set $n (i32.load (i32.const 0)))
    (block $done
      (loop $loop
        (br_if $done (i32.eqz (local.get $n)))
        (drop (call $arg (i32.const 1) (i32.const 16) (i32.const 4096)))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $loop)))
    (i32.const 1)))
