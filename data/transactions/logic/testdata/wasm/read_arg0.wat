;; A WASM LogicSig that approves only if it has exactly one argument, and reads
;; that argument so the arg-access rule is satisfied.
(module
  (import "env" "arg_count" (func $arg_count (result i32)))
  (import "env" "arg" (func $arg (param i32 i32 i32) (result i32)))
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    ;; read arg 0 into offset 0, up to 32 bytes
    i32.const 0
    i32.const 0
    i32.const 32
    call $arg
    drop
    ;; approve iff arg_count == 1
    call $arg_count
    i32.const 1
    i32.eq))
