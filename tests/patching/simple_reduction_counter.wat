;; Input
(module
    (func (export "hello") (result i32)
        i32.const 45
    )
)

;; EXPECTED-RESULT:
(module
    (global (;0 reduction counter ;) (mut i32) (i32.const 0))
    (type (;0 yield type ;) (func))
    (import "lunatic" "yield" (func (;0;) (type 0)))

    (func (;1;) (result i32)
        block  ;; Reduction counter logic
            global.get 0
            i32.const 1
            i32.add
            global.set 0
            global.get 0
            i32.const 100000
            i32.gt_s
            if
                call 0
                i32.const 0
                global.set 0
            else
            end
        end
        i32.const 45
    )

    (export "hello" (func 1))
)