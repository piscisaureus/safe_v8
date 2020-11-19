Make sure xcode command line tools are installed.

```sh
sudo xcode-select --install
sudo xcode-select --reset
```

Make sure to first successfully build `rusty_v8`. It has to be built from
source.

```sh
V8_FROM_SOURCE=1 cargo build --all-targets --release -vv
```

Do black magic

```sh
cd target/release/gn_out/obj
ar -p librusty_v8.a binding.o | nm - --defined-only --extern-only --just-symbol-name | grep -v '^__' > librusty_v8_binding.exp
ld -r -arch x86_64 -exported_symbols_list librusty_v8_binding.exp -L. -lrusty_v8 -o librusty_v8_binding.o
libtool -static -o librusty_v8_binding.a librusty_v8_binding.o
```

Verify: the expected line count is somewhere in the 400-500 range.

```sh
nm --extern-only --defined-only --just-symbol-name --print-file-name librusty_v8_binding.a | wc -l
```

Verify: the `nm` output should look similar.

```sh
nm --extern-only --defined-only --just-symbol-name --print-file-name librusty_v8_binding.a
```

```
librusty_v8_binding.a:librusty_v8_binding.o: _std__shared_ptr__v8__ArrayBuffer__Allocator__CONVERT__std__unique_ptr
librusty_v8_binding.a:librusty_v8_binding.o: _std__shared_ptr__v8__ArrayBuffer__Allocator__COPY
librusty_v8_binding.a:librusty_v8_binding.o: _std__shared_ptr__v8__ArrayBuffer__Allocator__get
librusty_v8_binding.a:librusty_v8_binding.o: _std__shared_ptr__v8__ArrayBuffer__Allocator__reset
librusty_v8_binding.a:librusty_v8_binding.o: _std__shared_ptr__v8__ArrayBuffer__Allocator__use_count# «...snip...»
  <...snip... >
librusty_v8_binding.a:librusty_v8_binding.o: _v8_inspector__V8Inspector__Channel__sendResponse
librusty_v8_binding.a:librusty_v8_binding.o: _v8_inspector__V8Inspector__DELETE
librusty_v8_binding.a:librusty_v8_binding.o: _v8_inspector__V8Inspector__connect
librusty_v8_binding.a:librusty_v8_binding.o: _v8_inspector__V8Inspector__contextCreated
librusty_v8_binding.a:librusty_v8_binding.o: _v8_inspector__V8Inspector__create
```

# TODO

Make sure `librusty_v8_binding.a` contains the (dummy) symbol
`___gxx_personality_v0`, otherwise the generated library successfully links with
deno, but fails when building the rusty_v8 tests.
