These are the MIT-licensed libspa-sys 0.8.0 and pipewire-sys 0.8.0 sources from
https://gitlab.freedesktop.org/pipewire/pipewire-rs. Only the Cargo build dependency
on bindgen changes to 0.72.1 (the obsolete experimental feature is removed).
This fixes opaque struct generation with libclang 22 and newer without changing
the runtime PipeWire API. See https://github.com/rust-lang/rust-bindgen/pull/3278.
