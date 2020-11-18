;; Input
(module
  (import "wasi_snapshot_preview1" "path_open"
    ;; Lunatic API signature: (externref i32 i32 i32 i32 i64 i64 i32) -> (i32 externref)
    (func $path_open (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (memory (;0;) 32)
)

;; EXPECTED-RESULT:
(module
  (type (;0;) (func))
  (type (;1;) (func (result i32)))
  (type (;2;) (func (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)))
  (type (;3;) (func (param externref) (result i32)))
  (type (;4;) (func (param externref i32 i32 i32 i32 i64 i64 i32) (result i32 externref)))

  (import "lunatic" "yield" (func (;0;) (type 0)))
  (import "lunatic" "get_externref_free_slot" (func (;1;) (type 1)))
  (import "wasi_snapshot_preview1" "path_open" (func (;2;) (type 4)))
  (import "lunatic" "memory" (memory (;0;) 32))

  ;; path_open wrapper
  (func $path_open (type 2) (param i32 i32 i32 i32 i32 i64 i64 i32 i32) (result i32)
    (local i32)

    local.get 0
    table.get 0
    local.get 1
    local.get 2
    local.get 3
    local.get 4
    local.get 5
    local.get 6
    local.get 7
    call 2
    call 4
    local.set 9
    local.get 8
    local.get 9
    i32.store align=1)

  ;; Save externref
  (func (;4;) (type 3) (param externref) (result i32)
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

  (table (;0;) 4 externref)
  (global (;0;) (mut i32) (i32.const 0))
  (export "__lunatic_externref_resource_table" (table 0))
  (export "_lunatic_externref_save" (func 4)))