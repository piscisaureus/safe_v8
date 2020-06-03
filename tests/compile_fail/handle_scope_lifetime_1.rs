// Copyright 2019-2020 the Deno authors. All rights reserved. MIT license.
use rusty_v8 as v8;

pub fn main() {
  let mut isolate = v8::Isolate::new(mock());
  let mut scope1 = v8::HandleScope::new(&mut isolate);
  let mut _scope2 = v8::EscapableHandleScope::new(&mut scope1);
  let _local = v8::Integer::new(&mut scope1, 123);
}

fn mock<T>() -> T {
  unimplemented!()
}
