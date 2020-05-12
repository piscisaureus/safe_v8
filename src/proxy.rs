use crate::Context;
use crate::HandleScope;
use crate::Local;
use crate::Object;
use crate::Proxy;
use crate::Value;

extern "C" {
  fn v8__Proxy__New(
    context: *const Context,
    target: *const Object,
    handler: *const Object,
  ) -> *const Proxy;
  fn v8__Proxy__GetHandler(this: *const Proxy) -> *const Value;
  fn v8__Proxy__GetTarget(this: *const Proxy) -> *const Value;
  fn v8__Proxy__IsRevoked(this: *const Proxy) -> bool;
  fn v8__Proxy__Revoke(this: *const Proxy);
}

impl Proxy {
  pub fn new<'sc>(
    scope: &mut HandleScope<'sc>,
    context: Local<Context>,
    target: Local<Object>,
    handler: Local<Object>,
  ) -> Option<Local<'sc, Proxy>> {
    unsafe {
      let ptr = v8__Proxy__New(&*context, &*target, &*handler);
      scope.to_local(ptr)
    }
  }

  pub fn get_handler<'sc>(
    &mut self,
    scope: &mut HandleScope<'sc>,
  ) -> Local<'sc, Value> {
    unsafe { scope.to_local(v8__Proxy__GetHandler(&*self)) }.unwrap()
  }

  pub fn get_target<'sc>(
    &mut self,
    scope: &mut HandleScope<'sc>,
  ) -> Local<'sc, Value> {
    unsafe { scope.to_local(v8__Proxy__GetTarget(&*self)) }.unwrap()
  }

  pub fn is_revoked(&mut self) -> bool {
    unsafe { v8__Proxy__IsRevoked(self) }
  }

  pub fn revoke(&mut self) {
    unsafe { v8__Proxy__Revoke(self) };
  }
}
