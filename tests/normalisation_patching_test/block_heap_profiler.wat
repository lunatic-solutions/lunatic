;; NOTE expected result is long due too reduction counter logic
;; Input
(module
    (type (;0;) (func))
    (type (;1;) (func (param i32) (result i32)))
    (type (;2;) (func (param i32 i32)))
    (func $malloc (type 1) (param i32) (result i32)
        block ;; test block
            i32.const 0
            block ;; test nested block
                i32.const 0
                i32.eqz
                if ;; test if block
                    i32.const 0
                    drop
                else
                    br 0 ;; test br statement
                end
                loop ;; test loop block
                    i32.const 0
                    i32.eqz
                    br_if 2 ;; test br if statement
                end
            end
            drop
        end
        i32.const 0
    )
)

;; EXPECTED-RESULT:
(module
    (type (;0;) (func))
    (type (;1;) (func (param i32) (result i32)))
    (type (;2;) (func (param i32 i32)))
    (import "lunatic" "yield_" (func (;0;) (type 0)))
    (import "heap_profiler" "malloc_profiler" (func (;1;) (type 2)))
    (func $malloc_wrap (type 1) (param i32) (result i32)
        block  ;; Reduction counter logic
            global.get 0
            i32.const 1
            i32.add
            global.set 0
            global.get 0
            i32.const 10000
            i32.gt_s
            if
                call 0
                i32.const 0
                global.set 0
            else
            end
        end
        block ;; test block
            i32.const 0
            block ;; test nested block
                i32.const 0
                i32.eqz
                if ;; test if block
                    i32.const 0
                    drop
                else
                    br 0 ;; test jmp statement
                end
                loop ;; test loop block
                    block  ;; Reduction counter logic
                        global.get 0
                        i32.const 1
                        i32.add
                        global.set 0
                        global.get 0
                        i32.const 10000
                        i32.gt_s
                        if
                            call 0
                            i32.const 0
                            global.set 0
                        else
                        end
                    end
                    i32.const 0
                    i32.eqz
                    br_if 2 ;; test jmp if statement
                end
            end
            drop
        end
        i32.const 0
    )
    (func $malloc (type 1) (param i32) (result i32)
          (local i32)
          local.get 0
          call $malloc_wrap
          local.set 1
          local.get 0
          local.get 1
          call 1
          local.get 1
    )
    (global (;0;) (mut i32) (i32.const 0))
)
