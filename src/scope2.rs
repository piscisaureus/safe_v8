use std::any::Any;
use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::replace;
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use crate::isolate::IsolateAnnex;
use crate::undefined;
use crate::Context;
use crate::Data;
use crate::Isolate;
use crate::Local;
use crate::Message;
use crate::Primitive;
use crate::TryCatch;
use crate::Value;

pub(self) mod reference {
  use super::*;

  pub struct ContextScope<'a, P> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<&'a mut P>,
  }
  pub struct HandleScope<'a, P> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<&'a mut P>,
  }
  pub struct EscapableHandleScope<'a, 'b: 'a> {
    scope_state: NonNull<data::ScopeState>,
    _phantom: PhantomData<(&'a mut raw::HandleScope, &'b raw::EscapeSlot)>,
  }
}

pub(crate) mod data {
  use super::*;

  pub(crate) struct ScopeState {
    pub(super) isolate: NonNull<Isolate>,
    pub(super) context: Option<NonNull<Context>>,
    pub(super) escape_slot: Option<NonNull<raw::EscapeSlot>>,
    parent: Option<NonNull<ScopeState>>,
    data: ScopeData,
  }

  impl ScopeState {
    // TODO(piscisaureus): use something more efficient than a separate heap
    // allocation for every scope.
    fn new_common(isolate: &mut Isolate, data: ScopeData) -> Box<Self> {
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
      let state = Self {
        isolate: NonNull::from(isolate),
        parent,
        context,
        escape_slot,
        data,
      };
      Box::new(state)
    }

    pub(super) fn new_context_scope(
      isolate: &mut Isolate,
      context: Local<Context>,
    ) -> Box<Self> {
      let mut state = Self::new_common(isolate, ScopeData::Context);
      state.context = NonNull::new(&*context as *const Context as *mut Context);
      state
    }

    pub(super) fn new_handle_scope(isolate: &mut Isolate) -> Box<Self> {
      let mut state = Self::new_common(
        isolate,
        ScopeData::HandleScope(raw::HandleScope::uninit()),
      );
      match &mut state.data {
        ScopeData::HandleScope(raw) => raw.init(isolate),
        _ => unreachable!(),
      };
      state
    }

    pub(super) fn new_escapable_handle_scope(
      isolate: &mut Isolate,
    ) -> Box<Self> {
      let mut state = Self::new_common(
        isolate,
        ScopeData::EscapableHandleScope(raw::EscapableHandleScope::uninit()),
      );
      match &mut state.data {
        ScopeData::EscapableHandleScope(raw) => {
          state.escape_slot = raw.init(isolate);
        }
        _ => unreachable!(),
      };
      state
    }
  }

  pub(super) enum ScopeData {
    None,
    Context,
    HandleScope(raw::HandleScope),
    EscapableHandleScope(raw::EscapableHandleScope),
  }

  impl Default for ScopeData {
    fn default() -> Self {
      Self::None
    }
  }
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
    pub(super) fn uninit() -> Self {
      unsafe { MaybeUninit::<Self>::zeroed().assume_init() }
    }

    pub(super) fn init(&mut self, isolate: *mut Isolate) {
      assert!(self.isolate_.is_null());
      let buf = self as *mut _ as *mut MaybeUninit<Self>;
      unsafe { v8__HandleScope__CONSTRUCT(buf, isolate) };
    }
  }

  impl Drop for HandleScope {
    fn drop(&mut self) {
      assert!(!self.isolate_.is_null());
      unsafe { v8__HandleScope__DESTRUCT(self) };
    }
  }

  pub(super) struct EscapeSlot(Option<NonNull<raw::Address>>);

  impl EscapeSlot {
    fn uninit() -> Self {
      Self(None)
    }

    fn init(&mut self, isolate: *mut Isolate) {
      unsafe {
        let undefined = v8__Undefined(isolate);
        let local = v8__Local__New(isolate, undefined as *const _);
        let address = &*local as *const _ as *mut raw::Address;
        let slot = NonNull::new_unchecked(address);
        let none = self.0.replace(slot.cast());
        debug_assert!(none.is_none());
      }
    }
  }

  pub(super) struct EscapableHandleScope {
    handle_scope: raw::HandleScope,
    escape_slot: raw::EscapeSlot,
  }

  impl EscapableHandleScope {
    pub(super) fn uninit() -> Self {
      Self {
        handle_scope: HandleScope::uninit(),
        escape_slot: EscapeSlot::uninit(),
      }
    }

    pub(super) fn init(
      &mut self,
      isolate: *mut Isolate,
    ) -> Option<NonNull<EscapeSlot>> {
      // Note: the `EscapeSlot` must be initialized *before* the `HandleScope`.
      self.escape_slot.init(isolate);
      self.handle_scope.init(isolate);
      Some(NonNull::from(&mut self.escape_slot))
    }
  }

  extern "C" {
    pub(super) fn v8__Isolate__GetCurrentContext(
      isolate: *mut Isolate,
    ) -> *const Context;
    pub(super) fn v8__Isolate__GetEnteredOrMicrotaskContext(
      isolate: *mut Isolate,
    ) -> *const Context;

    pub(super) fn v8__Context__GetIsolate(this: *const Context)
      -> *mut Isolate;
    pub(super) fn v8__Context__Enter(this: *const Context);
    pub(super) fn v8__Context__Exit(this: *const Context);

    pub(super) fn v8__HandleScope__CONSTRUCT(
      buf: *mut MaybeUninit<HandleScope>,
      isolate: *mut Isolate,
    );
    pub(super) fn v8__HandleScope__DESTRUCT(this: *mut HandleScope);

    pub(super) fn v8__Undefined(isolate: *mut Isolate) -> *const Primitive;
    pub(super) fn v8__Local__New(
      isolate: *mut Isolate,
      other: *const Data,
    ) -> *const Data;

    pub(super) fn v8__TryCatch__CONSTRUCT(
      buf: *mut MaybeUninit<TryCatch>,
      isolate: *mut Isolate,
    );
    pub(super) fn v8__TryCatch__DESTRUCT(this: *mut TryCatch);
    pub(super) fn v8__TryCatch__HasCaught(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__CanContinue(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__HasTerminated(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__Exception(
      this: *const TryCatch,
    ) -> *const Value;
    pub(super) fn v8__TryCatch__StackTrace(
      this: *const TryCatch,
      context: *const Context,
    ) -> *const Value;
    pub(super) fn v8__TryCatch__Message(
      this: *const TryCatch,
    ) -> *const Message;
    pub(super) fn v8__TryCatch__Reset(this: *mut TryCatch);
    pub(super) fn v8__TryCatch__ReThrow(this: *mut TryCatch) -> *const Value;
    pub(super) fn v8__TryCatch__IsVerbose(this: *const TryCatch) -> bool;
    pub(super) fn v8__TryCatch__SetVerbose(this: *mut TryCatch, value: bool);
    pub(super) fn v8__TryCatch__SetCaptureMessage(
      this: *mut TryCatch,
      value: bool,
    );
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
