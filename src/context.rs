// Copyright 2019-2020 the Deno authors. All rights reserved. MIT license.
use crate::isolate::Isolate;
use crate::Context;
use crate::HandleScope;
use crate::Local;
use crate::Object;
use crate::ObjectTemplate;
use crate::Value;
use std::ptr::null;

extern "C" {
  fn v8__Context__New(
    isolate: *mut Isolate,
    templ: *const ObjectTemplate,
    global_object: *const Value,
  ) -> *const Context;
  fn v8__Context__Enter(this: *const Context);
  fn v8__Context__Exit(this: *const Context);
  fn v8__Context__Global(this: *const Context) -> *const Object;
}

impl Context {
  /// Creates a new context.
  pub fn new<'sc>(scope: &mut HandleScope<'sc>) -> Local<'sc, Context> {
    // TODO: optional arguments;
    let ptr = unsafe { v8__Context__New(scope.isolate(), null(), null()) };
    unsafe { scope.to_local(ptr) }.unwrap()
  }

  /// Creates a new context using the object template as the template for
  /// the global object.
  pub fn new_from_template<'sc>(
    scope: &mut HandleScope<'sc>,
    templ: Local<ObjectTemplate>,
  ) -> Local<'sc, Context> {
    let ptr = unsafe { v8__Context__New(scope.isolate(), &*templ, null()) };
    unsafe { scope.to_local(ptr) }.unwrap()
  }

  /// Returns the global proxy object.
  ///
  /// Global proxy object is a thin wrapper whose prototype points to actual
  /// context's global object with the properties like Object, etc. This is done
  /// that way for security reasons (for more details see
  /// https://wiki.mozilla.org/Gecko:SplitWindow).
  ///
  /// Please note that changes to global proxy object prototype most probably
  /// would break VM---v8 expects only global object as a prototype of global
  /// proxy object.
  pub fn global<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Local<'sc, Object> {
    unsafe { scope.to_local(v8__Context__Global(self)) }.unwrap()
  }

  /// Enter this context.  After entering a context, all code compiled
  /// and run is compiled and run in this context.  If another context
  /// is already entered, this old context is saved so it can be
  /// restored when the new context is exited.
  pub(crate) fn enter(&self) {
    // TODO: enter/exit should be controlled by a scope.
    unsafe { v8__Context__Enter(self) };
  }

  /// Exit this context.  Exiting the current context restores the
  /// context that was in place when entering the current context.
  pub(crate) fn exit(&self) {
    // TODO: enter/exit should be controlled by a scope.
    unsafe { v8__Context__Exit(self) };
  }
}
