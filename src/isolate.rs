// Copyright 2019-2020 the Deno authors. All rights reserved. MIT license.
use crate::array_buffer::Allocator;
use crate::external_references::ExternalReferences;
use crate::promise::PromiseRejectMessage;
use crate::support::intptr_t;
use crate::support::Delete;
use crate::support::Opaque;
use crate::support::UniqueRef;
use crate::Context;
use crate::Function;
use crate::Local;
use crate::Message;
use crate::Module;
use crate::Object;
use crate::Promise;
use crate::ScriptOrModule;
use crate::StartupData;
use crate::String;
use crate::Value;
use std::ffi::c_void;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ptr::NonNull;

pub type MessageCallback = extern "C" fn(Local<Message>, Local<Value>);

pub type PromiseRejectCallback = extern "C" fn(PromiseRejectMessage);

/// HostInitializeImportMetaObjectCallback is called the first time import.meta
/// is accessed for a module. Subsequent access will reuse the same value.
///
/// The method combines two implementation-defined abstract operations into one:
/// HostGetImportMetaProperties and HostFinalizeImportMeta.
///
/// The embedder should use v8::Object::CreateDataProperty to add properties on
/// the meta object.
pub type HostInitializeImportMetaObjectCallback =
  extern "C" fn(Local<Context>, Local<Module>, Local<Object>);

/// HostImportModuleDynamicallyCallback is called when we require the
/// embedder to load a module. This is used as part of the dynamic
/// import syntax.
///
/// The referrer contains metadata about the script/module that calls
/// import.
///
/// The specifier is the name of the module that should be imported.
///
/// The embedder must compile, instantiate, evaluate the Module, and
/// obtain it's namespace object.
///
/// The Promise returned from this function is forwarded to userland
/// JavaScript. The embedder must resolve this promise with the module
/// namespace object. In case of an exception, the embedder must reject
/// this promise with the exception. If the promise creation itself
/// fails (e.g. due to stack overflow), the embedder must propagate
/// that exception by returning an empty MaybeLocal.
pub type HostImportModuleDynamicallyCallback = extern "C" fn(
  Local<Context>,
  Local<ScriptOrModule>,
  Local<String>,
) -> *mut Promise;

extern "C" {
  fn v8__Isolate__New(params: *mut CreateParams) -> *mut Isolate;
  fn v8__Isolate__Dispose(this: *mut Isolate);
  fn v8__Isolate__SetData(this: *mut Isolate, slot: u32, data: *mut c_void);
  fn v8__Isolate__GetData(this: *const Isolate, slot: u32) -> *mut c_void;
  fn v8__Isolate__GetNumberOfDataSlots(this: *const Isolate) -> u32;
  fn v8__Isolate__Enter(this: *mut Isolate);
  fn v8__Isolate__Exit(this: *mut Isolate);
  fn v8__Isolate__GetCurrentContext(this: *mut Isolate) -> *mut Context;
  fn v8__Isolate__SetCaptureStackTraceForUncaughtExceptions(
    this: *mut Isolate,
    caputre: bool,
    frame_limit: i32,
  );
  fn v8__Isolate__AddMessageListener(
    this: &mut Isolate,
    callback: MessageCallback,
  ) -> bool;
  fn v8__Isolate__SetPromiseRejectCallback(
    isolate: *mut Isolate,
    callback: PromiseRejectCallback,
  );
  fn v8__Isolate__SetHostInitializeImportMetaObjectCallback(
    isolate: *mut Isolate,
    callback: HostInitializeImportMetaObjectCallback,
  );
  fn v8__Isolate__SetHostImportModuleDynamicallyCallback(
    isolate: *mut Isolate,
    callback: HostImportModuleDynamicallyCallback,
  );
  fn v8__Isolate__ThrowException(
    isolate: &Isolate,
    exception: &Value,
  ) -> *mut Value;
  fn v8__Isolate__TerminateExecution(isolate: &Isolate);
  fn v8__Isolate__IsExecutionTerminating(isolate: &Isolate) -> bool;
  fn v8__Isolate__CancelTerminateExecution(isolate: &Isolate);
  fn v8__Isolate__RunMicrotasks(isolate: &Isolate);
  fn v8__Isolate__EnqueueMicrotask(isolate: &Isolate, microtask: *mut Function);

  fn v8__Isolate__CreateParams__NEW() -> *mut CreateParams;
  fn v8__Isolate__CreateParams__DELETE(this: &mut CreateParams);
  fn v8__Isolate__CreateParams__SET__array_buffer_allocator(
    this: &mut CreateParams,
    value: *mut Allocator,
  );
  fn v8__Isolate__CreateParams__SET__external_references(
    this: &mut CreateParams,
    value: *const intptr_t,
  );
  fn v8__Isolate__CreateParams__SET__snapshot_blob(
    this: &mut CreateParams,
    snapshot_blob: *mut StartupData,
  );
}

#[repr(C)]
/// Isolate represents an isolated instance of the V8 engine.  V8 isolates have
/// completely separate states.  Objects from one isolate must not be used in
/// other isolates.  The embedder can create multiple isolates and use them in
/// parallel in multiple threads.  An isolate can be entered by at most one
/// thread at any given time.  The Locker/Unlocker API must be used to
/// synchronize.
pub struct Isolate(Opaque);

impl Isolate {
  /// Creates a new isolate.  Does not change the currently entered
  /// isolate.
  ///
  /// When an isolate is no longer used its resources should be freed
  /// by calling V8::dispose().  Using the delete operator is not allowed.
  ///
  /// V8::initialize() must have run prior to this.
  #[allow(clippy::new_ret_no_self)]
  pub fn new(params: UniqueRef<CreateParams>) -> OwnedIsolate {
    // TODO: support CreateParams.
    crate::V8::assert_initialized();
    unsafe { new_owned_isolate(v8__Isolate__New(params.into_raw())) }
  }

  /// Initial configuration parameters for a new Isolate.
  pub fn create_params() -> UniqueRef<CreateParams> {
    CreateParams::new()
  }

  /// Associate embedder-specific data with the isolate. |slot| has to be
  /// between 0 and GetNumberOfDataSlots() - 1.
  pub unsafe fn set_data(&mut self, slot: u32, ptr: *mut c_void) {
    v8__Isolate__SetData(self, slot, ptr)
  }

  /// Retrieve embedder-specific data from the isolate.
  /// Returns NULL if SetData has never been called for the given |slot|.
  pub fn get_data(&self, slot: u32) -> *mut c_void {
    unsafe { v8__Isolate__GetData(self, slot) }
  }

  /// Returns the maximum number of available embedder data slots. Valid slots
  /// are in the range of 0 - GetNumberOfDataSlots() - 1.
  pub fn get_number_of_data_slots(&self) -> u32 {
    unsafe { v8__Isolate__GetNumberOfDataSlots(self) }
  }

  /// Sets this isolate as the entered one for the current thread.
  /// Saves the previously entered one (if any), so that it can be
  /// restored when exiting.  Re-entering an isolate is allowed.
  pub fn enter(&mut self) {
    unsafe { v8__Isolate__Enter(self) }
  }

  /// Exits this isolate by restoring the previously entered one in the
  /// current thread.  The isolate may still stay the same, if it was
  /// entered more than once.
  ///
  /// Requires: self == Isolate::GetCurrent().
  pub fn exit(&mut self) {
    unsafe { v8__Isolate__Exit(self) }
  }

  /// Returns the context of the currently running JavaScript, or the context
  /// on the top of the stack if no JavaScript is running.
  pub fn get_current_context<'sc>(&mut self) -> Local<'sc, Context> {
    unsafe { Local::from_raw(v8__Isolate__GetCurrentContext(self)).unwrap() }
  }

  /// Tells V8 to capture current stack trace when uncaught exception occurs
  /// and report it to the message listeners. The option is off by default.
  pub fn set_capture_stack_trace_for_uncaught_exceptions(
    &mut self,
    capture: bool,
    frame_limit: i32,
  ) {
    unsafe {
      v8__Isolate__SetCaptureStackTraceForUncaughtExceptions(
        self,
        capture,
        frame_limit,
      )
    }
  }

  /// Adds a message listener (errors only).
  ///
  /// The same message listener can be added more than once and in that
  /// case it will be called more than once for each message.
  ///
  /// The exception object will be passed to the callback.
  pub fn add_message_listener(&mut self, callback: MessageCallback) -> bool {
    unsafe { v8__Isolate__AddMessageListener(self, callback) }
  }

  /// Set callback to notify about promise reject with no handler, or
  /// revocation of such a previous notification once the handler is added.
  pub fn set_promise_reject_callback(
    &mut self,
    callback: PromiseRejectCallback,
  ) {
    unsafe { v8__Isolate__SetPromiseRejectCallback(self, callback) }
  }
  /// This specifies the callback called by the upcoming importa.meta
  /// language feature to retrieve host-defined meta data for a module.
  pub fn set_host_initialize_import_meta_object_callback(
    &mut self,
    callback: HostInitializeImportMetaObjectCallback,
  ) {
    unsafe {
      v8__Isolate__SetHostInitializeImportMetaObjectCallback(self, callback)
    }
  }

  /// This specifies the callback called by the upcoming dynamic
  /// import() language feature to load modules.
  pub fn set_host_import_module_dynamically_callback(
    &mut self,
    callback: HostImportModuleDynamicallyCallback,
  ) {
    unsafe {
      v8__Isolate__SetHostImportModuleDynamicallyCallback(self, callback)
    }
  }

  /// Schedules an exception to be thrown when returning to JavaScript. When an
  /// exception has been scheduled it is illegal to invoke any JavaScript
  /// operation; the caller must return immediately and only after the exception
  /// has been handled does it become legal to invoke JavaScript operations.
  pub fn throw_exception<'sc>(
    &self,
    exception: Local<'_, Value>,
  ) -> Local<'sc, Value> {
    unsafe {
      let ptr = v8__Isolate__ThrowException(self, &exception);
      Local::from_raw(ptr).unwrap()
    }
  }

  /// Forcefully terminate the current thread of JavaScript execution
  /// in the given isolate.
  ///
  /// This method can be used by any thread even if that thread has not
  /// acquired the V8 lock with a Locker object.
  pub fn terminate_execution(&self) {
    unsafe { v8__Isolate__TerminateExecution(self) }
  }

  /// Is V8 terminating JavaScript execution.
  ///
  /// Returns true if JavaScript execution is currently terminating
  /// because of a call to TerminateExecution.  In that case there are
  /// still JavaScript frames on the stack and the termination
  /// exception is still active.
  pub fn is_execution_terminating(&self) -> bool {
    unsafe { v8__Isolate__IsExecutionTerminating(self) }
  }

  /// Resume execution capability in the given isolate, whose execution
  /// was previously forcefully terminated using TerminateExecution().
  ///
  /// When execution is forcefully terminated using TerminateExecution(),
  /// the isolate can not resume execution until all JavaScript frames
  /// have propagated the uncatchable exception which is generated.  This
  /// method allows the program embedding the engine to handle the
  /// termination event and resume execution capability, even if
  /// JavaScript frames remain on the stack.
  ///
  /// This method can be used by any thread even if that thread has not
  /// acquired the V8 lock with a Locker object.
  pub fn cancel_terminate_execution(&self) {
    unsafe { v8__Isolate__CancelTerminateExecution(self) }
  }

  /// Runs the default MicrotaskQueue until it gets empty.
  /// Any exceptions thrown by microtask callbacks are swallowed.
  pub fn run_microtasks(&self) {
    unsafe { v8__Isolate__RunMicrotasks(self) }
  }

  /// Enqueues the callback to the default MicrotaskQueue
  pub fn enqueue_microtask(&self, mut microtask: Local<Function>) {
    unsafe { v8__Isolate__EnqueueMicrotask(self, &mut *microtask) }
  }

  /// Disposes the isolate.  The isolate must not be entered by any
  /// thread to be disposable.
  pub unsafe fn dispose(&mut self) {
    v8__Isolate__Dispose(self)
  }
}

/// Internal method for constructing an OwnedIsolate.
pub unsafe fn new_owned_isolate(isolate_ptr: *mut Isolate) -> OwnedIsolate {
  OwnedIsolate(NonNull::new(isolate_ptr).unwrap())
}

/// Same as Isolate but gets disposed when it goes out of scope.
pub struct OwnedIsolate(NonNull<Isolate>);

impl Drop for OwnedIsolate {
  fn drop(&mut self) {
    unsafe { self.0.as_mut().dispose() }
  }
}

impl Deref for OwnedIsolate {
  type Target = Isolate;
  fn deref(&self) -> &Self::Target {
    unsafe { self.0.as_ref() }
  }
}

impl DerefMut for OwnedIsolate {
  fn deref_mut(&mut self) -> &mut Self::Target {
    unsafe { self.0.as_mut() }
  }
}

/// Initial configuration parameters for a new Isolate.
#[repr(C)]
pub struct CreateParams(Opaque);

impl CreateParams {
  pub fn new() -> UniqueRef<CreateParams> {
    unsafe { UniqueRef::from_raw(v8__Isolate__CreateParams__NEW()) }
  }

  /// The ArrayBuffer::Allocator to use for allocating and freeing the backing
  /// store of ArrayBuffers.
  ///
  /// If the shared_ptr version is used, the Isolate instance and every
  /// |BackingStore| allocated using this allocator hold a std::shared_ptr
  /// to the allocator, in order to facilitate lifetime
  /// management for the allocator instance.
  pub fn set_array_buffer_allocator(&mut self, value: UniqueRef<Allocator>) {
    unsafe {
      v8__Isolate__CreateParams__SET__array_buffer_allocator(
        self,
        value.into_raw(),
      )
    };
  }

  /// Specifies an optional nullptr-terminated array of raw addresses in the
  /// embedder that V8 can match against during serialization and use for
  /// deserialization. This array and its content must stay valid for the
  /// entire lifetime of the isolate.
  pub fn set_external_references(
    &mut self,
    external_references: &'static ExternalReferences,
  ) {
    unsafe {
      v8__Isolate__CreateParams__SET__external_references(
        self,
        external_references.as_ptr(),
      )
    };
  }

  /// Hand startup data to V8, in case the embedder has chosen to build
  /// V8 with external startup data.
  ///
  /// Note:
  /// - By default the startup data is linked into the V8 library, in which
  ///   case this function is not meaningful.
  /// - If this needs to be called, it needs to be called before V8
  ///   tries to make use of its built-ins.
  /// - To avoid unnecessary copies of data, V8 will point directly into the
  ///   given data blob, so pretty please keep it around until V8 exit.
  /// - Compression of the startup blob might be useful, but needs to
  ///   handled entirely on the embedders' side.
  /// - The call will abort if the data is invalid.
  pub fn set_snapshot_blob(&mut self, snapshot_blob: &StartupData) {
    unsafe {
      v8__Isolate__CreateParams__SET__snapshot_blob(
        self,
        snapshot_blob as *const _ as *mut StartupData,
      )
    };
  }
}

impl Delete for CreateParams {
  fn delete(&'static mut self) {
    unsafe { v8__Isolate__CreateParams__DELETE(self) }
  }
}
