use crate::isolate::Isolate;
use crate::support::int;
use crate::support::MapFnTo;
use crate::support::MaybeBool;
use crate::AccessorNameGetterCallback;
use crate::Array;
use crate::Context;
use crate::HandleScope;
use crate::Local;
use crate::Map;
use crate::Name;
use crate::Object;
use crate::PropertyAttribute;
use crate::Value;

extern "C" {
  fn v8__Object__New(isolate: *mut Isolate) -> *const Object;
  fn v8__Object__New__with_prototype_and_properties(
    isolate: *mut Isolate,
    prototype_or_null: *const Value,
    names: *const *const Name,
    values: *const *const Value,
    length: usize,
  ) -> *const Object;
  fn v8__Object__SetAccessor(
    this: *const Object,
    context: *const Context,
    key: *const Name,
    getter: AccessorNameGetterCallback,
  ) -> MaybeBool;
  fn v8__Object__Get(
    this: *const Object,
    context: *const Context,
    key: *const Value,
  ) -> *const Value;
  fn v8__Object__GetIndex(
    this: *const Object,
    context: *const Context,
    index: u32,
  ) -> *const Value;
  fn v8__Object__GetPrototype(this: *const Object) -> *const Value;
  fn v8__Object__Set(
    this: *const Object,
    context: *const Context,
    key: *const Value,
    value: *const Value,
  ) -> MaybeBool;
  fn v8__Object__SetIndex(
    this: *const Object,
    context: *const Context,
    index: u32,
    value: *const Value,
  ) -> MaybeBool;
  fn v8__Object__SetPrototype(
    this: *const Object,
    context: *const Context,
    prototype: *const Value,
  ) -> MaybeBool;
  fn v8__Object__CreateDataProperty(
    this: *const Object,
    context: *const Context,
    key: *const Name,
    value: *const Value,
  ) -> MaybeBool;
  fn v8__Object__DefineOwnProperty(
    this: *const Object,
    context: *const Context,
    key: *const Name,
    value: *const Value,
    attr: PropertyAttribute,
  ) -> MaybeBool;
  fn v8__Object__GetIdentityHash(this: *const Object) -> int;
  fn v8__Object__CreationContext(this: *const Object) -> *const Context;
  fn v8__Object__GetOwnPropertyNames(
    this: *const Object,
    context: *const Context,
  ) -> *const Array;
  fn v8__Object__GetPropertyNames(
    this: *const Object,
    context: *const Context,
  ) -> *const Array;

  fn v8__Array__New(isolate: *mut Isolate, length: int) -> *const Array;
  fn v8__Array__New_with_elements(
    isolate: *mut Isolate,
    elements: *const *const Value,
    length: usize,
  ) -> *const Array;
  fn v8__Array__Length(array: *const Array) -> u32;
  fn v8__Map__Size(map: *const Map) -> usize;
  fn v8__Map__As__Array(this: *const Map) -> *const Array;
}

impl Object {
  /// Creates an empty object.
  pub fn new<'s>(scope: &mut HandleScope<'s>) -> Local<'s, Object> {
    unsafe {
      scope.cast_local(|scope| v8__Object__New(scope.get_isolate_ptr()))
    }
    .unwrap()
  }

  /// Creates a JavaScript object with the given properties, and the given
  /// prototype_or_null (which can be any JavaScript value, and if it's null,
  /// the newly created object won't have a prototype at all). This is similar
  /// to Object.create(). All properties will be created as enumerable,
  /// configurable and writable properties.
  pub fn with_prototype_and_properties<'s>(
    scope: &mut HandleScope<'s>,
    prototype_or_null: Local<'s, Value>,
    names: &[Local<Name>],
    values: &[Local<Value>],
  ) -> Local<'s, Object> {
    assert_eq!(names.len(), values.len());
    let names = Local::slice_into_raw(names);
    let values = Local::slice_into_raw(values);
    unsafe {
      scope.cast_local(|scope| {
        v8__Object__New__with_prototype_and_properties(
          scope.get_isolate_ptr(),
          &*prototype_or_null,
          names.as_ptr(),
          values.as_ptr(),
          names.len(),
        )
      })
    }
    .unwrap()
  }

  /// Set only return Just(true) or Empty(), so if it should never fail, use
  /// result.Check().
  pub fn set(
    &self,
    context: Local<Context>,
    key: Local<Value>,
    value: Local<Value>,
  ) -> Option<bool> {
    unsafe { v8__Object__Set(self, &*context, &*key, &*value) }.into()
  }

  /// Set only return Just(true) or Empty(), so if it should never fail, use
  /// result.Check().
  pub fn set_index(
    &self,
    context: Local<Context>,
    index: u32,
    value: Local<Value>,
  ) -> Option<bool> {
    unsafe { v8__Object__SetIndex(self, &*context, index, &*value) }.into()
  }

  /// Set the prototype object. This does not skip objects marked to be
  /// skipped by proto and it does not consult the security handler.
  pub fn set_prototype(
    &self,
    context: Local<Context>,
    prototype: Local<Value>,
  ) -> Option<bool> {
    unsafe { v8__Object__SetPrototype(self, &*context, &*prototype) }.into()
  }

  /// Implements CreateDataProperty (ECMA-262, 7.3.4).
  ///
  /// Defines a configurable, writable, enumerable property with the given value
  /// on the object unless the property already exists and is not configurable
  /// or the object is not extensible.
  ///
  /// Returns true on success.
  pub fn create_data_property(
    &self,
    context: Local<Context>,
    key: Local<Name>,
    value: Local<Value>,
  ) -> Option<bool> {
    unsafe { v8__Object__CreateDataProperty(self, &*context, &*key, &*value) }
      .into()
  }

  /// Implements DefineOwnProperty.
  ///
  /// In general, CreateDataProperty will be faster, however, does not allow
  /// for specifying attributes.
  ///
  /// Returns true on success.
  pub fn define_own_property(
    &self,
    context: Local<Context>,
    key: Local<Name>,
    value: Local<Value>,
    attr: PropertyAttribute,
  ) -> Option<bool> {
    unsafe {
      v8__Object__DefineOwnProperty(self, &*context, &*key, &*value, attr)
    }
    .into()
  }

  pub fn get<'s>(
    &self,
    scope: &mut HandleScope<'s>,
    context: Local<Context>,
    key: Local<Value>,
  ) -> Option<Local<'s, Value>> {
    unsafe { scope.cast_local(|_| v8__Object__Get(self, &*context, &*key)) }
  }

  pub fn get_index<'s>(
    &self,
    scope: &mut HandleScope<'s>,
    context: Local<Context>,
    index: u32,
  ) -> Option<Local<'s, Value>> {
    unsafe {
      scope.cast_local(|_| v8__Object__GetIndex(self, &*context, index))
    }
  }

  /// Get the prototype object. This does not skip objects marked to be
  /// skipped by proto and it does not consult the security handler.
  pub fn get_prototype<'s>(
    &self,
    scope: &mut HandleScope<'s>,
  ) -> Option<Local<'s, Value>> {
    unsafe { scope.cast_local(|_| v8__Object__GetPrototype(self)) }
  }

  /// Note: SideEffectType affects the getter only, not the setter.
  pub fn set_accessor(
    &mut self,
    context: Local<Context>,
    name: Local<Name>,
    getter: impl for<'s> MapFnTo<AccessorNameGetterCallback<'s>>,
  ) -> Option<bool> {
    unsafe {
      v8__Object__SetAccessor(self, &*context, &*name, getter.map_fn_to())
    }
    .into()
  }

  /// Returns the identity hash for this object. The current implementation
  /// uses a hidden property on the object to store the identity hash.
  ///
  /// The return value will never be 0. Also, it is not guaranteed to be
  /// unique.
  pub fn get_identity_hash(&self) -> int {
    unsafe { v8__Object__GetIdentityHash(self) }
  }

  /// Returns the context in which the object was created.
  pub fn creation_context<'s>(
    &self,
    scope: &mut HandleScope<'s>,
  ) -> Local<'s, Context> {
    unsafe { scope.cast_local(|_| v8__Object__CreationContext(self)) }.unwrap()
  }

  /// This function has the same functionality as GetPropertyNames but the
  /// returned array doesn't contain the names of properties from prototype
  /// objects.
  pub fn get_own_property_names<'s>(
    &self,
    scope: &mut HandleScope<'s>,
    context: Local<Context>,
  ) -> Option<Local<'s, Array>> {
    unsafe {
      scope.cast_local(|_| v8__Object__GetOwnPropertyNames(self, &*context))
    }
  }

  /// Returns an array containing the names of the filtered properties of this
  /// object, including properties from prototype objects. The array returned by
  /// this method contains the same values as would be enumerated by a for-in
  /// statement over this object.
  pub fn get_property_names<'s>(
    &self,
    scope: &mut HandleScope<'s>,
    context: Local<Context>,
  ) -> Option<Local<'s, Array>> {
    unsafe {
      scope.cast_local(|_| v8__Object__GetPropertyNames(self, &*context))
    }
  }
}

impl Array {
  /// Creates a JavaScript array with the given length. If the length
  /// is negative the returned array will have length 0.
  pub fn new<'s>(scope: &mut HandleScope<'s>, length: i32) -> Local<'s, Array> {
    unsafe {
      scope.cast_local(|scope| v8__Array__New(scope.get_isolate_ptr(), length))
    }
    .unwrap()
  }

  /// Creates a JavaScript array out of a Local<Value> array with a known length.
  pub fn new_with_elements<'s>(
    scope: &mut HandleScope<'s>,
    elements: &[Local<Value>],
  ) -> Local<'s, Array> {
    if elements.is_empty() {
      return Self::new(scope, 0);
    }
    let elements = Local::slice_into_raw(elements);
    unsafe {
      scope.cast_local(|scope| {
        v8__Array__New_with_elements(
          scope.get_isolate_ptr(),
          elements.as_ptr(),
          elements.len(),
        )
      })
    }
    .unwrap()
  }

  pub fn length(&self) -> u32 {
    unsafe { v8__Array__Length(self) }
  }
}

impl Map {
  pub fn size(&self) -> usize {
    unsafe { v8__Map__Size(self) }
  }
  /// Returns an array of length size() * 2, where index N is the Nth key and
  /// index N + 1 is the Nth value.
  pub fn as_array<'s>(&self, scope: &mut HandleScope<'s>) -> Local<'s, Array> {
    unsafe { scope.cast_local(|_| v8__Map__As__Array(self)) }.unwrap()
  }
}
