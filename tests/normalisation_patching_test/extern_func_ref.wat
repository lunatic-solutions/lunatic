;; Input
(module
  ;; Import expects a i32, but the API for `lunatic::spawn` returns an Externref.
  ;; This will tell the normaliser to generate wrappers.
  (import "lunatic" "spawn" (func (;0;) (param i32 i32 i64) (result i32)))
  ;; If the normaliser sees the import `lunatic::drop_externref`, it will replace
  ;; it with an in-place drop function.
  (import "lunatic" "drop_externref" (func (;1;) (param i32)))
  (func $test (result i32)
    i32.const 0
    i32.const 0
    i64.const 0
    call 0)
)

;; EXPECTED-RESULT:
(module
  (type (;0;) (func))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32)))
  (type (;3;) (func (param i32 i32 i64) (result i32)))
  (type (;4;) (func (param i32 i32 i64) (result externref)))
  (type (;5;) (func (param externref) (result i32)))

  (import "lunatic" "yield" (func (;0;) (type 0)))
  (import "lunatic" "get_externref_free_slot" (func (;1;) (type 1)))
  (import "lunatic" "set_externref_free_slot" (func (;2;) (type 2)))
  (import "lunatic" "spawn" (func (;3;) (type 4)))

  (func $test (type 1) (result i32)
    block  ;; label = @1
      global.get 0
      i32.const 1
      i32.add
      global.set 0
      global.get 0
      i32.const 10000
      i32.gt_s
      if  ;; label = @2
        call 0
        i32.const 0
        global.set 0
      else
      end
    end
    i32.const 0
    i32.const 0
    i64.const 0
    call 6)
  
  ;; Save externref
  (func (;5;) (type 5) (param externref) (result i32)
    (local i32)
    call 1
    local.tee 1
    table.size 0
    i32.eq
    if (result i32)  ;; label = @1",
      ref.null extern
      table.size 0
      table.grow 0
    else
      local.get 1
    end
    local.get 0
    table.set 0
    local.get 1)

  ;; Swap import wrapper
  (func (;6;) (type 3) (param i32 i32 i64) (result i32)
    local.get 0
    local.get 1
    local.get 2
    call 3
    call 5)
  
  ;; Drop externref
  (func (;7;) (type 2) (param i32)
    local.get 0
    ref.null extern
    table.set 0
    local.get 0
    call 2)

  (table (;0;) 4 externref)
  (global (;0;) (mut i32) (i32.const 0))
  (export "__lunatic_externref_resource_table" (table 0))
  (export "_lunatic_externref_save" (func 5))
  (export "_lunatic_externref_drop" (func 7)))