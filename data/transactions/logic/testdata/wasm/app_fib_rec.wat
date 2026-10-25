;; A WASM application program that computes fib(N) recursively, where N is
;; ApplicationArgs[0] (4-byte little-endian). It is the recursive counterpart to
;; app_fib.wat, which computes the same sequence in a loop.
;;
;; The result is written to the uint global "fib", so the recursion cannot be
;; dead code and tests can check the value. Paired with the TEAL `callsub`
;; recursion in wasm_app_bench_test.go, for comparing call-frame costs.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "fib")
  ;; fib(n) = n for n < 2, else fib(n-1) + fib(n-2). N is kept small enough
  ;; that the i64 result cannot overflow.
  (func $fib (param $n i32) (result i64)
    (if (result i64) (i32.lt_u (local.get $n) (i32.const 2))
      (then (i64.extend_i32_u (local.get $n)))
      (else
        (i64.add
          (call $fib (i32.sub (local.get $n) (i32.const 1)))
          (call $fib (i32.sub (local.get $n) (i32.const 2)))))))
  (func (export "program") (result i32)
    (local $n i32)
    ;; read ApplicationArgs[0] into offset 16 as 4 little-endian bytes
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (call $put (i32.const 0) (i32.const 3) (call $fib (local.get $n)))
    (i32.const 1)))
