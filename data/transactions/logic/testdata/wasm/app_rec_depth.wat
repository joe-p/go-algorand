;; A WASM application program that recurses to depth N, where N is
;; ApplicationArgs[0] (4-byte little-endian), doing O(1) work per frame.
;;
;; Unlike recursive fib, the number of calls equals the depth, so cost growth is
;; the cost of the call stack itself rather than the shape of the recursion
;; tree. The result is written to the uint global "depth" to keep the work
;; observable. Paired with app_depth_loop.wat (an iterative baseline) and with
;; the TEAL `callsub` recursion in wasm_app_bench_test.go.
(module
  (import "env" "txna_bytes" (func $arg (param i32 i32 i32 i32) (result i32)))
  (import "env" "app_global_put_uint" (func $put (param i32 i32 i64)))
  (memory (export "memory") 1)
  (data (i32.const 0) "depth")
  ;; rec(n) = 0 for n == 0, else 1 + rec(n-1).
  (func $rec (param $n i32) (result i32)
    (if (result i32) (i32.eqz (local.get $n))
      (then (i32.const 0))
      (else (i32.add (i32.const 1) (call $rec (i32.sub (local.get $n) (i32.const 1)))))))
  (func (export "program") (result i32)
    (local $n i32)
    ;; read ApplicationArgs[0] into offset 16 as 4 little-endian bytes
    (drop (call $arg (i32.const 26) (i32.const 0) (i32.const 16) (i32.const 4)))
    (local.set $n (i32.load (i32.const 16)))
    (call $put (i32.const 0) (i32.const 5)
      (i64.extend_i32_u (call $rec (local.get $n))))
    (i32.const 1)))
