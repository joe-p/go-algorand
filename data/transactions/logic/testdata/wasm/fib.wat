;; A WASM LogicSig that computes fib(N), where N is read from LogicSig argument
;; 0 as a 4-byte big-endian unsigned integer. It approves (returns 1)
;; unconditionally after the computation; the point is the work performed.
;;
;; Used by the block-eval benchmark in ledger/lsigbench_test.go to compare the
;; cost of a block of fibonacci WASM LogicSigs against the equivalent TEAL ones.
(module
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $a i64)
    (local $b i64)
    (local $t i64)
    (local $n i64)
    ;; read argument 0 into linear memory at offset 0 (up to 4 bytes)
    (i32.const 0)
    (i32.const 0)
    (i32.const 4)
    (call $arg)
    (drop)
    ;; n = big-endian u32 assembled from memory[0..4]
    (local.set $n
      (i64.or
        (i64.or
          (i64.or
            (i64.shl (i64.extend_i32_u (i32.load8_u (i32.const 0))) (i64.const 24))
            (i64.shl (i64.extend_i32_u (i32.load8_u (i32.const 1))) (i64.const 16)))
          (i64.shl (i64.extend_i32_u (i32.load8_u (i32.const 2))) (i64.const 8)))
        (i64.extend_i32_u (i32.load8_u (i32.const 3)))))
    ;; a = 0, b = 1
    (local.set $b (i64.const 1))
    ;; iterate n times: (a, b) <- (b, (a + b) mod 2^62). The mask matches the
    ;; TEAL program in ledger/lsigbench_test.go, which needs it to avoid the
    ;; uint64 overflow check on `+`.
    (block $done
      (loop $loop
        (br_if $done (i64.eqz (local.get $n)))
        (local.set $t (i64.and (i64.add (local.get $a) (local.get $b)) (i64.const 0x3FFFFFFFFFFFFFFF)))
        (local.set $a (local.get $b))
        (local.set $b (local.get $t))
        (local.set $n (i64.sub (local.get $n) (i64.const 1)))
        (br $loop)))
    (i32.const 1)))
