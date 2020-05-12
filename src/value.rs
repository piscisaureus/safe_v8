use crate::support::Maybe;
use crate::BigInt;
use crate::Context;
use crate::HandleScope;
use crate::Int32;
use crate::Integer;
use crate::Local;
use crate::Number;
use crate::Object;
use crate::String;
use crate::Uint32;
use crate::Value;

extern "C" {
  fn v8__Value__IsUndefined(this: *const Value) -> bool;
  fn v8__Value__IsNull(this: *const Value) -> bool;
  fn v8__Value__IsNullOrUndefined(this: *const Value) -> bool;
  fn v8__Value__IsTrue(this: *const Value) -> bool;
  fn v8__Value__IsFalse(this: *const Value) -> bool;
  fn v8__Value__IsName(this: *const Value) -> bool;
  fn v8__Value__IsString(this: *const Value) -> bool;
  fn v8__Value__IsSymbol(this: *const Value) -> bool;
  fn v8__Value__IsFunction(this: *const Value) -> bool;
  fn v8__Value__IsArray(this: *const Value) -> bool;
  fn v8__Value__IsObject(this: *const Value) -> bool;
  fn v8__Value__IsBigInt(this: *const Value) -> bool;
  fn v8__Value__IsBoolean(this: *const Value) -> bool;
  fn v8__Value__IsNumber(this: *const Value) -> bool;
  fn v8__Value__IsExternal(this: *const Value) -> bool;
  fn v8__Value__IsInt32(this: *const Value) -> bool;
  fn v8__Value__IsUint32(this: *const Value) -> bool;
  fn v8__Value__IsDate(this: *const Value) -> bool;
  fn v8__Value__IsArgumentsObject(this: *const Value) -> bool;
  fn v8__Value__IsBigIntObject(this: *const Value) -> bool;
  fn v8__Value__IsBooleanObject(this: *const Value) -> bool;
  fn v8__Value__IsNumberObject(this: *const Value) -> bool;
  fn v8__Value__IsStringObject(this: *const Value) -> bool;
  fn v8__Value__IsSymbolObject(this: *const Value) -> bool;
  fn v8__Value__IsNativeError(this: *const Value) -> bool;
  fn v8__Value__IsRegExp(this: *const Value) -> bool;
  fn v8__Value__IsAsyncFunction(this: *const Value) -> bool;
  fn v8__Value__IsGeneratorFunction(this: *const Value) -> bool;
  fn v8__Value__IsGeneratorObject(this: *const Value) -> bool;
  fn v8__Value__IsPromise(this: *const Value) -> bool;
  fn v8__Value__IsMap(this: *const Value) -> bool;
  fn v8__Value__IsSet(this: *const Value) -> bool;
  fn v8__Value__IsMapIterator(this: *const Value) -> bool;
  fn v8__Value__IsSetIterator(this: *const Value) -> bool;
  fn v8__Value__IsWeakMap(this: *const Value) -> bool;
  fn v8__Value__IsWeakSet(this: *const Value) -> bool;
  fn v8__Value__IsArrayBuffer(this: *const Value) -> bool;
  fn v8__Value__IsArrayBufferView(this: *const Value) -> bool;
  fn v8__Value__IsTypedArray(this: *const Value) -> bool;
  fn v8__Value__IsUint8Array(this: *const Value) -> bool;
  fn v8__Value__IsUint8ClampedArray(this: *const Value) -> bool;
  fn v8__Value__IsInt8Array(this: *const Value) -> bool;
  fn v8__Value__IsUint16Array(this: *const Value) -> bool;
  fn v8__Value__IsInt16Array(this: *const Value) -> bool;
  fn v8__Value__IsUint32Array(this: *const Value) -> bool;
  fn v8__Value__IsInt32Array(this: *const Value) -> bool;
  fn v8__Value__IsFloat32Array(this: *const Value) -> bool;
  fn v8__Value__IsFloat64Array(this: *const Value) -> bool;
  fn v8__Value__IsBigInt64Array(this: *const Value) -> bool;
  fn v8__Value__IsBigUint64Array(this: *const Value) -> bool;
  fn v8__Value__IsDataView(this: *const Value) -> bool;
  fn v8__Value__IsSharedArrayBuffer(this: *const Value) -> bool;
  fn v8__Value__IsProxy(this: *const Value) -> bool;
  fn v8__Value__IsWasmModuleObject(this: *const Value) -> bool;
  fn v8__Value__IsModuleNamespaceObject(this: *const Value) -> bool;
  fn v8__Value__StrictEquals(this: *const Value, that: *const Value) -> bool;
  fn v8__Value__SameValue(this: *const Value, that: *const Value) -> bool;

  fn v8__Value__ToBigInt(
    this: *const Value,
    context: *const Context,
  ) -> *const BigInt;
  fn v8__Value__ToNumber(
    this: *const Value,
    context: *const Context,
  ) -> *const Number;
  fn v8__Value__ToString(
    this: *const Value,
    context: *const Context,
  ) -> *const String;
  fn v8__Value__ToDetailString(
    this: *const Value,
    context: *const Context,
  ) -> *const String;
  fn v8__Value__ToObject(
    this: *const Value,
    context: *const Context,
  ) -> *const Object;
  fn v8__Value__ToInteger(
    this: *const Value,
    context: *const Context,
  ) -> *const Integer;
  fn v8__Value__ToUint32(
    this: *const Value,
    context: *const Context,
  ) -> *const Uint32;
  fn v8__Value__ToInt32(
    this: *const Value,
    context: *const Context,
  ) -> *const Int32;

  fn v8__Value__NumberValue(
    this: *const Value,
    context: *const Context,
    out: *mut Maybe<f64>,
  );
  fn v8__Value__IntegerValue(
    this: *const Value,
    context: *const Context,
    out: *mut Maybe<i64>,
  );
  fn v8__Value__Uint32Value(
    this: *const Value,
    context: *const Context,
    out: *mut Maybe<u32>,
  );
  fn v8__Value__Int32Value(
    this: *const Value,
    context: *const Context,
    out: *mut Maybe<i32>,
  );
}

impl Value {
  /// Returns true if this value is the undefined value.  See ECMA-262 4.3.10.
  pub fn is_undefined(&self) -> bool {
    unsafe { v8__Value__IsUndefined(self) }
  }

  /// Returns true if this value is the null value.  See ECMA-262 4.3.11.
  pub fn is_null(&self) -> bool {
    unsafe { v8__Value__IsNull(self) }
  }

  /// Returns true if this value is either the null or the undefined value.
  /// See ECMA-262 4.3.11. and 4.3.12
  pub fn is_null_or_undefined(&self) -> bool {
    unsafe { v8__Value__IsNullOrUndefined(self) }
  }

  /// Returns true if this value is true.
  /// This is not the same as `BooleanValue()`. The latter performs a
  /// conversion to boolean, i.e. the result of `Boolean(value)` in JS, whereas
  /// this checks `value === true`.
  pub fn is_true(&self) -> bool {
    unsafe { v8__Value__IsTrue(self) }
  }

  /// Returns true if this value is false.
  /// This is not the same as `!BooleanValue()`. The latter performs a
  /// conversion to boolean, i.e. the result of `!Boolean(value)` in JS, whereas
  /// this checks `value === false`.
  pub fn is_false(&self) -> bool {
    unsafe { v8__Value__IsFalse(self) }
  }

  /// Returns true if this value is a symbol or a string.
  /// This is equivalent to
  /// `typeof value === 'string' || typeof value === 'symbol'` in JS.
  pub fn is_name(&self) -> bool {
    unsafe { v8__Value__IsName(self) }
  }

  /// Returns true if this value is an instance of the String type.
  /// See ECMA-262 8.4.
  pub fn is_string(&self) -> bool {
    unsafe { v8__Value__IsString(self) }
  }

  /// Returns true if this value is a symbol.
  /// This is equivalent to `typeof value === 'symbol'` in JS.
  pub fn is_symbol(&self) -> bool {
    unsafe { v8__Value__IsSymbol(self) }
  }

  /// Returns true if this value is a function.
  pub fn is_function(&self) -> bool {
    unsafe { v8__Value__IsFunction(self) }
  }

  /// Returns true if this value is an array. Note that it will return false for
  /// an Proxy for an array.
  pub fn is_array(&self) -> bool {
    unsafe { v8__Value__IsArray(self) }
  }

  /// Returns true if this value is an object.
  pub fn is_object(&self) -> bool {
    unsafe { v8__Value__IsObject(self) }
  }

  /// Returns true if this value is a bigint.
  /// This is equivalent to `typeof value === 'bigint'` in JS.
  pub fn is_big_int(&self) -> bool {
    unsafe { v8__Value__IsBigInt(self) }
  }

  /// Returns true if this value is boolean.
  /// This is equivalent to `typeof value === 'boolean'` in JS.
  pub fn is_boolean(&self) -> bool {
    unsafe { v8__Value__IsBoolean(self) }
  }

  /// Returns true if this value is a number.
  pub fn is_number(&self) -> bool {
    unsafe { v8__Value__IsNumber(self) }
  }

  /// Returns true if this value is an `External` object.
  pub fn is_external(&self) -> bool {
    unsafe { v8__Value__IsExternal(self) }
  }

  /// Returns true if this value is a 32-bit signed integer.
  pub fn is_int32(&self) -> bool {
    unsafe { v8__Value__IsInt32(self) }
  }

  /// Returns true if this value is a 32-bit unsigned integer.
  pub fn is_uint32(&self) -> bool {
    unsafe { v8__Value__IsUint32(self) }
  }

  /// Returns true if this value is a Date.
  pub fn is_date(&self) -> bool {
    unsafe { v8__Value__IsDate(self) }
  }

  /// Returns true if this value is an Arguments object.
  pub fn is_arguments_object(&self) -> bool {
    unsafe { v8__Value__IsArgumentsObject(self) }
  }

  /// Returns true if this value is a BigInt object.
  pub fn is_big_int_object(&self) -> bool {
    unsafe { v8__Value__IsBigIntObject(self) }
  }

  /// Returns true if this value is a Boolean object.
  pub fn is_boolean_object(&self) -> bool {
    unsafe { v8__Value__IsBooleanObject(self) }
  }

  /// Returns true if this value is a Number object.
  pub fn is_number_object(&self) -> bool {
    unsafe { v8__Value__IsNumberObject(self) }
  }

  /// Returns true if this value is a String object.
  pub fn is_string_object(&self) -> bool {
    unsafe { v8__Value__IsStringObject(self) }
  }

  /// Returns true if this value is a Symbol object.
  pub fn is_symbol_object(&self) -> bool {
    unsafe { v8__Value__IsSymbolObject(self) }
  }

  /// Returns true if this value is a NativeError.
  pub fn is_native_error(&self) -> bool {
    unsafe { v8__Value__IsNativeError(self) }
  }

  /// Returns true if this value is a RegExp.
  pub fn is_reg_exp(&self) -> bool {
    unsafe { v8__Value__IsRegExp(self) }
  }

  /// Returns true if this value is an async function.
  pub fn is_async_function(&self) -> bool {
    unsafe { v8__Value__IsAsyncFunction(self) }
  }

  /// Returns true if this value is a Generator function.
  pub fn is_generator_function(&self) -> bool {
    unsafe { v8__Value__IsGeneratorFunction(self) }
  }

  /// Returns true if this value is a Promise.
  pub fn is_promise(&self) -> bool {
    unsafe { v8__Value__IsPromise(self) }
  }

  /// Returns true if this value is a Map.
  pub fn is_map(&self) -> bool {
    unsafe { v8__Value__IsMap(self) }
  }

  /// Returns true if this value is a Set.
  pub fn is_set(&self) -> bool {
    unsafe { v8__Value__IsSet(self) }
  }

  /// Returns true if this value is a Map Iterator.
  pub fn is_map_iterator(&self) -> bool {
    unsafe { v8__Value__IsMapIterator(self) }
  }

  /// Returns true if this value is a Set Iterator.
  pub fn is_set_iterator(&self) -> bool {
    unsafe { v8__Value__IsSetIterator(self) }
  }

  /// Returns true if this value is a WeakMap.
  pub fn is_weak_map(&self) -> bool {
    unsafe { v8__Value__IsWeakMap(self) }
  }

  /// Returns true if this value is a WeakSet.
  pub fn is_weak_set(&self) -> bool {
    unsafe { v8__Value__IsWeakSet(self) }
  }

  /// Returns true if this value is an ArrayBuffer.
  pub fn is_array_buffer(&self) -> bool {
    unsafe { v8__Value__IsArrayBuffer(self) }
  }

  /// Returns true if this value is an ArrayBufferView.
  pub fn is_array_buffer_view(&self) -> bool {
    unsafe { v8__Value__IsArrayBufferView(self) }
  }

  /// Returns true if this value is one of TypedArrays.
  pub fn is_typed_array(&self) -> bool {
    unsafe { v8__Value__IsTypedArray(self) }
  }

  /// Returns true if this value is an Uint8Array.
  pub fn is_uint8_array(&self) -> bool {
    unsafe { v8__Value__IsUint8Array(self) }
  }

  /// Returns true if this value is an Uint8ClampedArray.
  pub fn is_uint8_clamped_array(&self) -> bool {
    unsafe { v8__Value__IsUint8ClampedArray(self) }
  }

  /// Returns true if this value is an Int8Array.
  pub fn is_int8_array(&self) -> bool {
    unsafe { v8__Value__IsInt8Array(self) }
  }

  /// Returns true if this value is an Uint16Array.
  pub fn is_uint16_array(&self) -> bool {
    unsafe { v8__Value__IsUint16Array(self) }
  }

  /// Returns true if this value is an Int16Array.
  pub fn is_int16_array(&self) -> bool {
    unsafe { v8__Value__IsInt16Array(self) }
  }

  /// Returns true if this value is an Uint32Array.
  pub fn is_uint32_array(&self) -> bool {
    unsafe { v8__Value__IsUint32Array(self) }
  }

  /// Returns true if this value is an Int32Array.
  pub fn is_int32_array(&self) -> bool {
    unsafe { v8__Value__IsInt32Array(self) }
  }

  /// Returns true if this value is a Float32Array.
  pub fn is_float32_array(&self) -> bool {
    unsafe { v8__Value__IsFloat32Array(self) }
  }

  /// Returns true if this value is a Float64Array.
  pub fn is_float64_array(&self) -> bool {
    unsafe { v8__Value__IsFloat64Array(self) }
  }

  /// Returns true if this value is a BigInt64Array.
  pub fn is_big_int64_array(&self) -> bool {
    unsafe { v8__Value__IsBigInt64Array(self) }
  }

  /// Returns true if this value is a BigUint64Array.
  pub fn is_big_uint64_array(&self) -> bool {
    unsafe { v8__Value__IsBigUint64Array(self) }
  }

  /// Returns true if this value is a DataView.
  pub fn is_data_view(&self) -> bool {
    unsafe { v8__Value__IsDataView(self) }
  }

  /// Returns true if this value is a SharedArrayBuffer.
  /// This is an experimental feature.
  pub fn is_shared_array_buffer(&self) -> bool {
    unsafe { v8__Value__IsSharedArrayBuffer(self) }
  }

  /// Returns true if this value is a JavaScript Proxy.
  pub fn is_proxy(&self) -> bool {
    unsafe { v8__Value__IsProxy(self) }
  }

  /// Returns true if this value is a WasmModuleObject.
  pub fn is_wasm_module_object(&self) -> bool {
    unsafe { v8__Value__IsWasmModuleObject(self) }
  }

  /// Returns true if the value is a Module Namespace Object.
  pub fn is_module_namespace_object(&self) -> bool {
    unsafe { v8__Value__IsModuleNamespaceObject(self) }
  }

  pub fn strict_equals<'sc>(&self, that: Local<'sc, Value>) -> bool {
    unsafe { v8__Value__StrictEquals(self, &*that) }
  }

  pub fn same_value<'sc>(&self, that: Local<'sc, Value>) -> bool {
    unsafe { v8__Value__SameValue(self, &*that) }
  }

  pub fn to_big_int<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, BigInt>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToBigInt(self, &*context))
    })
  }

  pub fn to_number<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, Number>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToNumber(self, &*context))
    })
  }

  pub fn to_string<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, String>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToString(self, &*context))
    })
  }

  pub fn to_detail_string<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, String>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToDetailString(self, &*context))
    })
  }

  pub fn to_object<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, Object>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToObject(self, &*context))
    })
  }

  pub fn to_integer<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, Integer>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToInteger(self, &*context))
    })
  }

  pub fn to_uint32<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, Uint32>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToUint32(self, &*context))
    })
  }

  pub fn to_int32<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<Local<'sc, Int32>> {
    scope.get_current_context().and_then(|context| unsafe {
      Local::from_raw(v8__Value__ToInt32(self, &*context))
    })
  }

  pub fn number_value<'sc>(&self, scope: &mut HandleScope<'sc>) -> Option<f64> {
    scope.get_current_context().and_then(|context| unsafe {
      let mut out = Maybe::<f64>::default();
      v8__Value__NumberValue(self, &*context, &mut out);
      out.into()
    })
  }

  pub fn integer_value<'sc>(
    &self,
    scope: &mut HandleScope<'sc>,
  ) -> Option<i64> {
    scope.get_current_context().and_then(|context| unsafe {
      let mut out = Maybe::<i64>::default();
      v8__Value__IntegerValue(self, &*context, &mut out);
      out.into()
    })
  }

  pub fn uint32_value<'sc>(&self, scope: &mut HandleScope<'sc>) -> Option<u32> {
    scope.get_current_context().and_then(|context| unsafe {
      let mut out = Maybe::<u32>::default();
      v8__Value__Uint32Value(self, &*context, &mut out);
      out.into()
    })
  }

  pub fn int32_value<'sc>(&self, scope: &mut HandleScope<'sc>) -> Option<i32> {
    scope.get_current_context().and_then(|context| unsafe {
      let mut out = Maybe::<i32>::default();
      v8__Value__Int32Value(self, &*context, &mut out);
      out.into()
    })
  }
}
