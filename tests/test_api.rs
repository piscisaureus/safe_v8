// Copyright 2019-2020 the Deno authors. All rights reserved. MIT license.

#[macro_use]
extern crate lazy_static;

use std::convert::{Into, TryFrom, TryInto};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rusty_v8 as v8;
// TODO(piscisaureus): Ideally there would be no need to import this trait.
use v8::MapFnTo;

lazy_static! {
  static ref INIT_LOCK: Mutex<u32> = Mutex::new(0);
}

#[must_use]
struct SetupGuard {}

impl Drop for SetupGuard {
  fn drop(&mut self) {
    // TODO shutdown process cleanly.
  }
}

fn setup() -> SetupGuard {
  let mut g = INIT_LOCK.lock().unwrap();
  *g += 1;
  if *g == 1 {
    v8::V8::initialize_platform(v8::new_default_platform().unwrap());
    v8::V8::initialize();
  }
  SetupGuard {}
}

#[test]
fn handle_scope_nested() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope1 = &mut v8::HandleScope::new(isolate);
    {
      let _scope2 = &mut v8::HandleScope::new(scope1);
    }
  }
}

#[test]
#[allow(clippy::float_cmp)]
fn handle_scope_numbers() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope1 = &mut v8::HandleScope::new(isolate);
    let l1 = v8::Integer::new(scope1, -123);
    let l2 = v8::Integer::new_from_unsigned(scope1, 456);
    {
      let scope2 = &mut v8::HandleScope::new(scope1);
      let l3 = v8::Number::new(scope2, 78.9);
      assert_eq!(l1.value(), -123);
      assert_eq!(l2.value(), 456);
      assert_eq!(l3.value(), 78.9);
      assert_eq!(v8::Number::value(&l1), -123f64);
      assert_eq!(v8::Number::value(&l2), 456f64);
    }
  }
}

#[test]
fn handle_scope_non_lexical_lifetime() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let scope1 = &mut v8::HandleScope::new(isolate);

  // Despite `local` living slightly longer than `scope2`, this test should
  // not crash.
  let local = {
    let scope2 = &mut v8::HandleScope::new(scope1);
    v8::Integer::new(scope2, 123)
  };
  assert_eq!(local.value(), 123);
}

#[test]
fn global_handles() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let g1: v8::Global<v8::String>;
  let mut g2: Option<v8::Global<v8::Integer>> = None;
  let g3: v8::Global<v8::Integer>;
  let g4: v8::Global<v8::Integer>;
  let mut g5: Option<v8::Global<v8::Integer>> = None;
  let g6;
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let l1 = v8::String::new(scope, "bla").unwrap();
    let l2 = v8::Integer::new(scope, 123);
    g1 = v8::Global::new(scope, l1);
    g2.replace(v8::Global::new(scope, l2));
    g3 = v8::Global::new(scope, g2.as_ref().unwrap());
    g4 = v8::Global::new(scope, l2);
    let l5 = v8::Integer::new(scope, 100);
    g5.replace(v8::Global::new(scope, l5));
    g6 = g1.clone();
  }
  {
    let scope = &mut v8::HandleScope::new(isolate);
    assert_eq!(g1.get(scope).to_rust_string_lossy(scope), "bla");
    assert_eq!(g2.as_ref().unwrap().get(scope).value(), 123);
    assert_eq!(g3.get(scope).value(), 123);
    assert_eq!(g4.get(scope).value(), 123);
    {
      let num = g5.as_ref().unwrap().get(scope);
      assert_eq!(num.value(), 100);
    }
    g5.take();
    assert!(g6 == g1);
    assert_eq!(g6.get(scope).to_rust_string_lossy(scope), "bla");
  }
}

#[test]
fn global_handle_drop() {
  let _setup_guard = setup();

  // Global 'g1' will be dropped _after_ the Isolate has been disposed.
  let _g1: v8::Global<v8::String>;

  let isolate = &mut v8::Isolate::new(Default::default());
  let scope = &mut v8::HandleScope::new(isolate);

  let l1 = v8::String::new(scope, "foo").unwrap();
  _g1 = v8::Global::new(scope, l1);

  // Global 'g2' will be dropped _before_ the Isolate has been disposed.
  let l2 = v8::String::new(scope, "bar").unwrap();
  let _g2 = v8::Global::new(scope, l2);
}

#[test]
fn test_string() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let reference = "Hello 🦕 world!";
    let local = v8::String::new(scope, reference).unwrap();
    assert_eq!(15, local.length());
    assert_eq!(17, local.utf8_length(scope));
    assert_eq!(reference, local.to_rust_string_lossy(scope));
  }
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let local = v8::String::empty(scope);
    assert_eq!(0, local.length());
    assert_eq!(0, local.utf8_length(scope));
    assert_eq!("", local.to_rust_string_lossy(scope));
  }
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let local =
      v8::String::new_from_utf8(scope, b"", v8::NewStringType::Normal).unwrap();
    assert_eq!(0, local.length());
    assert_eq!(0, local.utf8_length(scope));
    assert_eq!("", local.to_rust_string_lossy(scope));
  }
}

#[test]
#[allow(clippy::float_cmp)]
fn escapable_handle_scope() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let handle_scope = &mut v8::HandleScope::new(isolate);

    // After dropping EscapableHandleScope, we should be able to
    // read escaped values.
    let number = {
      let escapable_scope = &mut v8::EscapableHandleScope::new(handle_scope);
      let number = v8::Number::new(escapable_scope, 78.9);
      escapable_scope.escape(number)
    };
    assert_eq!(number.value(), 78.9);

    let string = {
      let escapable_scope = &mut v8::EscapableHandleScope::new(handle_scope);
      let string = v8::String::new(escapable_scope, "Hello 🦕 world!").unwrap();
      escapable_scope.escape(string)
    };
    assert_eq!("Hello 🦕 world!", string.to_rust_string_lossy(handle_scope));

    let string = {
      let escapable_scope = &mut v8::EscapableHandleScope::new(handle_scope);
      let nested_str_val = {
        let nested_escapable_scope =
          &mut v8::EscapableHandleScope::new(escapable_scope);
        let string =
          v8::String::new(nested_escapable_scope, "Hello 🦕 world!").unwrap();
        nested_escapable_scope.escape(string)
      };
      escapable_scope.escape(nested_str_val)
    };
    assert_eq!("Hello 🦕 world!", string.to_rust_string_lossy(handle_scope));
  }
}

#[test]
#[should_panic(expected = "EscapableHandleScope::escape() called twice")]
fn escapable_handle_scope_can_escape_only_once() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  let scope1 = &mut v8::HandleScope::new(isolate);
  let scope2 = &mut v8::EscapableHandleScope::new(scope1);

  let local1 = v8::Integer::new(scope2, -123);
  let escaped1 = scope2.escape(local1);
  assert!(escaped1 == local1);

  let local2 = v8::Integer::new(scope2, 456);
  let escaped2 = scope2.escape(local2);
  assert!(escaped2 == local2);
}

#[test]
fn context_scope() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  let scope = &mut v8::HandleScope::new(isolate);
  let context1 = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context1);

  assert!(scope.get_current_context() == context1);
  assert!(scope.get_entered_or_microtask_context() == context1);

  {
    let context2 = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context2);

    assert!(scope.get_current_context() == context2);
    assert!(scope.get_entered_or_microtask_context() == context2);
  }

  assert!(scope.get_current_context() == context1);
  assert!(scope.get_entered_or_microtask_context() == context1);
}

#[test]
#[should_panic(
  expected = "HandleScope<()> and Context do not belong to the same Isolate"
)]
fn context_scope_param_and_context_must_share_isolate() {
  let _setup_guard = setup();
  let isolate1 = &mut v8::Isolate::new(Default::default());
  let isolate2 = &mut v8::Isolate::new(Default::default());
  let scope1 = &mut v8::HandleScope::new(isolate1);
  let scope2 = &mut v8::HandleScope::new(isolate2);
  let context1 = v8::Context::new(scope1);
  let context2 = v8::Context::new(scope2);
  let _context_scope_12 = &mut v8::ContextScope::new(scope1, context2);
  let _context_scope_21 = &mut v8::ContextScope::new(scope2, context1);
}

#[test]
fn microtasks() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  isolate.run_microtasks();

  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
    let function = v8::Function::new(
      scope,
      |_: &mut v8::HandleScope,
       _: v8::FunctionCallbackArguments,
       _: v8::ReturnValue| {
        CALL_COUNT.fetch_add(1, Ordering::SeqCst);
      },
    )
    .unwrap();
    scope.enqueue_microtask(function);

    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 0);
    scope.run_microtasks();
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
  }
}

#[test]
fn get_isolate_from_handle() {
  extern "C" {
    fn v8__internal__GetIsolateFromHeapObject(
      location: *const v8::Data,
    ) -> *mut v8::Isolate;
  }

  fn check_handle_helper(
    isolate: &mut v8::Isolate,
    expect_some: Option<bool>,
    local: v8::Local<v8::Data>,
  ) {
    let isolate_ptr = NonNull::from(isolate);
    let maybe_ptr = unsafe { v8__internal__GetIsolateFromHeapObject(&*local) };
    let maybe_ptr = NonNull::new(maybe_ptr);
    if let Some(ptr) = maybe_ptr {
      assert_eq!(ptr, isolate_ptr);
    }
    if let Some(expected_some) = expect_some {
      assert_eq!(maybe_ptr.is_some(), expected_some);
    }
  };

  fn check_handle<'s, F, D>(
    scope: &mut v8::HandleScope<'s>,
    expect_some: Option<bool>,
    f: F,
  ) where
    F: Fn(&mut v8::HandleScope<'s>) -> D,
    D: Into<v8::Local<'s, v8::Data>>,
  {
    let local = f(scope).into();

    // Check that we can get the isolate from a Local.
    check_handle_helper(scope, expect_some, local);

    // Check that we can still get it after converting it to a Global.
    let global = v8::Global::new(scope, local);
    let local2 = v8::Local::new(scope, &global);
    check_handle_helper(scope, expect_some, local2);
  };

  fn check_eval<'s>(
    scope: &mut v8::HandleScope<'s>,
    expect_some: Option<bool>,
    code: &str,
  ) {
    check_handle(scope, expect_some, |scope| eval(scope, code).unwrap());
  }

  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  check_handle(scope, None, |s| v8::null(s));
  check_handle(scope, None, |s| v8::undefined(s));
  check_handle(scope, None, |s| v8::Boolean::new(s, true));
  check_handle(scope, None, |s| v8::Boolean::new(s, false));
  check_handle(scope, None, |s| v8::String::new(s, "").unwrap());
  check_eval(scope, None, "''");
  check_handle(scope, Some(true), |s| v8::String::new(s, "Words").unwrap());
  check_eval(scope, Some(true), "'Hello'");
  check_eval(scope, Some(true), "Symbol()");
  check_handle(scope, Some(true), |s| v8::Object::new(s));
  check_eval(scope, Some(true), "this");
  check_handle(scope, Some(true), |s| s.get_current_context());
  check_eval(scope, Some(true), "({ foo: 'bar' })");
  check_eval(scope, Some(true), "() => {}");
  check_handle(scope, Some(true), |s| v8::Number::new(s, 4.2f64));
  check_handle(scope, Some(true), |s| v8::Number::new(s, -0f64));
  check_handle(scope, Some(false), |s| v8::Integer::new(s, 0));
  check_eval(scope, Some(true), "3.3");
  check_eval(scope, Some(false), "3.3 / 3.3");
}

#[test]
fn array_buffer() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let ab = v8::ArrayBuffer::new(scope, 42);
    assert_eq!(42, ab.byte_length());

    let bs = v8::ArrayBuffer::new_backing_store(scope, 84);
    assert_eq!(84, bs.byte_length());
    assert_eq!(false, bs.is_shared());

    let data: Box<[u8]> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9].into_boxed_slice();
    let unique_bs = v8::ArrayBuffer::new_backing_store_from_boxed_slice(data);
    assert_eq!(10, unique_bs.byte_length());
    assert_eq!(false, unique_bs.is_shared());
    assert_eq!(unique_bs[0].get(), 0);
    assert_eq!(unique_bs[9].get(), 9);

    let shared_bs_1 = unique_bs.make_shared();
    assert_eq!(10, shared_bs_1.byte_length());
    assert_eq!(false, shared_bs_1.is_shared());
    assert_eq!(shared_bs_1[0].get(), 0);
    assert_eq!(shared_bs_1[9].get(), 9);

    let ab = v8::ArrayBuffer::with_backing_store(scope, &shared_bs_1);
    let shared_bs_2 = ab.get_backing_store();
    assert_eq!(10, shared_bs_2.byte_length());
    assert_eq!(shared_bs_2[0].get(), 0);
    assert_eq!(shared_bs_2[9].get(), 9);
  }
}

#[test]
fn backing_store_segfault() {
  let _setup_guard = setup();
  let array_buffer_allocator = v8::new_default_allocator().make_shared();
  let shared_bs = {
    assert_eq!(1, v8::SharedRef::use_count(&array_buffer_allocator));
    let params = v8::Isolate::create_params()
      .array_buffer_allocator(array_buffer_allocator.clone());
    assert_eq!(2, v8::SharedRef::use_count(&array_buffer_allocator));
    let isolate = &mut v8::Isolate::new(params);
    assert_eq!(2, v8::SharedRef::use_count(&array_buffer_allocator));
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let ab = v8::ArrayBuffer::new(scope, 10);
    let shared_bs = ab.get_backing_store();
    assert_eq!(3, v8::SharedRef::use_count(&array_buffer_allocator));
    shared_bs
  };
  assert_eq!(1, v8::SharedRef::use_count(&shared_bs));
  assert_eq!(2, v8::SharedRef::use_count(&array_buffer_allocator));
  drop(array_buffer_allocator);
  drop(shared_bs); // Error occurred here.
}

#[test]
fn shared_array_buffer_allocator() {
  let alloc1 = v8::new_default_allocator().make_shared();
  assert_eq!(1, v8::SharedRef::use_count(&alloc1));

  let alloc2 = alloc1.clone();
  assert_eq!(2, v8::SharedRef::use_count(&alloc1));
  assert_eq!(2, v8::SharedRef::use_count(&alloc2));

  let mut alloc2 = v8::SharedPtr::from(alloc2);
  assert_eq!(2, v8::SharedRef::use_count(&alloc1));
  assert_eq!(2, v8::SharedPtr::use_count(&alloc2));

  drop(alloc1);
  assert_eq!(1, v8::SharedPtr::use_count(&alloc2));

  alloc2.take();
  assert_eq!(0, v8::SharedPtr::use_count(&alloc2));
}

#[test]
fn array_buffer_with_shared_backing_store() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);

    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let ab1 = v8::ArrayBuffer::new(scope, 42);
    assert_eq!(42, ab1.byte_length());

    let bs1 = ab1.get_backing_store();
    assert_eq!(ab1.byte_length(), bs1.byte_length());
    assert_eq!(2, v8::SharedRef::use_count(&bs1));

    let bs2 = ab1.get_backing_store();
    assert_eq!(ab1.byte_length(), bs2.byte_length());
    assert_eq!(3, v8::SharedRef::use_count(&bs1));
    assert_eq!(3, v8::SharedRef::use_count(&bs2));

    let bs3 = ab1.get_backing_store();
    assert_eq!(ab1.byte_length(), bs3.byte_length());
    assert_eq!(4, v8::SharedRef::use_count(&bs1));
    assert_eq!(4, v8::SharedRef::use_count(&bs2));
    assert_eq!(4, v8::SharedRef::use_count(&bs3));

    drop(bs2);
    assert_eq!(3, v8::SharedRef::use_count(&bs1));
    assert_eq!(3, v8::SharedRef::use_count(&bs3));

    drop(bs1);
    assert_eq!(2, v8::SharedRef::use_count(&bs3));

    let ab2 = v8::ArrayBuffer::with_backing_store(scope, &bs3);
    assert_eq!(ab1.byte_length(), ab2.byte_length());
    assert_eq!(3, v8::SharedRef::use_count(&bs3));

    let bs4 = ab2.get_backing_store();
    assert_eq!(ab1.byte_length(), bs4.byte_length());
    assert_eq!(4, v8::SharedRef::use_count(&bs3));
    assert_eq!(4, v8::SharedRef::use_count(&bs4));

    let bs5 = bs4.clone();
    assert_eq!(5, v8::SharedRef::use_count(&bs3));
    assert_eq!(5, v8::SharedRef::use_count(&bs4));
    assert_eq!(5, v8::SharedRef::use_count(&bs5));

    drop(bs3);
    assert_eq!(4, v8::SharedRef::use_count(&bs4));
    assert_eq!(4, v8::SharedRef::use_count(&bs4));

    drop(bs4);
    assert_eq!(3, v8::SharedRef::use_count(&bs5));
  }
}

fn eval<'s>(
  scope: &mut v8::HandleScope<'s>,
  code: &str,
) -> Option<v8::Local<'s, v8::Value>> {
  let scope = &mut v8::EscapableHandleScope::new(scope);
  let source = v8::String::new(scope, code).unwrap();
  let script = v8::Script::compile(scope, source, None).unwrap();
  let r = script.run(scope);
  r.map(|v| scope.escape(v))
}

#[test]
fn try_catch() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    {
      // Error thrown - should be caught.
      let tc = &mut v8::TryCatch::new(scope);
      let result = eval(tc, "throw new Error('foo')");
      assert!(result.is_none());
      assert!(tc.has_caught());
      assert!(tc.exception().is_some());
      assert!(tc.stack_trace().is_some());
      assert!(tc.message().is_some());
      assert_eq!(
        tc.message().unwrap().get(tc).to_rust_string_lossy(tc),
        "Uncaught Error: foo"
      );
    };
    {
      // No error thrown.
      let tc = &mut v8::TryCatch::new(scope);
      let result = eval(tc, "1 + 1");
      assert!(result.is_some());
      assert!(!tc.has_caught());
      assert!(tc.exception().is_none());
      assert!(tc.stack_trace().is_none());
      assert!(tc.message().is_none());
      assert!(tc.rethrow().is_none());
    };
    {
      // Rethrow and reset.
      let tc1 = &mut v8::TryCatch::new(scope);
      {
        let tc2 = &mut v8::TryCatch::new(tc1);
        eval(tc2, "throw 'bar'");
        assert!(tc2.has_caught());
        assert!(tc2.rethrow().is_some());
        tc2.reset();
        assert!(!tc2.has_caught());
      }
      assert!(tc1.has_caught());
    };
  }
}

#[test]
fn try_catch_caught_lifetime() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);
  let (caught_exc, caught_msg) = {
    let tc = &mut v8::TryCatch::new(scope);
    // Throw exception.
    let msg = v8::String::new(tc, "DANG!").unwrap();
    let exc = v8::Exception::type_error(tc, msg);
    tc.throw_exception(exc);
    // Catch exception.
    let caught_exc = tc.exception().unwrap();
    let caught_msg = tc.message().unwrap();
    // Move `caught_exc` and `caught_msg` out of the extent of the TryCatch,
    // but still within the extent of the enclosing HandleScope.
    (caught_exc, caught_msg)
  };
  // This should not crash.
  assert!(caught_exc
    .to_string(scope)
    .unwrap()
    .to_rust_string_lossy(scope)
    .contains("DANG"));
  assert!(caught_msg
    .get(scope)
    .to_rust_string_lossy(scope)
    .contains("DANG"));
}

#[test]
fn throw_exception() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    {
      let tc = &mut v8::TryCatch::new(scope);
      let exception = v8::String::new(tc, "boom").unwrap();
      tc.throw_exception(exception.into());
      assert!(tc.has_caught());
      assert!(tc
        .exception()
        .unwrap()
        .strict_equals(v8::String::new(tc, "boom").unwrap().into()));
    };
  }
}

#[test]
fn thread_safe_handle_drop_after_isolate() {
  let _setup_guard = setup();
  let mut isolate = v8::Isolate::new(Default::default());
  let handle = isolate.thread_safe_handle();
  // We can call it twice.
  let handle_ = isolate.thread_safe_handle();
  // Check that handle is Send and Sync.
  fn f<S: Send + Sync>(_: S) {}
  f(handle_);
  // All methods on IsolateHandle should return false after the isolate is
  // dropped.
  drop(isolate);
  assert_eq!(false, handle.terminate_execution());
  assert_eq!(false, handle.cancel_terminate_execution());
  assert_eq!(false, handle.is_execution_terminating());
  static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
  extern "C" fn callback(
    _isolate: &mut v8::Isolate,
    data: *mut std::ffi::c_void,
  ) {
    assert_eq!(data, std::ptr::null_mut());
    CALL_COUNT.fetch_add(1, Ordering::SeqCst);
  }
  assert_eq!(
    false,
    handle.request_interrupt(callback, std::ptr::null_mut())
  );
  assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 0);
}

#[test]
fn terminate_execution() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let (tx, rx) = std::sync::mpsc::channel::<bool>();
  let handle = isolate.thread_safe_handle();
  let t = std::thread::spawn(move || {
    // allow deno to boot and run
    std::thread::sleep(std::time::Duration::from_millis(300));
    handle.terminate_execution();
    // allow shutdown
    std::thread::sleep(std::time::Duration::from_millis(200));
    // unless reported otherwise the test should fail after this point
    tx.send(false).ok();
  });

  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);
  // Rn an infinite loop, which should be terminated.
  let source = v8::String::new(scope, "for(;;) {}").unwrap();
  let r = v8::Script::compile(scope, source, None);
  let script = r.unwrap();
  let result = script.run(scope);
  assert!(result.is_none());
  // TODO assert_eq!(e.to_string(), "Uncaught Error: execution terminated")
  let msg = rx.recv().expect("execution should be terminated");
  assert!(!msg);
  // Make sure the isolate unusable again.
  eval(scope, "1+1").expect("execution should be possible again");
  t.join().expect("join t");
}

// TODO(ry) This test should use threads
#[test]
fn request_interrupt_small_scripts() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let handle = isolate.thread_safe_handle();
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn callback(
      _isolate: &mut v8::Isolate,
      data: *mut std::ffi::c_void,
    ) {
      assert_eq!(data, std::ptr::null_mut());
      CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    }
    handle.request_interrupt(callback, std::ptr::null_mut());
    eval(scope, "(function(x){return x;})(1);");
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
  }
}

#[test]
fn add_message_listener() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  isolate.set_capture_stack_trace_for_uncaught_exceptions(true, 32);

  static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

  extern "C" fn check_message_0(
    message: v8::Local<v8::Message>,
    _exception: v8::Local<v8::Value>,
  ) {
    let scope = &mut unsafe { v8::CallbackScope::new(message) };
    let scope = &mut v8::HandleScope::new(scope);
    let message_str = message.get(scope);
    assert_eq!(message_str.to_rust_string_lossy(scope), "Uncaught foo");
    assert_eq!(Some(1), message.get_line_number(scope));
    assert!(message.get_script_resource_name(scope).is_some());
    assert!(message.get_source_line(scope).is_some());
    assert_eq!(message.get_start_position(), 0);
    assert_eq!(message.get_end_position(), 1);
    assert_eq!(message.get_wasm_function_index(), -1);
    assert!(message.error_level() >= 0);
    assert_eq!(message.get_start_column(), 0);
    assert_eq!(message.get_end_column(), 1);
    assert!(!message.is_shared_cross_origin());
    assert!(!message.is_opaque());
    let stack_trace = message.get_stack_trace(scope).unwrap();
    assert_eq!(1, stack_trace.get_frame_count());
    let frame = stack_trace.get_frame(scope, 0).unwrap();
    assert_eq!(1, frame.get_line_number());
    assert_eq!(1, frame.get_column());
    assert_eq!(3, frame.get_script_id());
    assert!(frame.get_script_name(scope).is_none());
    assert!(frame.get_script_name_or_source_url(scope).is_none());
    assert!(frame.get_function_name(scope).is_none());
    assert_eq!(false, frame.is_eval());
    assert_eq!(false, frame.is_constructor());
    assert_eq!(false, frame.is_wasm());
    assert_eq!(true, frame.is_user_javascript());
    CALL_COUNT.fetch_add(1, Ordering::SeqCst);
  }
  isolate.add_message_listener(check_message_0);

  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let source = v8::String::new(scope, "throw 'foo'").unwrap();
    let script = v8::Script::compile(scope, source, None).unwrap();
    assert!(script.run(scope).is_none());
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
  }
}

fn unexpected_module_resolve_callback<'a>(
  _context: v8::Local<'a, v8::Context>,
  _specifier: v8::Local<'a, v8::String>,
  _referrer: v8::Local<'a, v8::Module>,
) -> Option<v8::Local<'a, v8::Module>> {
  unreachable!()
}

#[test]
fn set_host_initialize_import_meta_object_callback() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

  extern "C" fn callback(
    context: v8::Local<v8::Context>,
    _module: v8::Local<v8::Module>,
    meta: v8::Local<v8::Object>,
  ) {
    CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    let scope = &mut unsafe { v8::CallbackScope::new(context) };
    let scope = &mut v8::HandleScope::new(scope);
    let key = v8::String::new(scope, "foo").unwrap();
    let value = v8::String::new(scope, "bar").unwrap();
    meta.create_data_property(scope, key.into(), value.into());
  }
  isolate.set_host_initialize_import_meta_object_callback(callback);

  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let source = mock_source(scope, "google.com", "import.meta;");
    let module = v8::script_compiler::compile_module(scope, source).unwrap();
    let result =
      module.instantiate_module(scope, unexpected_module_resolve_callback);
    assert!(result.is_some());
    let meta = module.evaluate(scope).unwrap();
    assert!(meta.is_object());
    let meta = meta.to_object(scope).unwrap();
    let key = v8::String::new(scope, "foo").unwrap();
    let expected = v8::String::new(scope, "bar").unwrap();
    let actual = meta.get(scope, key.into()).unwrap();
    assert!(expected.strict_equals(actual));
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
  }
}

#[test]
fn script_compile_and_run() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let source = v8::String::new(scope, "'Hello ' + 13 + 'th planet'").unwrap();
    let script = v8::Script::compile(scope, source, None).unwrap();
    source.to_rust_string_lossy(scope);
    let result = script.run(scope).unwrap();
    let result = result.to_string(scope).unwrap();
    assert_eq!(result.to_rust_string_lossy(scope), "Hello 13th planet");
  }
}

#[test]
fn script_origin() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let resource_name = v8::String::new(scope, "foo.js").unwrap();
    let resource_line_offset = v8::Integer::new(scope, 4);
    let resource_column_offset = v8::Integer::new(scope, 5);
    let resource_is_shared_cross_origin = v8::Boolean::new(scope, true);
    let script_id = v8::Integer::new(scope, 123);
    let source_map_url = v8::String::new(scope, "source_map_url").unwrap();
    let resource_is_opaque = v8::Boolean::new(scope, true);
    let is_wasm = v8::Boolean::new(scope, false);
    let is_module = v8::Boolean::new(scope, false);

    let script_origin = v8::ScriptOrigin::new(
      resource_name.into(),
      resource_line_offset,
      resource_column_offset,
      resource_is_shared_cross_origin,
      script_id,
      source_map_url.into(),
      resource_is_opaque,
      is_wasm,
      is_module,
    );

    let source = v8::String::new(scope, "1+2").unwrap();
    let script =
      v8::Script::compile(scope, source, Some(&script_origin)).unwrap();
    source.to_rust_string_lossy(scope);
    let _result = script.run(scope).unwrap();
  }
}

#[test]
fn get_version() {
  assert!(v8::V8::get_version().len() > 3);
}

#[test]
fn set_flags_from_command_line() {
  let r = v8::V8::set_flags_from_command_line(vec![
    "binaryname".to_string(),
    "--log-colour".to_string(),
    "--should-be-ignored".to_string(),
  ]);
  assert_eq!(
    r,
    vec!["binaryname".to_string(), "--should-be-ignored".to_string()]
  );
}

#[test]
fn inspector_string_view() {
  let chars = b"Hello world!";
  let view = v8::inspector::StringView::from(&chars[..]);

  assert_eq!(chars.len(), view.into_iter().len());
  assert_eq!(chars.len(), view.len());
  for (c1, c2) in chars.iter().copied().map(u16::from).zip(view) {
    assert_eq!(c1, c2);
  }
}

#[test]
fn inspector_string_buffer() {
  let chars = b"Hello Venus!";
  let mut buf = {
    let src_view = v8::inspector::StringView::from(&chars[..]);
    v8::inspector::StringBuffer::create(src_view)
  };
  let view = buf.as_mut().unwrap().string();

  assert_eq!(chars.len(), view.into_iter().len());
  assert_eq!(chars.len(), view.len());
  for (c1, c2) in chars.iter().copied().map(u16::from).zip(view) {
    assert_eq!(c1, c2);
  }
}

#[test]
fn test_primitives() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let null = v8::null(scope);
    assert!(!null.is_undefined());
    assert!(null.is_null());
    assert!(null.is_null_or_undefined());

    let undefined = v8::undefined(scope);
    assert!(undefined.is_undefined());
    assert!(!undefined.is_null());
    assert!(undefined.is_null_or_undefined());

    let true_ = v8::Boolean::new(scope, true);
    assert!(true_.is_true());
    assert!(!true_.is_undefined());
    assert!(!true_.is_null());
    assert!(!true_.is_null_or_undefined());

    let false_ = v8::Boolean::new(scope, false);
    assert!(false_.is_false());
    assert!(!false_.is_undefined());
    assert!(!false_.is_null());
    assert!(!false_.is_null_or_undefined());
  }
}

#[test]
fn exception() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  let msg_in = v8::String::new(scope, "This is a test error").unwrap();
  let _exception = v8::Exception::error(scope, msg_in);
  let _exception = v8::Exception::range_error(scope, msg_in);
  let _exception = v8::Exception::reference_error(scope, msg_in);
  let _exception = v8::Exception::syntax_error(scope, msg_in);
  let exception = v8::Exception::type_error(scope, msg_in);

  let actual_msg_out =
    v8::Exception::create_message(scope, exception).get(scope);
  let expected_msg_out =
    v8::String::new(scope, "Uncaught TypeError: This is a test error").unwrap();
  assert!(actual_msg_out.strict_equals(expected_msg_out.into()));
  assert!(v8::Exception::get_stack_trace(scope, exception).is_none());
}

#[test]
fn create_message_argument_lifetimes() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  {
    let create_message = v8::Function::new(
      scope,
      |scope: &mut v8::HandleScope,
       args: v8::FunctionCallbackArguments,
       mut rv: v8::ReturnValue| {
        let message = v8::Exception::create_message(scope, args.get(0));
        let message_str = message.get(scope);
        rv.set(message_str.into())
      },
    )
    .unwrap();
    let receiver = context.global(scope);
    let message_str = v8::String::new(scope, "mishap").unwrap();
    let exception = v8::Exception::type_error(scope, message_str);
    let actual = create_message
      .call(scope, receiver.into(), &[exception])
      .unwrap();
    let expected =
      v8::String::new(scope, "Uncaught TypeError: mishap").unwrap();
    assert!(actual.strict_equals(expected.into()));
  }
}

#[test]
fn json() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let json_string = v8::String::new(scope, "{\"a\": 1, \"b\": 2}").unwrap();
    let maybe_value = v8::json::parse(scope, json_string);
    assert!(maybe_value.is_some());
    let value = maybe_value.unwrap();
    let maybe_stringified = v8::json::stringify(scope, value);
    assert!(maybe_stringified.is_some());
    let stringified = maybe_stringified.unwrap();
    let rust_str = stringified.to_rust_string_lossy(scope);
    assert_eq!("{\"a\":1,\"b\":2}".to_string(), rust_str);
  }
}

#[test]
fn object_template() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let object_templ = v8::ObjectTemplate::new(scope);
    let function_templ = v8::FunctionTemplate::new(scope, fortytwo_callback);
    let name = v8::String::new(scope, "f").unwrap();
    let attr = v8::READ_ONLY + v8::DONT_ENUM + v8::DONT_DELETE;
    object_templ.set_with_attr(name.into(), function_templ.into(), attr);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let object = object_templ.new_instance(scope).unwrap();
    assert!(!object.is_null_or_undefined());
    let name = v8::String::new(scope, "g").unwrap();
    context.global(scope).define_own_property(
      scope,
      name.into(),
      object.into(),
      v8::DONT_ENUM,
    );
    let source = r#"
      {
        const d = Object.getOwnPropertyDescriptor(globalThis, "g");
        [d.configurable, d.enumerable, d.writable].toString()
      }
    "#;
    let actual = eval(scope, source).unwrap();
    let expected = v8::String::new(scope, "true,false,true").unwrap();
    assert!(expected.strict_equals(actual));
    let actual = eval(scope, "g.f()").unwrap();
    let expected = v8::Integer::new(scope, 42);
    assert!(expected.strict_equals(actual));
    let source = r#"
      {
        const d = Object.getOwnPropertyDescriptor(g, "f");
        [d.configurable, d.enumerable, d.writable].toString()
      }
    "#;
    let actual = eval(scope, source).unwrap();
    let expected = v8::String::new(scope, "false,false,false").unwrap();
    assert!(expected.strict_equals(actual));
  }
}

#[test]
fn object_template_from_function_template() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let function_templ = v8::FunctionTemplate::new(scope, fortytwo_callback);
    let expected_class_name = v8::String::new(scope, "fortytwo").unwrap();
    function_templ.set_class_name(expected_class_name);
    let object_templ =
      v8::ObjectTemplate::new_from_template(scope, function_templ);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let object = object_templ.new_instance(scope).unwrap();
    assert!(!object.is_null_or_undefined());
    let name = v8::String::new(scope, "g").unwrap();
    context.global(scope).set(scope, name.into(), object.into());
    let actual_class_name = eval(scope, "g.constructor.name").unwrap();
    assert!(expected_class_name.strict_equals(actual_class_name));
  }
}

#[test]
fn object() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let null: v8::Local<v8::Value> = v8::null(scope).into();
    let n1: v8::Local<v8::Name> = v8::String::new(scope, "a").unwrap().into();
    let n2: v8::Local<v8::Name> = v8::String::new(scope, "b").unwrap().into();
    let v1: v8::Local<v8::Value> = v8::Number::new(scope, 1.0).into();
    let v2: v8::Local<v8::Value> = v8::Number::new(scope, 2.0).into();
    let object = v8::Object::with_prototype_and_properties(
      scope,
      null,
      &[n1, n2],
      &[v1, v2],
    );
    assert!(!object.is_null_or_undefined());
    let lhs = object.creation_context(scope).global(scope);
    let rhs = context.global(scope);
    assert!(lhs.strict_equals(rhs.into()));

    let object_ = v8::Object::new(scope);
    assert!(!object_.is_null_or_undefined());
    let id = object_.get_identity_hash();
    assert_ne!(id, 0);
  }
}

#[test]
fn array() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let s1 = v8::String::new(scope, "a").unwrap();
    let s2 = v8::String::new(scope, "b").unwrap();
    let array = v8::Array::new(scope, 2);
    assert_eq!(array.length(), 2);
    let lhs = array.creation_context(scope).global(scope);
    let rhs = context.global(scope);
    assert!(lhs.strict_equals(rhs.into()));
    array.set_index(scope, 0, s1.into());
    array.set_index(scope, 1, s2.into());

    let maybe_v1 = array.get_index(scope, 0);
    assert!(maybe_v1.is_some());
    assert!(maybe_v1.unwrap().same_value(s1.into()));
    let maybe_v2 = array.get_index(scope, 1);
    assert!(maybe_v2.is_some());
    assert!(maybe_v2.unwrap().same_value(s2.into()));

    let array = v8::Array::new_with_elements(scope, &[]);
    assert_eq!(array.length(), 0);

    let array = v8::Array::new_with_elements(scope, &[s1.into(), s2.into()]);
    assert_eq!(array.length(), 2);

    let maybe_v1 = array.get_index(scope, 0);
    assert!(maybe_v1.is_some());
    assert!(maybe_v1.unwrap().same_value(s1.into()));
    let maybe_v2 = array.get_index(scope, 1);
    assert!(maybe_v2.is_some());
    assert!(maybe_v2.unwrap().same_value(s2.into()));
  }
}

#[test]
fn create_data_property() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    eval(scope, "var a = {};");

    let key = v8::String::new(scope, "a").unwrap();
    let obj = context.global(scope).get(scope, key.into()).unwrap();
    assert!(obj.is_object());
    let obj = obj.to_object(scope).unwrap();
    let key = v8::String::new(scope, "foo").unwrap();
    let value = v8::String::new(scope, "bar").unwrap();
    assert!(obj
      .create_data_property(scope, key.into(), value.into())
      .unwrap());
    let actual = obj.get(scope, key.into()).unwrap();
    assert!(value.strict_equals(actual));

    let key2 = v8::String::new(scope, "foo2").unwrap();
    assert!(obj.set(scope, key2.into(), value.into()).unwrap());
    let actual = obj.get(scope, key2.into()).unwrap();
    assert!(value.strict_equals(actual));
  }
}

#[test]
fn object_set_accessor() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  {
    static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

    let getter = |scope: &mut v8::HandleScope,
                  key: v8::Local<v8::Name>,
                  args: v8::PropertyCallbackArguments,
                  mut rv: v8::ReturnValue| {
      let this = args.this();

      let expected_key = v8::String::new(scope, "getter_key").unwrap();
      assert!(key.strict_equals(expected_key.into()));

      let int_key = v8::String::new(scope, "int_key").unwrap();
      let int_value = this.get(scope, int_key.into()).unwrap();
      let int_value = v8::Local::<v8::Integer>::try_from(int_value).unwrap();
      assert_eq!(int_value.value(), 42);

      let s = v8::String::new(scope, "hello").unwrap();
      assert!(rv.get(scope).is_undefined());
      rv.set(s.into());

      CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    };

    let obj = v8::Object::new(scope);

    let getter_key = v8::String::new(scope, "getter_key").unwrap();
    obj.set_accessor(scope, getter_key.into(), getter);

    let int_key = v8::String::new(scope, "int_key").unwrap();
    let int_value = v8::Integer::new(scope, 42);
    obj.set(scope, int_key.into(), int_value.into());

    let obj_name = v8::String::new(scope, "obj").unwrap();
    context
      .global(scope)
      .set(scope, obj_name.into(), obj.into());

    let actual = eval(scope, "obj.getter_key").unwrap();
    let expected = v8::String::new(scope, "hello").unwrap();
    assert!(actual.strict_equals(expected.into()));

    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
  }
}

#[test]
fn promise_resolved() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let maybe_resolver = v8::PromiseResolver::new(scope);
    assert!(maybe_resolver.is_some());
    let resolver = maybe_resolver.unwrap();
    let promise = resolver.get_promise(scope);
    assert!(!promise.has_handler());
    assert_eq!(promise.state(), v8::PromiseState::Pending);
    let value = v8::String::new(scope, "test").unwrap();
    resolver.resolve(scope, value.into());
    assert_eq!(promise.state(), v8::PromiseState::Fulfilled);
    let result = promise.result(scope);
    let result_str = result.to_string(scope).unwrap();
    assert_eq!(result_str.to_rust_string_lossy(scope), "test".to_string());
    // Resolve again with different value, since promise is already in
    // `Fulfilled` state it should be ignored.
    let value = v8::String::new(scope, "test2").unwrap();
    resolver.resolve(scope, value.into());
    let result = promise.result(scope);
    let result_str = result.to_string(scope).unwrap();
    assert_eq!(result_str.to_rust_string_lossy(scope), "test".to_string());
  }
}

#[test]
fn promise_rejected() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let maybe_resolver = v8::PromiseResolver::new(scope);
    assert!(maybe_resolver.is_some());
    let resolver = maybe_resolver.unwrap();
    let promise = resolver.get_promise(scope);
    assert!(!promise.has_handler());
    assert_eq!(promise.state(), v8::PromiseState::Pending);
    let value = v8::String::new(scope, "test").unwrap();
    let rejected = resolver.reject(scope, value.into());
    assert!(rejected.unwrap());
    assert_eq!(promise.state(), v8::PromiseState::Rejected);
    let result = promise.result(scope);
    let result_str = result.to_string(scope).unwrap();
    assert_eq!(result_str.to_rust_string_lossy(scope), "test".to_string());
    // Reject again with different value, since promise is already in `Rejected`
    // state it should be ignored.
    let value = v8::String::new(scope, "test2").unwrap();
    resolver.reject(scope, value.into());
    let result = promise.result(scope);
    let result_str = result.to_string(scope).unwrap();
    assert_eq!(result_str.to_rust_string_lossy(scope), "test".to_string());
  }
}
#[test]
fn proxy() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let target = v8::Object::new(scope);
    let handler = v8::Object::new(scope);
    let maybe_proxy = v8::Proxy::new(scope, target, handler);
    assert!(maybe_proxy.is_some());
    let proxy = maybe_proxy.unwrap();
    assert!(target == proxy.get_target(scope));
    assert!(handler == proxy.get_handler(scope));
    assert!(!proxy.is_revoked());
    proxy.revoke();
    assert!(proxy.is_revoked());
  }
}

fn fn_callback(
  scope: &mut v8::HandleScope,
  args: v8::FunctionCallbackArguments,
  mut rv: v8::ReturnValue,
) {
  assert_eq!(args.length(), 0);
  let s = v8::String::new(scope, "Hello callback!").unwrap();
  assert!(rv.get(scope).is_undefined());
  rv.set(s.into());
}

fn fn_callback2(
  scope: &mut v8::HandleScope,
  args: v8::FunctionCallbackArguments,
  mut rv: v8::ReturnValue,
) {
  assert_eq!(args.length(), 2);
  let arg1_val = v8::String::new(scope, "arg1").unwrap();
  let arg1 = args.get(0);
  assert!(arg1.is_string());
  assert!(arg1.strict_equals(arg1_val.into()));

  let arg2_val = v8::Integer::new(scope, 2);
  let arg2 = args.get(1);
  assert!(arg2.is_number());
  assert!(arg2.strict_equals(arg2_val.into()));

  let s = v8::String::new(scope, "Hello callback!").unwrap();
  assert!(rv.get(scope).is_undefined());
  rv.set(s.into());
}

fn fortytwo_callback(
  scope: &mut v8::HandleScope,
  _: v8::FunctionCallbackArguments,
  mut rv: v8::ReturnValue,
) {
  rv.set(v8::Integer::new(scope, 42).into());
}

fn data_is_true_callback(
  _scope: &mut v8::HandleScope,
  args: v8::FunctionCallbackArguments,
  _rv: v8::ReturnValue,
) {
  let data = args.data();
  assert!(data.is_some());
  let data = data.unwrap();
  assert!(data.is_true());
}

#[test]
fn function() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let global = context.global(scope);
    let recv: v8::Local<v8::Value> = global.into();
    // create function using template
    let fn_template = v8::FunctionTemplate::new(scope, fn_callback);
    let function = fn_template
      .get_function(scope)
      .expect("Unable to create function");
    let lhs = function.creation_context(scope).global(scope);
    let rhs = context.global(scope);
    assert!(lhs.strict_equals(rhs.into()));
    function
      .call(scope, recv, &[])
      .expect("Function call failed");
    // create function without a template
    let function = v8::Function::new(scope, fn_callback2)
      .expect("Unable to create function");
    let arg1 = v8::String::new(scope, "arg1").unwrap();
    let arg2 = v8::Integer::new(scope, 2);
    let value = function
      .call(scope, recv, &[arg1.into(), arg2.into()])
      .unwrap();
    let value_str = value.to_string(scope).unwrap();
    let rust_str = value_str.to_rust_string_lossy(scope);
    assert_eq!(rust_str, "Hello callback!".to_string());
    // create a function with associated data
    let true_data = v8::Boolean::new(scope, true);
    let function = v8::Function::new_with_data(
      scope,
      true_data.into(),
      data_is_true_callback,
    )
    .expect("Unable to create function with data");
    function
      .call(scope, recv, &[])
      .expect("Function call failed");
  }
}

extern "C" fn promise_reject_callback(msg: v8::PromiseRejectMessage) {
  let scope = &mut unsafe { v8::CallbackScope::new(&msg) };
  let event = msg.get_event();
  assert_eq!(event, v8::PromiseRejectEvent::PromiseRejectWithNoHandler);
  let promise = msg.get_promise();
  assert_eq!(promise.state(), v8::PromiseState::Rejected);
  let value = msg.get_value();
  {
    let scope = &mut v8::HandleScope::new(scope);
    let value_str = value.to_string(scope).unwrap();
    let rust_str = value_str.to_rust_string_lossy(scope);
    assert_eq!(rust_str, "promise rejected".to_string());
  }
}

#[test]
fn set_promise_reject_callback() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  isolate.set_promise_reject_callback(promise_reject_callback);
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let resolver = v8::PromiseResolver::new(scope).unwrap();
    let value = v8::String::new(scope, "promise rejected").unwrap();
    resolver.reject(scope, value.into());
  }
}

fn mock_script_origin<'s>(
  scope: &mut v8::HandleScope<'s>,
  resource_name_: &str,
) -> v8::ScriptOrigin<'s> {
  let resource_name = v8::String::new(scope, resource_name_).unwrap();
  let resource_line_offset = v8::Integer::new(scope, 0);
  let resource_column_offset = v8::Integer::new(scope, 0);
  let resource_is_shared_cross_origin = v8::Boolean::new(scope, true);
  let script_id = v8::Integer::new(scope, 123);
  let source_map_url = v8::String::new(scope, "source_map_url").unwrap();
  let resource_is_opaque = v8::Boolean::new(scope, true);
  let is_wasm = v8::Boolean::new(scope, false);
  let is_module = v8::Boolean::new(scope, true);
  v8::ScriptOrigin::new(
    resource_name.into(),
    resource_line_offset,
    resource_column_offset,
    resource_is_shared_cross_origin,
    script_id,
    source_map_url.into(),
    resource_is_opaque,
    is_wasm,
    is_module,
  )
}

fn mock_source<'s>(
  scope: &mut v8::HandleScope<'s>,
  resource_name: &str,
  source: &str,
) -> v8::script_compiler::Source {
  let source_str = v8::String::new(scope, source).unwrap();
  let script_origin = mock_script_origin(scope, resource_name);
  v8::script_compiler::Source::new(source_str, &script_origin)
}

#[test]
fn script_compiler_source() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  isolate.set_promise_reject_callback(promise_reject_callback);
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let source = "1+2";
    let script_origin = mock_script_origin(scope, "foo.js");
    let source = v8::script_compiler::Source::new(
      v8::String::new(scope, source).unwrap(),
      &script_origin,
    );

    let result = v8::script_compiler::compile_module(scope, source);
    assert!(result.is_some());
  }
}

#[test]
fn module_instantiation_failures1() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let source_text = v8::String::new(
      scope,
      "import './foo.js';\n\
       export {} from './bar.js';",
    )
    .unwrap();
    let origin = mock_script_origin(scope, "foo.js");
    let source = v8::script_compiler::Source::new(source_text, &origin);

    let module = v8::script_compiler::compile_module(scope, source).unwrap();
    assert_eq!(v8::ModuleStatus::Uninstantiated, module.get_status());
    assert_eq!(2, module.get_module_requests_length());

    assert_eq!(
      "./foo.js",
      module.get_module_request(0).to_rust_string_lossy(scope)
    );
    let loc = module.get_module_request_location(0);
    assert_eq!(0, loc.get_line_number());
    assert_eq!(7, loc.get_column_number());

    assert_eq!(
      "./bar.js",
      module.get_module_request(1).to_rust_string_lossy(scope)
    );
    let loc = module.get_module_request_location(1);
    assert_eq!(1, loc.get_line_number());
    assert_eq!(15, loc.get_column_number());

    // Instantiation should fail.
    {
      let tc = &mut v8::TryCatch::new(scope);
      fn resolve_callback<'a>(
        context: v8::Local<'a, v8::Context>,
        _specifier: v8::Local<'a, v8::String>,
        _referrer: v8::Local<'a, v8::Module>,
      ) -> Option<v8::Local<'a, v8::Module>> {
        let scope = &mut unsafe { v8::CallbackScope::new(context) };
        let scope = &mut v8::HandleScope::new(scope);
        let e = v8::String::new(scope, "boom").unwrap();
        scope.throw_exception(e.into());
        None
      }
      let result = module.instantiate_module(tc, resolve_callback);
      assert!(result.is_none());
      assert!(tc.has_caught());
      assert!(tc
        .exception()
        .unwrap()
        .strict_equals(v8::String::new(tc, "boom").unwrap().into()));
      assert_eq!(v8::ModuleStatus::Uninstantiated, module.get_status());
    }
  }
}

fn compile_specifier_as_module_resolve_callback<'a>(
  context: v8::Local<'a, v8::Context>,
  specifier: v8::Local<'a, v8::String>,
  _referrer: v8::Local<'a, v8::Module>,
) -> Option<v8::Local<'a, v8::Module>> {
  let scope = &mut unsafe { v8::CallbackScope::new(context) };
  let origin = mock_script_origin(scope, "module.js");
  let source = v8::script_compiler::Source::new(specifier, &origin);
  let module = v8::script_compiler::compile_module(scope, source).unwrap();
  Some(module)
}

#[test]
fn module_evaluation() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let source_text = v8::String::new(
      scope,
      "import 'Object.expando = 5';\n\
       import 'Object.expando *= 2';",
    )
    .unwrap();
    let origin = mock_script_origin(scope, "foo.js");
    let source = v8::script_compiler::Source::new(source_text, &origin);

    let module = v8::script_compiler::compile_module(scope, source).unwrap();
    assert_eq!(v8::ModuleStatus::Uninstantiated, module.get_status());

    let result = module
      .instantiate_module(scope, compile_specifier_as_module_resolve_callback);
    assert!(result.unwrap());
    assert_eq!(v8::ModuleStatus::Instantiated, module.get_status());

    let result = module.evaluate(scope);
    assert!(result.is_some());
    assert_eq!(v8::ModuleStatus::Evaluated, module.get_status());

    let result = eval(scope, "Object.expando").unwrap();
    assert!(result.is_number());
    let expected = v8::Number::new(scope, 10.);
    assert!(result.strict_equals(expected.into()));
  }
}

#[test]
fn primitive_array() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let length = 3;
    let array = v8::PrimitiveArray::new(scope, length);
    assert_eq!(length, array.length());

    for i in 0..length {
      let item = array.get(scope, i);
      assert!(item.is_undefined());
    }

    let string = v8::String::new(scope, "test").unwrap();
    array.set(scope, 1, string.into());
    assert!(array.get(scope, 0).is_undefined());
    assert!(array.get(scope, 1).is_string());

    let num = v8::Number::new(scope, 0.42);
    array.set(scope, 2, num.into());
    assert!(array.get(scope, 0).is_undefined());
    assert!(array.get(scope, 1).is_string());
    assert!(array.get(scope, 2).is_number());
  }
}

#[test]
fn equality() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    assert!(v8::String::new(scope, "a")
      .unwrap()
      .strict_equals(v8::String::new(scope, "a").unwrap().into()));
    assert!(!v8::String::new(scope, "a")
      .unwrap()
      .strict_equals(v8::String::new(scope, "b").unwrap().into()));

    assert!(v8::String::new(scope, "a")
      .unwrap()
      .same_value(v8::String::new(scope, "a").unwrap().into()));
    assert!(!v8::String::new(scope, "a")
      .unwrap()
      .same_value(v8::String::new(scope, "b").unwrap().into()));
  }
}

#[test]
fn array_buffer_view() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let source =
      v8::String::new(scope, "new Uint8Array([23,23,23,23])").unwrap();
    let script = v8::Script::compile(scope, source, None).unwrap();
    source.to_rust_string_lossy(scope);
    let result: v8::Local<v8::ArrayBufferView> =
      script.run(scope).unwrap().try_into().unwrap();
    assert_eq!(result.byte_length(), 4);
    assert_eq!(result.byte_offset(), 0);
    let mut dest = [0; 4];
    let copy_bytes = result.copy_contents(&mut dest);
    assert_eq!(copy_bytes, 4);
    assert_eq!(dest, [23, 23, 23, 23]);
    let maybe_ab = result.buffer(scope);
    assert!(maybe_ab.is_some());
    let ab = maybe_ab.unwrap();
    assert_eq!(ab.byte_length(), 4);
  }
}

#[test]
fn snapshot_creator() {
  let _setup_guard = setup();
  // First we create the snapshot, there is a single global variable 'a' set to
  // the value 3.
  let startup_data = {
    let mut snapshot_creator = v8::SnapshotCreator::new(None);
    // TODO(ry) this shouldn't be necessary. workaround unfinished business in
    // the scope type system.
    let mut isolate = unsafe { snapshot_creator.get_owned_isolate() };
    {
      // Check that the SnapshotCreator isolate has been set up correctly.
      let _ = isolate.thread_safe_handle();

      let scope = &mut v8::HandleScope::new(&mut isolate);
      let context = v8::Context::new(scope);
      let scope = &mut v8::ContextScope::new(scope, context);

      let source = v8::String::new(scope, "a = 1 + 2").unwrap();
      let script = v8::Script::compile(scope, source, None).unwrap();
      script.run(scope).unwrap();

      snapshot_creator.set_default_context(context);
    }
    std::mem::forget(isolate); // TODO(ry) this shouldn't be necessary.
    snapshot_creator
      .create_blob(v8::FunctionCodeHandling::Clear)
      .unwrap()
  };
  assert!(startup_data.len() > 0);
  // Now we try to load up the snapshot and check that 'a' has the correct
  // value.
  {
    let params = v8::Isolate::create_params().snapshot_blob(startup_data);
    let isolate = &mut v8::Isolate::new(params);
    {
      let scope = &mut v8::HandleScope::new(isolate);
      let context = v8::Context::new(scope);
      let scope = &mut v8::ContextScope::new(scope, context);
      let source = v8::String::new(scope, "a === 3").unwrap();
      let script = v8::Script::compile(scope, source, None).unwrap();
      let result = script.run(scope).unwrap();
      let true_val = v8::Boolean::new(scope, true).into();
      assert!(result.same_value(true_val));
    }
  }
}

lazy_static! {
  static ref EXTERNAL_REFERENCES: v8::ExternalReferences =
    v8::ExternalReferences::new(&[v8::ExternalReference {
      function: fn_callback.map_fn_to()
    }]);
}

#[test]
fn external_references() {
  let _setup_guard = setup();
  // First we create the snapshot, there is a single global variable 'a' set to
  // the value 3.
  let startup_data = {
    let mut snapshot_creator =
      v8::SnapshotCreator::new(Some(&EXTERNAL_REFERENCES));
    // TODO(ry) this shouldn't be necessary. workaround unfinished business in
    // the scope type system.
    let mut isolate = unsafe { snapshot_creator.get_owned_isolate() };
    {
      let scope = &mut v8::HandleScope::new(&mut isolate);
      let context = v8::Context::new(scope);
      let scope = &mut v8::ContextScope::new(scope, context);

      // create function using template
      let fn_template = v8::FunctionTemplate::new(scope, fn_callback);
      let function = fn_template
        .get_function(scope)
        .expect("Unable to create function");

      let global = context.global(scope);
      let key = v8::String::new(scope, "F").unwrap();
      global.set(scope, key.into(), function.into());

      snapshot_creator.set_default_context(context);
    }
    std::mem::forget(isolate); // TODO(ry) this shouldn't be necessary.
    snapshot_creator
      .create_blob(v8::FunctionCodeHandling::Clear)
      .unwrap()
  };
  assert!(startup_data.len() > 0);
  // Now we try to load up the snapshot and check that 'a' has the correct
  // value.
  {
    let params = v8::Isolate::create_params()
      .snapshot_blob(startup_data)
      .external_references(&**EXTERNAL_REFERENCES);
    let isolate = &mut v8::Isolate::new(params);
    {
      let scope = &mut v8::HandleScope::new(isolate);
      let context = v8::Context::new(scope);
      let scope = &mut v8::ContextScope::new(scope, context);

      let result = eval(scope, "if(F() != 'wrong answer') throw 'boom1'");
      assert!(result.is_none());

      let result = eval(scope, "if(F() != 'Hello callback!') throw 'boom2'");
      assert!(result.is_some());
    }
  }
}

#[test]
fn create_params_snapshot_blob() {
  let static_data = b"abcd";
  let _ = v8::CreateParams::default().snapshot_blob(&static_data[..]);

  let vec_1 = Vec::from(&b"defg"[..]);
  let _ = v8::CreateParams::default().snapshot_blob(vec_1);

  let vec_2 = std::fs::read(file!()).unwrap();
  let _ = v8::CreateParams::default().snapshot_blob(vec_2);

  let arc_slice: std::sync::Arc<[u8]> = std::fs::read(file!()).unwrap().into();
  let _ = v8::CreateParams::default().snapshot_blob(arc_slice.clone());
  let _ = v8::CreateParams::default().snapshot_blob(arc_slice);
}

#[test]
fn uint8_array() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let source =
      v8::String::new(scope, "new Uint8Array([23,23,23,23])").unwrap();
    let script = v8::Script::compile(scope, source, None).unwrap();
    source.to_rust_string_lossy(scope);
    let result: v8::Local<v8::ArrayBufferView> =
      script.run(scope).unwrap().try_into().unwrap();
    assert_eq!(result.byte_length(), 4);
    assert_eq!(result.byte_offset(), 0);
    let mut dest = [0; 4];
    let copy_bytes = result.copy_contents(&mut dest);
    assert_eq!(copy_bytes, 4);
    assert_eq!(dest, [23, 23, 23, 23]);
    let maybe_ab = result.buffer(scope);
    assert!(maybe_ab.is_some());
    let ab = maybe_ab.unwrap();
    let uint8_array = v8::Uint8Array::new(scope, ab, 0, 0);
    assert!(uint8_array.is_some());
  }
}

#[test]
fn dynamic_import() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

  extern "C" fn dynamic_import_cb(
    context: v8::Local<v8::Context>,
    _referrer: v8::Local<v8::ScriptOrModule>,
    specifier: v8::Local<v8::String>,
  ) -> *mut v8::Promise {
    let scope = &mut unsafe { v8::CallbackScope::new(context) };
    let scope = &mut v8::HandleScope::new(scope);
    assert!(
      specifier.strict_equals(v8::String::new(scope, "bar.js").unwrap().into())
    );
    let e = v8::String::new(scope, "boom").unwrap();
    scope.throw_exception(e.into());
    CALL_COUNT.fetch_add(1, Ordering::SeqCst);
    std::ptr::null_mut()
  }
  isolate.set_host_import_module_dynamically_callback(dynamic_import_cb);

  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let result = eval(
      scope,
      "(async function () {\n\
         let x = await import('bar.js');\n\
       })();",
    );
    assert!(result.is_some());
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 1);
  }
}

#[test]
fn shared_array_buffer() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let sab = v8::SharedArrayBuffer::new(scope, 16).unwrap();
    let shared_bs_1 = sab.get_backing_store();
    shared_bs_1[5].set(12);
    shared_bs_1[12].set(52);

    let global = context.global(scope);
    let key = v8::String::new(scope, "shared").unwrap();
    let r = global
      .create_data_property(scope, key.into(), sab.into())
      .unwrap();
    assert!(r);
    let source = v8::String::new(
      scope,
      r"sharedBytes = new Uint8Array(shared);
        sharedBytes[2] = 16;
        sharedBytes[14] = 62;
        sharedBytes[5] + sharedBytes[12]",
    )
    .unwrap();
    let script = v8::Script::compile(scope, source, None).unwrap();

    let result: v8::Local<v8::Integer> =
      script.run(scope).unwrap().try_into().unwrap();
    assert_eq!(result.value(), 64);
    assert_eq!(shared_bs_1[2].get(), 16);
    assert_eq!(shared_bs_1[14].get(), 62);

    let data: Box<[u8]> = vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9].into_boxed_slice();
    let bs = v8::SharedArrayBuffer::new_backing_store_from_boxed_slice(data);
    assert_eq!(bs.byte_length(), 10);
    assert_eq!(bs.is_shared(), true);

    let shared_bs_2 = bs.make_shared();
    assert_eq!(shared_bs_2.byte_length(), 10);
    assert_eq!(shared_bs_2.is_shared(), true);

    let ab = v8::SharedArrayBuffer::with_backing_store(scope, &shared_bs_2);
    let shared_bs_3 = ab.get_backing_store();
    assert_eq!(shared_bs_3.byte_length(), 10);
    assert_eq!(shared_bs_3[0].get(), 0);
    assert_eq!(shared_bs_3[9].get(), 9);
  }
}

#[test]
#[allow(clippy::cognitive_complexity)]
#[allow(clippy::eq_op)]
fn value_checker() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let value = eval(scope, "undefined").unwrap();
    assert!(value.is_undefined());
    assert!(value.is_null_or_undefined());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Primitive>::try_from(value).unwrap());
    assert!(value == v8::undefined(scope));
    assert!(value != v8::null(scope));
    assert!(value != v8::Boolean::new(scope, false));
    assert!(value != v8::Integer::new(scope, 0));

    let value = eval(scope, "null").unwrap();
    assert!(value.is_null());
    assert!(value.is_null_or_undefined());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Primitive>::try_from(value).unwrap());
    assert!(value == v8::null(scope));
    assert!(value == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::null(scope));
    assert!(value != v8::undefined(scope));
    assert!(value != v8::Boolean::new(scope, false));
    assert!(value != v8::Integer::new(scope, 0));

    let value = eval(scope, "true").unwrap();
    assert!(value.is_boolean());
    assert!(value.is_true());
    assert!(!value.is_false());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Boolean>::try_from(value).unwrap());
    assert!(value == v8::Boolean::new(scope, true));
    assert!(value == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == eval(scope, "!false").unwrap());
    assert!(v8::Global::new(scope, value) != eval(scope, "1").unwrap());
    assert!(value != v8::Boolean::new(scope, false));

    let value = eval(scope, "false").unwrap();
    assert!(value.is_boolean());
    assert!(!value.is_true());
    assert!(value.is_false());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Boolean>::try_from(value).unwrap());
    assert!(value == v8::Boolean::new(scope, false));
    assert!(value == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == eval(scope, "!true").unwrap());
    assert!(v8::Global::new(scope, value) != eval(scope, "0").unwrap());
    assert!(value != v8::Boolean::new(scope, true));
    assert!(value != v8::null(scope));
    assert!(value != v8::undefined(scope));
    assert!(value != v8::Integer::new(scope, 0));

    let value = eval(scope, "'name'").unwrap();
    assert!(value.is_name());
    assert!(value.is_string());
    assert!(value == value);
    assert!(value == v8::Local::<v8::String>::try_from(value).unwrap());
    assert!(value == v8::String::new(scope, "name").unwrap());
    assert!(value != v8::String::new(scope, "name\0").unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "Symbol()").unwrap();
    assert!(value.is_name());
    assert!(value.is_symbol());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Symbol>::try_from(value).unwrap());
    assert!(value == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(value != eval(scope, "Symbol()").unwrap());
    assert!(v8::Global::new(scope, value) != eval(scope, "Symbol()").unwrap());

    let value = eval(scope, "() => 0").unwrap();
    assert!(value.is_function());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Function>::try_from(value).unwrap());
    assert!(value == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(value != eval(scope, "() => 0").unwrap());
    assert!(v8::Global::new(scope, value) != eval(scope, "() => 0").unwrap());

    let value = eval(scope, "async () => 0").unwrap();
    assert!(value.is_async_function());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Function>::try_from(value).unwrap());
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(value != v8::Object::new(scope));
    assert!(v8::Global::new(scope, value) != v8::Object::new(scope));

    let value = eval(scope, "[]").unwrap();
    assert!(value.is_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Array>::try_from(value).unwrap());
    assert!(value != v8::Array::new(scope, 0));

    let value = eval(scope, "9007199254740995n").unwrap();
    assert!(value.is_big_int());
    assert!(value.to_big_int(scope).is_some());
    assert!(value == value);
    assert!(value == v8::Local::<v8::BigInt>::try_from(value).unwrap());
    assert!(value == eval(scope, "1801439850948199n * 5n").unwrap());
    assert!(value != eval(scope, "1801439850948199 * 5").unwrap());
    let detail_string = value.to_detail_string(scope).unwrap();
    let detail_string = detail_string.to_rust_string_lossy(scope);
    assert_eq!("9007199254740995", detail_string);

    let value = eval(scope, "123").unwrap();
    assert!(value.is_number());
    assert!(value.is_int32());
    assert!(value.is_uint32());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Number>::try_from(value).unwrap());
    assert!(value == v8::Integer::new(scope, 123));
    assert!(value == v8::Number::new(scope, 123f64));
    assert!(value == value.to_int32(scope).unwrap());
    assert!(value != value.to_string(scope).unwrap());
    assert_eq!(123, value.to_uint32(scope).unwrap().value());
    assert_eq!(123, value.to_int32(scope).unwrap().value());
    assert_eq!(123, value.to_integer(scope).unwrap().value());
    assert_eq!(123, value.integer_value(scope).unwrap());
    assert_eq!(123, value.uint32_value(scope).unwrap());
    assert_eq!(123, value.int32_value(scope).unwrap());

    let value = eval(scope, "12.3").unwrap();
    assert!(value.is_number());
    assert!(!value.is_int32());
    assert!(!value.is_uint32());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Number>::try_from(value).unwrap());
    assert!(value == v8::Number::new(scope, 12.3f64));
    assert!(value != value.to_integer(scope).unwrap());
    assert!(12.3 - value.number_value(scope).unwrap() < 0.00001);

    let value = eval(scope, "-123").unwrap();
    assert!(value.is_number());
    assert!(value.is_int32());
    assert!(!value.is_uint32());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Int32>::try_from(value).unwrap());
    assert!(value == v8::Integer::new(scope, -123));
    assert!(value == v8::Number::new(scope, -123f64));
    assert!(value != v8::String::new(scope, "-123").unwrap());
    assert!(
      value
        == v8::Integer::new_from_unsigned(scope, -123i32 as u32)
          .to_int32(scope)
          .unwrap()
    );
    // The following test does not pass. This appears to be a V8 bug.
    // assert!(value != value.to_uint32(scope).unwrap());

    let value = eval(scope, "NaN").unwrap();
    assert!(value.is_number());
    assert!(!value.is_int32());
    assert!(!value.is_uint32());
    assert!(value != value);
    assert!(
      value.to_string(scope).unwrap() == v8::String::new(scope, "NaN").unwrap()
    );

    let value = eval(scope, "({})").unwrap();
    assert!(value.is_object());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Object>::try_from(value).unwrap());
    assert!(value == v8::Global::new(scope, value));
    assert!(v8::Global::new(scope, value) == v8::Global::new(scope, value));
    assert!(value != v8::Object::new(scope));
    assert!(v8::Global::new(scope, value) != v8::Object::new(scope));

    let value = eval(scope, "new Date()").unwrap();
    assert!(value.is_date());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Date>::try_from(value).unwrap());
    assert!(value != eval(scope, "new Date()").unwrap());

    let value = eval(scope, "(function(){return arguments})()").unwrap();
    assert!(value.is_arguments_object());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Object>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Promise(function(){})").unwrap();
    assert!(value.is_promise());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Promise>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Map()").unwrap();
    assert!(value.is_map());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Map>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Set").unwrap();
    assert!(value.is_set());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Set>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Map().entries()").unwrap();
    assert!(value.is_map_iterator());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Object>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Set().entries()").unwrap();
    assert!(value.is_set_iterator());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Object>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new WeakMap()").unwrap();
    assert!(value.is_weak_map());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Object>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new WeakSet()").unwrap();
    assert!(value.is_weak_set());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Object>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new ArrayBuffer(8)").unwrap();
    assert!(value.is_array_buffer());
    assert!(value == value);
    assert!(value == v8::Local::<v8::ArrayBuffer>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Uint8Array([])").unwrap();
    assert!(value.is_uint8_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Uint8Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Uint8ClampedArray([])").unwrap();
    assert!(value.is_uint8_clamped_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(
      value == v8::Local::<v8::Uint8ClampedArray>::try_from(value).unwrap()
    );
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Int8Array([])").unwrap();
    assert!(value.is_int8_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Int8Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Uint16Array([])").unwrap();
    assert!(value.is_uint16_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Uint16Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Int16Array([])").unwrap();
    assert!(value.is_int16_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Int16Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Uint32Array([])").unwrap();
    assert!(value.is_uint32_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Uint32Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Int32Array([])").unwrap();
    assert!(value.is_int32_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Int32Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Float32Array([])").unwrap();
    assert!(value.is_float32_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Float32Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Float64Array([])").unwrap();
    assert!(value.is_float64_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Float64Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new BigInt64Array([])").unwrap();
    assert!(value.is_big_int64_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::BigInt64Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new BigUint64Array([])").unwrap();
    assert!(value.is_big_uint64_array());
    assert!(value.is_array_buffer_view());
    assert!(value.is_typed_array());
    assert!(value == value);
    assert!(value == v8::Local::<v8::BigUint64Array>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new SharedArrayBuffer(64)").unwrap();
    assert!(value.is_shared_array_buffer());
    assert!(value == value);
    assert!(
      value == v8::Local::<v8::SharedArrayBuffer>::try_from(value).unwrap()
    );
    assert!(value != v8::Object::new(scope));

    let value = eval(scope, "new Proxy({},{})").unwrap();
    assert!(value.is_proxy());
    assert!(value == value);
    assert!(value == v8::Local::<v8::Proxy>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));

    // Other checker, Just check if it can be called
    value.is_external();
    value.is_module_namespace_object();
    value.is_wasm_module_object();
  }
}

#[test]
fn try_from_local() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    {
      let value: v8::Local<v8::Value> = v8::undefined(scope).into();
      let _primitive = v8::Local::<v8::Primitive>::try_from(value).unwrap();
      assert_eq!(
        v8::Local::<v8::Object>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "Object expected"
      );
      assert_eq!(
        v8::Local::<v8::Int32>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "Int32 expected"
      );
    }

    {
      let value: v8::Local<v8::Value> = v8::Boolean::new(scope, true).into();
      let primitive = v8::Local::<v8::Primitive>::try_from(value).unwrap();
      let _boolean = v8::Local::<v8::Boolean>::try_from(value).unwrap();
      let _boolean = v8::Local::<v8::Boolean>::try_from(primitive).unwrap();
      assert_eq!(
        v8::Local::<v8::String>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "String expected"
      );
      assert_eq!(
        v8::Local::<v8::Number>::try_from(primitive)
          .err()
          .unwrap()
          .to_string(),
        "Number expected"
      );
    }

    {
      let value: v8::Local<v8::Value> = v8::Number::new(scope, -1234f64).into();
      let primitive = v8::Local::<v8::Primitive>::try_from(value).unwrap();
      let _number = v8::Local::<v8::Number>::try_from(value).unwrap();
      let number = v8::Local::<v8::Number>::try_from(primitive).unwrap();
      let _integer = v8::Local::<v8::Integer>::try_from(value).unwrap();
      let _integer = v8::Local::<v8::Integer>::try_from(primitive).unwrap();
      let integer = v8::Local::<v8::Integer>::try_from(number).unwrap();
      let _int32 = v8::Local::<v8::Int32>::try_from(value).unwrap();
      let _int32 = v8::Local::<v8::Int32>::try_from(primitive).unwrap();
      let _int32 = v8::Local::<v8::Int32>::try_from(integer).unwrap();
      let _int32 = v8::Local::<v8::Int32>::try_from(number).unwrap();
      assert_eq!(
        v8::Local::<v8::String>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "String expected"
      );
      assert_eq!(
        v8::Local::<v8::Boolean>::try_from(primitive)
          .err()
          .unwrap()
          .to_string(),
        "Boolean expected"
      );
      assert_eq!(
        v8::Local::<v8::Uint32>::try_from(integer)
          .err()
          .unwrap()
          .to_string(),
        "Uint32 expected"
      );
    }

    {
      let value: v8::Local<v8::Value> = eval(scope, "(() => {})").unwrap();
      let object = v8::Local::<v8::Object>::try_from(value).unwrap();
      let _function = v8::Local::<v8::Function>::try_from(value).unwrap();
      let _function = v8::Local::<v8::Function>::try_from(object).unwrap();
      assert_eq!(
        v8::Local::<v8::Primitive>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "Primitive expected"
      );
      assert_eq!(
        v8::Local::<v8::BigInt>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "BigInt expected"
      );
      assert_eq!(
        v8::Local::<v8::NumberObject>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "NumberObject expected"
      );
      assert_eq!(
        v8::Local::<v8::NumberObject>::try_from(object)
          .err()
          .unwrap()
          .to_string(),
        "NumberObject expected"
      );
      assert_eq!(
        v8::Local::<v8::Set>::try_from(value)
          .err()
          .unwrap()
          .to_string(),
        "Set expected"
      );
      assert_eq!(
        v8::Local::<v8::Set>::try_from(object)
          .err()
          .unwrap()
          .to_string(),
        "Set expected"
      );
    }
  }
}

struct ClientCounter {
  base: v8::inspector::V8InspectorClientBase,
  count_run_message_loop_on_pause: usize,
  count_quit_message_loop_on_pause: usize,
  count_run_if_waiting_for_debugger: usize,
}

impl ClientCounter {
  fn new() -> Self {
    Self {
      base: v8::inspector::V8InspectorClientBase::new::<Self>(),
      count_run_message_loop_on_pause: 0,
      count_quit_message_loop_on_pause: 0,
      count_run_if_waiting_for_debugger: 0,
    }
  }
}

impl v8::inspector::V8InspectorClientImpl for ClientCounter {
  fn base(&self) -> &v8::inspector::V8InspectorClientBase {
    &self.base
  }

  fn base_mut(&mut self) -> &mut v8::inspector::V8InspectorClientBase {
    &mut self.base
  }

  fn run_message_loop_on_pause(&mut self, context_group_id: i32) {
    assert_eq!(context_group_id, 1);
    self.count_run_message_loop_on_pause += 1;
  }

  fn quit_message_loop_on_pause(&mut self) {
    self.count_quit_message_loop_on_pause += 1;
  }

  fn run_if_waiting_for_debugger(&mut self, context_group_id: i32) {
    assert_eq!(context_group_id, 1);
    self.count_run_message_loop_on_pause += 1;
  }
}

struct ChannelCounter {
  base: v8::inspector::ChannelBase,
  count_send_response: usize,
  count_send_notification: usize,
  count_flush_protocol_notifications: usize,
}

impl ChannelCounter {
  pub fn new() -> Self {
    Self {
      base: v8::inspector::ChannelBase::new::<Self>(),
      count_send_response: 0,
      count_send_notification: 0,
      count_flush_protocol_notifications: 0,
    }
  }
}

impl v8::inspector::ChannelImpl for ChannelCounter {
  fn base(&self) -> &v8::inspector::ChannelBase {
    &self.base
  }
  fn base_mut(&mut self) -> &mut v8::inspector::ChannelBase {
    &mut self.base
  }
  fn send_response(
    &mut self,
    call_id: i32,
    message: v8::UniquePtr<v8::inspector::StringBuffer>,
  ) {
    println!(
      "send_response call_id {} message {}",
      call_id,
      message.unwrap().string()
    );
    self.count_send_response += 1;
  }
  fn send_notification(
    &mut self,
    message: v8::UniquePtr<v8::inspector::StringBuffer>,
  ) {
    println!("send_notificatio message {}", message.unwrap().string());
    self.count_send_notification += 1;
  }
  fn flush_protocol_notifications(&mut self) {
    self.count_flush_protocol_notifications += 1;
  }
}

#[test]
fn inspector_dispatch_protocol_message() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  use v8::inspector::*;
  let mut default_client = ClientCounter::new();
  let mut inspector = V8Inspector::create(isolate, &mut default_client);

  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let mut _scope = v8::ContextScope::new(scope, context);

  let name = b"";
  let name_view = StringView::from(&name[..]);
  inspector.context_created(context, 1, name_view);
  let mut channel = ChannelCounter::new();
  let state = b"{}";
  let state_view = StringView::from(&state[..]);
  let mut session = inspector.connect(1, &mut channel, state_view);
  let message = String::from(
    r#"{"id":1,"method":"Network.enable","params":{"maxPostDataSize":65536}}"#,
  );
  let message = &message.into_bytes()[..];
  let string_view = StringView::from(message);
  session.dispatch_protocol_message(string_view);
  assert_eq!(channel.count_send_response, 1);
  assert_eq!(channel.count_send_notification, 0);
  assert_eq!(channel.count_flush_protocol_notifications, 0);
}

#[test]
fn inspector_schedule_pause_on_next_statement() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  use v8::inspector::*;
  let mut client = ClientCounter::new();
  let mut inspector = V8Inspector::create(isolate, &mut client);

  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  let mut channel = ChannelCounter::new();
  let state = b"{}";
  let state_view = StringView::from(&state[..]);
  let mut session = inspector.connect(1, &mut channel, state_view);

  let name = b"";
  let name_view = StringView::from(&name[..]);
  inspector.context_created(context, 1, name_view);

  // In order for schedule_pause_on_next_statement to work, it seems you need
  // to first enable the debugger.
  let message = String::from(r#"{"id":1,"method":"Debugger.enable"}"#);
  let message = &message.into_bytes()[..];
  let message = StringView::from(message);
  session.dispatch_protocol_message(message);

  // The following commented out block seems to act similarly to
  // schedule_pause_on_next_statement. I'm not sure if they have the exact same
  // effect tho.
  //   let message = String::from(r#"{"id":2,"method":"Debugger.pause"}"#);
  //   let message = &message.into_bytes()[..];
  //   let message = StringView::from(message);
  //   session.dispatch_protocol_message(&message);
  let reason = b"";
  let reason = StringView::from(&reason[..]);
  let detail = b"";
  let detail = StringView::from(&detail[..]);
  session.schedule_pause_on_next_statement(reason, detail);

  assert_eq!(channel.count_send_response, 1);
  assert_eq!(channel.count_send_notification, 0);
  assert_eq!(channel.count_flush_protocol_notifications, 0);
  assert_eq!(client.count_run_message_loop_on_pause, 0);
  assert_eq!(client.count_quit_message_loop_on_pause, 0);
  assert_eq!(client.count_run_if_waiting_for_debugger, 0);

  let r = eval(scope, "1+2").unwrap();
  assert!(r.is_number());

  assert_eq!(channel.count_send_response, 1);
  assert_eq!(channel.count_send_notification, 3);
  assert_eq!(channel.count_flush_protocol_notifications, 1);
  assert_eq!(client.count_run_message_loop_on_pause, 1);
  assert_eq!(client.count_quit_message_loop_on_pause, 0);
  assert_eq!(client.count_run_if_waiting_for_debugger, 0);
}

#[test]
fn inspector_console_api_message() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  use v8::inspector::*;

  struct Client {
    base: V8InspectorClientBase,
    messages: Vec<String>,
  }

  impl Client {
    fn new() -> Self {
      Self {
        base: V8InspectorClientBase::new::<Self>(),
        messages: Vec::new(),
      }
    }
  }

  impl V8InspectorClientImpl for Client {
    fn base(&self) -> &V8InspectorClientBase {
      &self.base
    }

    fn base_mut(&mut self) -> &mut V8InspectorClientBase {
      &mut self.base
    }

    fn console_api_message(
      &mut self,
      _context_group_id: i32,
      _level: i32,
      message: &StringView,
      _url: &StringView,
      _line_number: u32,
      _column_number: u32,
      _stack_trace: &mut V8StackTrace,
    ) {
      self.messages.push(message.to_string());
    }
  }

  let mut client = Client::new();
  let mut inspector = V8Inspector::create(isolate, &mut client);

  let scope = &mut v8::HandleScope::new(isolate);
  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  let name = b"";
  let name_view = StringView::from(&name[..]);
  inspector.context_created(context, 1, name_view);

  let source = r#"
    console.log("one");
    console.error("two");
    console.trace("three");
  "#;
  let _ = eval(scope, source).unwrap();
  assert_eq!(client.messages, vec!["one", "two", "three"]);
}

#[test]
fn context_from_object_template() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let object_templ = v8::ObjectTemplate::new(scope);
    let function_templ = v8::FunctionTemplate::new(scope, fortytwo_callback);
    let name = v8::String::new(scope, "f").unwrap();
    object_templ.set(name.into(), function_templ.into());
    let context = v8::Context::new_from_template(scope, object_templ);
    let scope = &mut v8::ContextScope::new(scope, context);
    let actual = eval(scope, "f()").unwrap();
    let expected = v8::Integer::new(scope, 42);
    assert!(expected.strict_equals(actual));
  }
}

#[test]
fn take_heap_snapshot() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);
    let source = r#"
      {
        class Eyecatcher {}
        const eyecatchers = globalThis.eyecatchers = [];
        for (let i = 0; i < 1e4; i++) eyecatchers.push(new Eyecatcher);
      }
    "#;
    let _ = eval(scope, source).unwrap();
    let mut vec = Vec::<u8>::new();
    scope.take_heap_snapshot(|chunk| {
      vec.extend_from_slice(chunk);
      true
    });
    let s = std::str::from_utf8(&vec).unwrap();
    assert!(s.find(r#""Eyecatcher""#).is_some());
  }
}

#[test]
fn test_prototype_api() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let obj = v8::Object::new(scope);
    let proto_obj = v8::Object::new(scope);
    let key_local: v8::Local<v8::Value> =
      v8::String::new(scope, "test_proto_key").unwrap().into();
    let value_local: v8::Local<v8::Value> =
      v8::String::new(scope, "test_proto_value").unwrap().into();
    proto_obj.set(scope, key_local, value_local);
    obj.set_prototype(scope, proto_obj.into());

    assert!(obj
      .get_prototype(scope)
      .unwrap()
      .same_value(proto_obj.into()));

    let sub_gotten = obj.get(scope, key_local).unwrap();
    assert!(sub_gotten.is_string());
    let sub_gotten = sub_gotten.to_string(scope).unwrap();
    assert_eq!(sub_gotten.to_rust_string_lossy(scope), "test_proto_value");
  }
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let obj = v8::Object::new(scope);
    let null = v8::null(scope);
    obj.set_prototype(scope, null.into());

    assert!(obj.get_prototype(scope).unwrap().is_null());
  }
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let val = eval(scope, "({ __proto__: null })").unwrap();
    let obj = val.to_object(scope).unwrap();

    assert!(obj.get_prototype(scope).unwrap().is_null());
  }
}

#[test]
fn test_map_api() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());
  {
    let scope = &mut v8::HandleScope::new(isolate);
    let context = v8::Context::new(scope);
    let scope = &mut v8::ContextScope::new(scope, context);

    let value = eval(scope, "new Map([['r','s'],['v',8]])").unwrap();
    assert!(value.is_map());
    assert!(value == v8::Local::<v8::Map>::try_from(value).unwrap());
    assert!(value != v8::Object::new(scope));
    assert_eq!(v8::Local::<v8::Map>::try_from(value).unwrap().size(), 2);
    let map = v8::Local::<v8::Map>::try_from(value).unwrap();
    assert_eq!(map.size(), 2);
    let map_array = map.as_array(scope);
    assert_eq!(map_array.length(), 4);
    assert!(
      map_array.get_index(scope, 0).unwrap()
        == v8::String::new(scope, "r").unwrap()
    );
    assert!(
      map_array.get_index(scope, 1).unwrap()
        == v8::String::new(scope, "s").unwrap()
    );
    assert!(
      map_array.get_index(scope, 2).unwrap()
        == v8::String::new(scope, "v").unwrap()
    );
    assert!(
      map_array.get_index(scope, 3).unwrap() == v8::Number::new(scope, 8f64)
    );
  }
}

#[test]
fn test_object_get_property_names() {
  let _setup_guard = setup();
  let isolate = &mut v8::Isolate::new(Default::default());

  let scope = &mut v8::HandleScope::new(isolate);

  let context = v8::Context::new(scope);
  let scope = &mut v8::ContextScope::new(scope, context);

  let js_test_str: v8::Local<v8::Value> =
    v8::String::new(scope, "test").unwrap().into();
  let js_proto_test_str: v8::Local<v8::Value> =
    v8::String::new(scope, "proto_test").unwrap().into();
  let js_test_symbol: v8::Local<v8::Value> =
    eval(scope, "Symbol('test_symbol')").unwrap();
  let js_null: v8::Local<v8::Value> = v8::null(scope).into();
  let js_sort_fn: v8::Local<v8::Function> = eval(scope, "Array.prototype.sort")
    .unwrap()
    .try_into()
    .unwrap();

  {
    let obj = v8::Object::new(scope);
    obj.set(scope, js_test_str, js_null);

    let proto_obj = v8::Object::new(scope);
    proto_obj.set(scope, js_proto_test_str, js_null);
    obj.set_prototype(scope, proto_obj.into());

    let own_props = obj.get_own_property_names(scope).unwrap();
    assert_eq!(own_props.length(), 1);
    assert!(own_props.get_index(scope, 0).unwrap() == js_test_str);

    let proto_props = proto_obj.get_own_property_names(scope).unwrap();
    assert_eq!(proto_props.length(), 1);
    assert!(proto_props.get_index(scope, 0).unwrap() == js_proto_test_str);

    let all_props = obj.get_property_names(scope).unwrap();
    js_sort_fn.call(scope, all_props.into(), &[]).unwrap();
    assert_eq!(all_props.length(), 2);
    assert!(all_props.get_index(scope, 0).unwrap() == js_proto_test_str);
    assert!(all_props.get_index(scope, 1).unwrap() == js_test_str);
  }

  {
    let obj = v8::Object::new(scope);
    obj.set(scope, js_test_str, js_null);
    obj.set(scope, js_test_symbol, js_null);

    let own_props = obj.get_own_property_names(scope).unwrap();
    assert_eq!(own_props.length(), 1);
    assert!(own_props.get_index(scope, 0).unwrap() == js_test_str);
  }
}

#[test]
fn module_snapshot() {
  let _setup_guard = setup();

  let startup_data = {
    let mut snapshot_creator = v8::SnapshotCreator::new(None);
    // TODO(ry) this shouldn't be necessary. workaround unfinished business in
    // the scope type system.
    let mut isolate = unsafe { snapshot_creator.get_owned_isolate() };
    {
      let scope = &mut v8::HandleScope::new(&mut isolate);
      let context = v8::Context::new(scope);
      let scope = &mut v8::ContextScope::new(scope, context);

      let source_text = v8::String::new(
        scope,
        "import 'globalThis.b = 42';\n\
         globalThis.a = 3",
      )
      .unwrap();
      let origin = mock_script_origin(scope, "foo.js");
      let source = v8::script_compiler::Source::new(source_text, &origin);

      let module = v8::script_compiler::compile_module(scope, source).unwrap();
      assert_eq!(v8::ModuleStatus::Uninstantiated, module.get_status());

      let result = module.instantiate_module(
        scope,
        compile_specifier_as_module_resolve_callback,
      );
      assert!(result.unwrap());
      assert_eq!(v8::ModuleStatus::Instantiated, module.get_status());

      let result = module.evaluate(scope);
      assert!(result.is_some());
      assert_eq!(v8::ModuleStatus::Evaluated, module.get_status());

      snapshot_creator.set_default_context(context);
    }
    std::mem::forget(isolate); // TODO(ry) this shouldn't be necessary.
    snapshot_creator
      .create_blob(v8::FunctionCodeHandling::Keep)
      .unwrap()
  };
  assert!(startup_data.len() > 0);
  {
    let params = v8::Isolate::create_params().snapshot_blob(startup_data);
    let isolate = &mut v8::Isolate::new(params);
    {
      let scope = &mut v8::HandleScope::new(isolate);
      let context = v8::Context::new(scope);
      let scope = &mut v8::ContextScope::new(scope, context);

      let true_val = v8::Boolean::new(scope, true).into();

      let source = v8::String::new(scope, "a === 3").unwrap();
      let script = v8::Script::compile(scope, source, None).unwrap();
      let result = script.run(scope).unwrap();
      assert!(result.same_value(true_val));

      let source = v8::String::new(scope, "b === 42").unwrap();
      let script = v8::Script::compile(scope, source, None).unwrap();
      let result = script.run(scope).unwrap();
      assert!(result.same_value(true_val));
    }
  }
}
