use super::*;

extern "C" {
  fn v8__Context__GetIsolate(this: *const Context) -> *mut Isolate;
  fn v8__FunctionCallbackInfo__GetIsolate(
    this: &FunctionCallbackInfo,
  ) -> *mut Isolate;
  fn v8__Message__GetIsolate(this: *const Message) -> *mut Isolate;
  fn v8__Object__GetIsolate(this: *const Object) -> *mut Isolate;
  fn v8__PropertyCallbackInfo__GetIsolate(
    this: &PropertyCallbackInfo,
  ) -> *mut Isolate;
}

/// Internal trait for retrieving a raw Isolate pointer from various V8
/// API objects.
pub trait GetRawIsolate {
  fn get_raw_isolate(&self) -> *mut Isolate;
}

impl<T> GetRawIsolate for &T
where
  T: GetRawIsolate,
{
  fn get_raw_isolate(&self) -> *mut Isolate {
    <T as GetRawIsolate>::get_raw_isolate(*self)
  }
}

impl<'s, T> GetRawIsolate for Local<'s, T>
where
  Local<'s, Object>: From<Local<'s, T>>,
{
  fn get_raw_isolate(&self) -> *mut Isolate {
    let local = Local::<'s, Object>::from(*self);
    (&*local).get_raw_isolate()
  }
}

impl<'s> GetRawIsolate for Local<'s, Context> {
  fn get_raw_isolate(&self) -> *mut Isolate {
    (&**self).get_raw_isolate()
  }
}

impl<'s> GetRawIsolate for Local<'s, Message> {
  fn get_raw_isolate(&self) -> *mut Isolate {
    (&**self).get_raw_isolate()
  }
}

impl GetRawIsolate for Context {
  fn get_raw_isolate(&self) -> *mut Isolate {
    unsafe { v8__Context__GetIsolate(self) }
  }
}

impl GetRawIsolate for Message {
  fn get_raw_isolate(&self) -> *mut Isolate {
    unsafe { v8__Message__GetIsolate(self) }
  }
}

impl GetRawIsolate for Object {
  fn get_raw_isolate(&self) -> *mut Isolate {
    unsafe { v8__Object__GetIsolate(self) }
  }
}

impl<'a> GetRawIsolate for PromiseRejectMessage<'a> {
  fn get_raw_isolate(&self) -> *mut Isolate {
    self.get_promise().get_raw_isolate()
  }
}

impl GetRawIsolate for FunctionCallbackInfo {
  fn get_raw_isolate(&self) -> *mut Isolate {
    unsafe { v8__FunctionCallbackInfo__GetIsolate(self) }
  }
}

impl GetRawIsolate for PropertyCallbackInfo {
  fn get_raw_isolate(&self) -> *mut Isolate {
    unsafe { v8__PropertyCallbackInfo__GetIsolate(self) }
  }
}
