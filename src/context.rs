// Copyright 2018-2019 the Deno authors. All rights reserved. MIT license.
use std::mem::MaybeUninit;

use crate::isolate::Isolate;
use crate::scope::Scope;
use crate::scope::Scoped;
use crate::support::Opaque;
use crate::InIsolate;
use crate::Local;
use crate::Object;
use crate::ToLocal;

extern "C" {
  fn v8__Context__New(isolate: &Isolate) -> *mut Context;
  fn v8__Context__Enter(this: &mut Context);
  fn v8__Context__Exit(this: &mut Context);
  fn v8__Context__GetIsolate<'sc>(this: &'sc Context) -> &'sc mut Isolate;
  fn v8__Context__Global(this: *mut Context) -> *mut Object;
}

/// A sandboxed execution context with its own set of built-in objects and
/// functions.
#[repr(C)]
pub struct Context(Opaque);

impl Context {
  pub fn new<'sc>(scope: &mut impl ToLocal<'sc>) -> Local<'sc, Context> {
    // TODO: optional arguments;
    let ptr = unsafe { v8__Context__New(scope.isolate()) };
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
    &mut self,
    scope: &mut impl ToLocal<'sc>,
  ) -> Local<'sc, Object> {
    unsafe { scope.to_local(v8__Context__Global(&mut *self)) }.unwrap()
  }

  /// Enter this context.  After entering a context, all code compiled
  /// and run is compiled and run in this context.  If another context
  /// is already entered, this old context is saved so it can be
  /// restored when the new context is exited.
  fn enter(&mut self) {
    // TODO: enter/exit should be controlled by a scope.
    unsafe { v8__Context__Enter(self) };
  }

  /// Exit this context.  Exiting the current context restores the
  /// context that was in place when entering the current context.
  fn exit(&mut self) {
    // TODO: enter/exit should be controlled by a scope.
    unsafe { v8__Context__Exit(self) };
  }

  pub fn get_isolate(&mut self) -> &mut Isolate {
    unsafe { v8__Context__GetIsolate(self) }
  }
}

pub struct ContextScope<'s> {
  context: Local<'s, Context>,
}

unsafe impl<'s> Scoped<'s> for ContextScope<'s> {
  type Args = Local<'s, Context>;
  fn enter_scope(buf: &mut MaybeUninit<Self>, mut context: Local<'s, Context>) {
    *buf = MaybeUninit::new(ContextScope { context });
    context.enter();
  }
}

impl<'s> Drop for ContextScope<'s> {
  fn drop(&mut self) {
    self.context.exit();
  }
}

impl<'s> ContextScope<'s> {
  pub fn new(context: Local<'s, Context>) -> Scope<Self> {
    Scope::new(context)
  }
}

impl<'s> InIsolate for crate::scope::Entered<'s, ContextScope<'s>> {
  fn isolate(&mut self) -> &mut Isolate {
    self.context.get_isolate()
  }
}
