;; NOTE expected result is long due too reduction counter logic
;; Input
(module
    (type (;0;) (func))
    (type (;1;) (func (param i32)))
    (type (;2;) (func (param i32) (result i32)))
    (type (;3;) (func (param i32 i32)))
    (type (;4;) (func (param i32 i32) (result i32)))
    (type (;5;) (func (param i32 i32 i32)))
    (func $malloc (type 2) (param i32) (result i32)
        i32.const 0
    )
    (func $aligned_alloc (type 4) (param i32 i32) (result i32)
        i32.const 0
    )
    (func $calloc (type 4) (param i32 i32) (result i32)
        i32.const 0
    )
    (func $realloc (type 4) (param i32 i32) (result i32)
        i32.const 0
    )
    (func $free (type 1) (param i32)
    )
)

;; EXPECTED-RESULT:
(module
    (type (;0;) (func))
    (type (;1;) (func (param i32)))
    (type (;2;) (func (param i32) (result i32)))
    (type (;3;) (func (param i32 i32)))
    (type (;4;) (func (param i32 i32) (result i32)))
    (type (;5;) (func (param i32 i32 i32)))
    (import "lunatic" "yield_" (func (;0;) (type 0)))
    (import "heap_profiler" "malloc_profiler" (func (;1;) (type 3)))
    (import "heap_profiler" "aligned_alloc_profiler" (func (;2;) (type 5)))
    (import "heap_profiler" "calloc_profiler" (func (;3;) (type 5)))
    (import "heap_profiler" "realloc_profiler" (func (;4;) (type 5)))
    (import "heap_profiler" "free_profiler" (func (;5;) (type 1)))
    (func $malloc_wrap (type 2) (param i32) (result i32)
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
    )
    (func $aligned_alloc_wrap (type 4) (param i32 i32) (result i32)
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
    )
    (func $calloc_wrap (type 4) (param i32 i32) (result i32)
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
    )
    (func $realloc_wrap (type 4) (param i32 i32) (result i32)
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
    )
    (func $free_wrap (type 1) (param i32)
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
    )
    (func $aligned_alloc (type 4) (param i32 i32) (result i32)
          (local i32)
          local.get 0
          local.get 1
          call $aligned_alloc_wrap
          local.set 2
          local.get 0
          local.get 1
          local.get 2
          call 2
          local.get 2
    )
    (func $calloc (type 4) (param i32 i32) (result i32)
          (local i32)
          local.get 0
          local.get 1
          call $calloc_wrap
          local.set 2
          local.get 0
          local.get 1
          local.get 2
          call 3
          local.get 2
    )
    (func $realloc (type 4) (param i32 i32) (result i32)
          (local i32)
          local.get 0
          local.get 1
          call $realloc_wrap
          local.set 2
          local.get 0
          local.get 1
          local.get 2
          call 4
          local.get 2
    )
    (func $malloc (type 2) (param i32) (result i32)
          (local i32)
          local.get 0
          call $malloc_wrap
          local.set 1
          local.get 0
          local.get 1
          call 1
          local.get 1
    )
    (func $free (type 1) (param i32)
          local.get 0
          call $free_wrap
          local.get 0
          call 5
    )
    (global (;0;) (mut i32) (i32.const 0))
)
