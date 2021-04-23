(module
  (type (;0;) (func))
  (type (;1;) (func (result i32)))
  (func (;0;) (type 0)
    i32.const 0
    i32.const 0
    i32.load8_u
    i32.const 1
    i32.add
    i32.store8)
  (func (;1;) (type 1) (result i32)
    i32.const 0
    i32.load8_u
    return)
  (func (;2;) (type 0)
    call 0
    call 0
    call 0)
  (memory (;0;) 1 1)
  (export "inc" (func 0))
  (export "get" (func 1))
  (start 2)
  (data (;0;) (i32.const 0) "A"))