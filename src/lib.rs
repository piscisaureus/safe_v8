// Copyright 2019-2020 the Deno authors. All rights reserved. MIT license.

//! # Example
//!
//! ```rust
//! use rusty_v8 as v8;
//!
//! let platform = v8::new_default_platform().unwrap();
//! v8::V8::initialize_platform(platform);
//! v8::V8::initialize();
//!
//! let mut isolate = v8::Isolate::new(Default::default());
//!
//! let ref mut scope = v8::HandleScope::new(&mut isolate);
//! let context = v8::Context::new(scope);
//! let ref mut scope = v8::ContextScope::new(scope, context);
//!
//! let code = v8::String::new(scope, "'Hello' + ' World!'").unwrap();
//! println!("javascript code: {}", code.to_rust_string_lossy(scope));
//!
//! let mut script = v8::Script::compile(scope, context, code, None).unwrap();
//! let result = script.run(scope, context).unwrap();
//! let result = result.to_string(scope).unwrap();
//! println!("result: {}", result.to_rust_string_lossy(scope));
//! ```

#![allow(clippy::missing_safety_doc)]
#![allow(dead_code)]

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate lazy_static;
extern crate libc;

mod array_buffer;
mod array_buffer_view;
mod context;
mod data;
mod exception;
mod external_references;
mod function;
mod global;
mod isolate;
mod isolate_create_params;
mod local;
mod module;
mod number;
mod object;
mod platform;
mod primitive_array;
mod primitives;
mod promise;
mod property_attribute;
mod proxy;
mod scope;
mod script;
mod script_or_module;
mod shared_array_buffer;
mod snapshot;
mod string;
mod support;
mod template;
mod try_catch;
mod uint8_array;
mod value;

pub mod inspector;
pub mod json;
pub mod script_compiler;
// This module is intentionally named "V8" rather than "v8" to match the
// C++ namespace "v8::V8".
#[allow(non_snake_case)]
pub mod V8;

pub use array_buffer::*;
pub use data::*;
pub use exception::*;
pub use external_references::ExternalReference;
pub use external_references::ExternalReferences;
pub use function::*;
pub use global::Global;
pub use isolate::HostImportModuleDynamicallyCallback;
pub use isolate::HostInitializeImportMetaObjectCallback;
pub use isolate::Isolate;
pub use isolate::IsolateHandle;
pub use isolate::MessageCallback;
pub use isolate::OwnedIsolate;
pub use isolate::PromiseRejectCallback;
pub use isolate_create_params::CreateParams;
pub use local::Local;
pub use module::*;
pub use object::*;
pub use platform::new_default_platform;
pub use platform::Platform;
pub use platform::Task;
// TODO(ry) TaskBase and TaskImpl ideally shouldn't be part of the public API.
pub use platform::TaskBase;
pub use platform::TaskImpl;
pub use primitives::*;
pub use promise::{PromiseRejectEvent, PromiseRejectMessage, PromiseState};
pub use property_attribute::*;
pub use proxy::*;
pub use scope::*;
pub use script::ScriptOrigin;
pub use snapshot::FunctionCodeHandling;
pub use snapshot::SnapshotCreator;
pub use snapshot::StartupData;
pub use string::NewStringType;
pub use support::SharedPtr;
pub use support::SharedRef;
pub use support::UniquePtr;
pub use support::UniqueRef;
pub use template::*;
pub use try_catch::{TryCatch, TryCatchScope};

// TODO(piscisaureus): Ideally this trait would not be exported.
pub use support::MapFnTo;
