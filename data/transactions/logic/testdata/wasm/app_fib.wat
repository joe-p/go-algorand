;; A WASM application program that computes fib(N) in a loop for the compute-bound
;; crossover benchmark, where N is ApplicationArgs[0] (4-byte little-endian).
;; Values are masked to 2^62 so the computation cannot overflow. It is paired
;; with the TEAL fib program in wasm_app_bench_test.go.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $n i32)
    (local $a i64)
    (local $b i64)
    (local $t i64)
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (local.set $a (i64.const 0))
    (local.set $b (i64.const 1))
    (block $done
      (loop $l
        (br_if $done (i32.eqz (local.get $n)))
        (local.set $t (i64.add (local.get $a) (local.get $b)))
        (local.set $t (i64.and (local.get $t) (i64.const 0x3FFFFFFFFFFFFFFF)))
        (local.set $a (local.get $b))
        (local.set $b (local.get $t))
        (local.set $n (i32.sub (local.get $n) (i32.const 1)))
        (br $l)))
    (i32.const 1)))
