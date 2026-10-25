;; Adversarial LogicSig: grow linear memory from one page to the 4 MiB cap.
;; Measures runtime allocation/zeroing and memory.grow fuel.
(module
  (memory (export "memory") 1)
  (func (export "program") (result i32)
    (local $r i32)
    (block $done
      (loop $loop
        (local.set $r (memory.grow (i32.const 1)))
        (br_if $done (i32.eq (local.get $r) (i32.const -1)))
        (br $loop)))
    (i32.const 1)))
