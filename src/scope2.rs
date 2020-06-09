use std::any::Any;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use crate::isolate::IsolateAnnex;
use crate::Context;
use crate::Isolate;
use crate::Data;
use crate::undefined;

pub(crate) mod data {
  use super::*;

  pub(crate) struct ScopeState {
    isolate: NonNull<Isolate>,
    parent: Option<NonNull<ScopeState>>,
    context: Option<NonNull<Context>>,
    escape_slot: Option<NonNull<Option<EscapeSlot>>>,
    data: ScopeData,
  }

  enum ScopeData {
    None,
    Context,
    HandleScope(raw::HandleScope),
    EscapableHandleScope(raw::EscapableHandleScope),
  }

  impl ScopeState {
    fn new(isolate: &mut Isolate, data: ScopeData) -> Self {
      let mut parent = isolate.get_current_scope();
      let context = parent
        .as_mut()
        .map(|p| unsafe { p.as_mut() })
        .and_then(|p| p.context)
        .map(NonNull::from);
      let escape_slot = parent
        .as_mut()
        .map(|p| unsafe { p.as_mut() })
        .and_then(|p| p.escape_slot)
        .map(NonNull::from);
      Self {
        isolate: NonNull::from(isolate),
        parent,
        context,
        escape_slot,
        data,
      }
    }
  }

  impl Default for ScopeData {
    fn default() -> Self {
      Self::None
    }
  }

  //type HandleScopeData<'a> = ScopeData<'a, HandleScope>;
  //type EscapableHandleScopeData<'a> = ScopeData<'a, EscapableHandleScope>;
}

mod raw {
  use super::*;

  #[derive(Clone, Copy)]
  #[repr(transparent)]
  pub(super) struct Address(usize);

  #[repr(C)]
  #[derive(Eq, PartialEq)]
  pub(super) struct HandleScope {
    isolate_: *mut Isolate,
    prev_next_: *mut Address,
    prev_limit_: *mut Address,
  }

  impl HandleScope {
    pub(super) fn zeroed() -> Self {
      unsafe { MaybeUninit::<Self>::zeroed().assume_init() }
    }

    pub(super) fn init(&mut self, isolate: *mut Isolate) {
      assert!(self.isolate_.is_null());
      let buf = self as *mut _ as *mut MaybeUninit<Self>;
      unsafe { v8__HandleScope__CONSTRUCT(buf, isolate)};
    }
  }

  impl Drop for HandleScope {
    fn drop(&mut self) {
      assert!(!self.isolate_.is_null());
      unsafe { v8__HandleScope__DESTRUCT(self) };
    }
  }

  pub(super) struct EscapeSlot(NonNull<raw::Address>) {
    pub(super) fn new(isolate: *mut Isolate) -> Self {
      unsafe {
        let undefined = v8__Undefined(isolate);
        let data: &Data = &undefined;
        let local =  v8__Local__New(isolate, data);
        let data: &Data = &undefined;
        let slot = NonNull::new_unchecked(data as *const _ as *mut _);
        slot.cast()
      } 
    }
  }

  struct EscapableHandleScope {
    handle_scope: raw::HandleScope,
    escape_slot: raw::EscapeSlot,
  }

  impl EscapableHandleScope {
    pub(super) fn new(isolate: *mut Isolate) -> Self {
      // Node: the `EscapeSlot` *must* be created *before* the `HandleScope`.
      Self {
        handle_scope: HandleScope::zeroed(),
        escape_slot: EscapeSlot::new(isolate)
      }
    }

      pub(super) fn init(&mut self, isolate: *mut Isolate) {
        self.handle_scope.init(isolate)
      }
    }
  }
  
  extern "C" {
    pub fn v8__Isolate__GetCurrentContext(
      isolate: *mut Isolate,
    ) -> *const Context;
    pub fn v8__Isolate__GetEnteredOrMicrotaskContext(
      isolate: *mut Isolate,
    ) -> *const Context;

    pub fn v8__Context__GetIsolate(this: *const Context) -> *mut Isolate;
    pub fn v8__Context__Enter(this: *const Context);
    pub fn v8__Context__Exit(this: *const Context);

    pub fn v8__HandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<HandleScope>,
      isolate: *mut Isolate,
    );
    pub fn v8__HandleScope__DESTRUCT(this: *mut HandleScope);

    pub fn v8__Undefined(isolate: *mut Isolate) -> *const Primitive;
    pub fn v8__Local__New(
      isolate: *mut Isolate,
      other: *const Data,
    ) -> *const Data;

    pub fn v8__TryCatch__CONSTRUCT(
      buf: *mut MaybeUninit<TryCatch>,
      isolate: *mut Isolate,
    );
    pub fn v8__TryCatch__DESTRUCT(this: *mut TryCatch);
    pub fn v8__TryCatch__HasCaught(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__CanContinue(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__HasTerminated(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__Exception(this: *const TryCatch) -> *const Value;
    pub fn v8__TryCatch__StackTrace(
      this: *const TryCatch,
      context: *const Context,
    ) -> *const Value;
    pub fn v8__TryCatch__Message(this: *const TryCatch) -> *const Message;
    pub fn v8__TryCatch__Reset(this: *mut TryCatch);
    pub fn v8__TryCatch__ReThrow(this: *mut TryCatch) -> *const Value;
    pub fn v8__TryCatch__IsVerbose(this: *const TryCatch) -> bool;
    pub fn v8__TryCatch__SetVerbose(this: *mut TryCatch, value: bool);
    pub fn v8__TryCatch__SetCaptureMessage(this: *mut TryCatch, value: bool);
  }
}

mod raw_unused {
  use super::*;

  #[repr(C)]
  pub struct EscapableHandleScope([usize; 4]);

  extern "C" {
    fn v8__EscapableHandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<EscapableHandleScope>,
      isolate: *mut Isolate,
    );
    fn v8__EscapableHandleScope__DESTRUCT(this: *mut EscapableHandleScope);
    fn v8__EscapableHandleScope__GetIsolate(
      this: &EscapableHandleScope,
    ) -> *mut Isolate;
    fn v8__EscapableHandleScope__Escape(
      this: *mut EscapableHandleScope,
      value: *const Data,
    ) -> *const Data;
  }
}

mod entered {
  use super::*;

  pub struct ContextScope<'a>(&'a mut ScopeState);
  pub struct HandleScope<'a>(&'a mut ScopeState);

}
