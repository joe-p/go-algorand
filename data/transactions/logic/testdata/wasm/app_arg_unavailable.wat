;; A WASM program that tries to use the signature-only `arg_count` host
;; function. As an application program it must trap, matching the AVM, where the
;; arg opcodes are signature-only.
(module
  (import "env" "arg_count" (func $arg_count (result i32)))
  (func (export "program") (result i32) (call $arg_count)))
