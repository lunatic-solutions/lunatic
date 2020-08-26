(module
            (import "env" "minus_42" (func $minus_42 (param i32)))

            (func (export "hello")
                i32.const 45
                call $minus_42)
        )