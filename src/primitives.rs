use crate::isolate::Isolate;
use crate::Boolean;
use crate::HandleScope;
use crate::Local;
use crate::Primitive;

extern "C" {
  fn v8__Null(isolate: *mut Isolate) -> *const Primitive;
  fn v8__Undefined(isolate: *mut Isolate) -> *const Primitive;
  fn v8__Boolean__New(isolate: *mut Isolate, value: bool) -> *const Boolean;
}

pub fn null<'s>(scope: &mut HandleScope<'s, ()>) -> Local<'s, Primitive> {
  unsafe { scope.cast_local(|scope| v8__Null(scope.get_isolate_ptr())) }
    .unwrap()
}

pub fn undefined<'s>(scope: &mut HandleScope<'s, ()>) -> Local<'s, Primitive> {
  unsafe { scope.cast_local(|scope| v8__Undefined(scope.get_isolate_ptr())) }
    .unwrap()
}

impl Boolean {
  pub fn new<'s>(
    scope: &mut HandleScope<'s, ()>,
    value: bool,
  ) -> Local<'s, Boolean> {
    unsafe {
      scope.cast_local(|scope| v8__Boolean__New(scope.get_isolate_ptr(), value))
    }
    .unwrap()
  }
}
