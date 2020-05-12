use crate::isolate::Isolate;
use crate::HandleScope;
use crate::Integer;
use crate::Local;
use crate::Number;

extern "C" {
  fn v8__Number__New(isolate: *mut Isolate, value: f64) -> *const Number;
  fn v8__Number__Value(this: *const Number) -> f64;
  fn v8__Integer__New(isolate: *mut Isolate, value: i32) -> *const Integer;
  fn v8__Integer__NewFromUnsigned(
    isolate: *mut Isolate,
    value: u32,
  ) -> *const Integer;
  fn v8__Integer__Value(this: *const Integer) -> i64;
}

impl Number {
  pub fn new<'sc>(
    scope: &mut HandleScope<'sc>,
    value: f64,
  ) -> Local<'sc, Number> {
    let local = unsafe { v8__Number__New(scope.isolate(), value) };
    unsafe { scope.to_local(local) }.unwrap()
  }

  pub fn value(&self) -> f64 {
    unsafe { v8__Number__Value(self) }
  }
}

impl Integer {
  pub fn new<'sc>(
    scope: &mut HandleScope<'sc>,
    value: i32,
  ) -> Local<'sc, Integer> {
    let local = unsafe { v8__Integer__New(scope.isolate(), value) };
    unsafe { scope.to_local(local) }.unwrap()
  }

  pub fn new_from_unsigned<'sc>(
    scope: &mut HandleScope<'sc>,
    value: u32,
  ) -> Local<'sc, Integer> {
    let local = unsafe { v8__Integer__NewFromUnsigned(scope.isolate(), value) };
    unsafe { scope.to_local(local) }.unwrap()
  }

  pub fn value(&self) -> i64 {
    unsafe { v8__Integer__Value(self) }
  }
}
