let rust_common_program = () => `\
#![allow(warnings)]

use std::any::type_name;

use rusty_v8::Local;
use rusty_v8::CallbackScope;
use rusty_v8::Context;
use rusty_v8::OwnedIsolate;
use rusty_v8::ContextScope;
use rusty_v8::EscapableHandleScope;
use rusty_v8::HandleScope;
use rusty_v8::Isolate;
use rusty_v8::TryCatchNew as TryCatch;

fn mock<T>() -> T {
  unimplemented!();
}

fn mock_mut<'a, T>() -> &'a mut T {
  unimplemented!();
}

fn result_type_name<R, F: FnOnce() -> R>(_: F) -> &'static str {
  std::any::type_name::<R>()
}
`;

let try_types_program = (inner_type, outer_type, ...extra_params) => `\
${rust_common_program()}

fn main() {
  let make = || {
    ${outer_type}::new(
      mock_mut::<${inner_type}>()
      ${extra_params.map((p) => `, mock::<${p}>()`).join(``)}
    )
  };
  let s = result_type_name(make);
  println!("{}", s);
}
`;

let try_deref_program = (type) => `\
${rust_common_program()}


fn main() {
  let make = || {
    let t = mock::<${type}>();
    let d = Deref::deref(&t);
    unsafe { std::ptr::read(d) }
  };
  let s = result_type_name(make);
  println!("{}", s);
}
`;

let try_as_mut_program = (inner_type, outer_type_lt, lifetimes) => `\
${rust_common_program()}

fn check<
  ${lifetimes
    .map((l, idx) =>
      idx === 0 ? `'${l}, ` : `'${l}: '${lifetimes[idx - 1]}, `
    )
    .join("")}
  R: AsMut<${outer_type_lt}>,
  F: FnOnce() -> R
>(_: F) {}

fn main() {
  check(mock::<${inner_type}>);
  println!("ok");
}
`;

function add_lifetimes(type, lifetimes = []) {
  let new_lifetime = () =>
    (lifetimes[lifetimes.length] = String.fromCharCode(
      "a".charCodeAt(0) + lifetimes.length
    ));
  type = type.replace(
    /(EscapableHandleScope|\bHandleScope|ContextScope|CallbackScope|TryCatch)(?!<)/g,
    (s) => `${s}<>`
  );
  type = type.replace(
    /(EscapableHandleScope|\bHandleScope|ContextScope|CallbackScope|TryCatch)</g,
    (s, m1) =>
      `${s}'${new_lifetime()}, ` +
      (m1 === "EscapableHandleScope" ? `'${new_lifetime()}, ` : "")
  );
  type = type.replace(/, >/g, ">");
  return type;
}

const assert = require("assert");
const { execFileSync } = require("child_process");
const { mkdirSync, writeFileSync } = require("fs");
const { dirname, resolve } = require("path");

const rust_src_file = resolve(__dirname, "examples", "gen.rs");
mkdirSync(dirname(rust_src_file), { recursive: true });

function try_program(src, stderr = "ignore") {
  writeFileSync(rust_src_file, src);
  try {
    r = execFileSync("cargo", ["run", "--example", "gen"], {
      cwd: __dirname,
      encoding: "utf8",
      stdio: ["ignore", "pipe", stderr],
    });
    r = r
      .replace(/(^[\s\n\r]+)|([\s\n\r]+$)/g, "")
      .replace(/\brusty_v8::([a-z0-9_]+::)*/g, "");
    assert(r.length > 0);
    return r;
  } catch (err) {
    return null;
  }
}

const type_info_map = new Map();
const todo_inner_types = new Set();
const done_inner_types = new Set();

function try_new_types(inner_type, outer_type, ...extra) {
  let src = try_types_program(inner_type, outer_type, ...extra);
  let r = try_program(src);
  console.log(`${inner_type} + ${outer_type} = ${r}`);
  type_info_map.get(inner_type).new_scope.set(outer_type, r);
  if (r != null && !done_inner_types.has(r)) {
    todo_inner_types.add(r);
  }
}

function try_as_mut(inner_type, outer_type) {
  let lifetimes = [];
  let outer_type_lt = add_lifetimes(outer_type, lifetimes);
  const src = try_as_mut_program(inner_type, outer_type_lt, lifetimes);
  const r = try_program(src, "ignore") != null;
  console.log(`${inner_type}: AsMut<${outer_type}> => ${r}`);
  type_info_map.get(inner_type).as_mut.set(outer_type, r);
}

function try_deref(type) {
  const src = try_deref_program(type);
  const r = try_program(src);
  console.log(`${type}::Target = ${r}`);
  type_info_map.get(type).deref = r;
  if (r != null && !done_inner_types.has(r)) {
    todo_inner_types.add(r);
  }
}

function try_inner_type(inner_type) {
  if (!type_info_map.has(inner_type)) {
    type_info_map.set(inner_type, { new_scope: new Map(), as_mut: new Map() });
  }
  try_new_types(inner_type, "ContextScope", "Local<Context>");
  try_new_types(inner_type, "HandleScope");
  try_new_types(inner_type, "EscapableHandleScope");
  try_new_types(inner_type, "TryCatch");
  try_deref(inner_type);
  done_inner_types.add(inner_type);
  todo_inner_types.delete(inner_type);
}

try_inner_type("Isolate");
try_inner_type("CallbackScope");

while (todo_inner_types.size > 0) {
  for (const ty of todo_inner_types) {
    try_inner_type(ty);
    try_as_mut(ty, ty);
    break;
  }
}

for (const t1 of done_inner_types) {
  for (const t2 of done_inner_types) {
    try_as_mut(t1, t2);
  }
}

console.log(type_info_map);
