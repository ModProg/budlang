# Bud (budlang)

A safe, fast, lightweight embeddable scripting language written in Rust.

**WARNING: This crate is not anywhere near being ready to publish.**

[![crate version](https://img.shields.io/crates/v/budlang.svg)](https://crates.io/crates/budlang)
[![Live Build Status](https://img.shields.io/github/workflow/status/khonsulabs/budlang/Tests/main)](https://github.com/khonsulabs/budlang/actions?query=workflow:Tests)
[![HTML Coverage Report for `main` branch](https://khonsulabs.github.io/budlang/coverage/badge.svg)](https://khonsulabs.github.io/budlang/coverage/)
[![Documentation](https://img.shields.io/badge/docs-main-informational)](https://khonsulabs.github.io/budlang/main/budlang)

## Why Bud?

### Memory-safe

This crate forbids unsafe code (`#![forbid(unsafe_code)]`) and has no
dependencies.

### Safe to run untrusted code

The virtual machine invokes [`Environment::step()`](https://khonsulabs.github.io/budlang/main/budlang/vm/trait.Environment.html#tymethod.step) before each
instruction is exected. The environment can return
[`ExecutionBehavior::Pause`](https://khonsulabs.github.io/budlang/main/budlang/vm/enum.ExecutionBehavior.html#variant.Pause) to pause execution, and the state of the
virtual machine will be saved such that it can be resumed again.

**Work In Progress:** Bud will have various configuration
options including maximum stack size, maximum memory used, and more.

**Work In Progress:** Bud should never panic. This crate is in early
development, so many instances of `todo!()` and `unwrap()` abound. These will
all be replaced with descriptive errors instead of panics.

**Work In Progress:** Bud will only support these primitive types: integers,
real numbers (floating point), strings, lists, and maps. Bud will be able to be
extended to support additional features via Rust, placing the developer
embedding Bud in full control of what scripts can do.

### Efficient

Bud is a compiled language powered by its own virtual machine. Currently, Bud
has no optimizer, but the virtual machine code generated by the compiler closely
mirrors the syntax of the language. For example, the repository includes three
examples of a naive [Fibonacci number][fib] function implementation. The [Bud
version][fib-ex] looks like this:

```bud
function fibonacci(n)
    if n <= 2
        1
    else
        this(n - 1) + this(n - 2)
    end
end
```

Another example [shows an identical implementation][fib-vm] using hand-written
virtual machine instructions. Despite not having an optimizer, the compiled
`fibonacci()` function's code is nearly identical, having one extra (unreachable)
instruction:

|  # | hand-written          | # | compiled             |
|----|-----------------------|---|----------------------|
|  0 | `lte @0 2 jump 2`     | 0 | `lte @0 2 jump 3`    |
|  1 | `return 1`            | 1 | `return 1`           |
|    |                       | 2 | `jump 8`             |
|  2 | `sub @0 1 stack`      | 3 | `sub @0 1 stack`     |
|  3 | `recurse-call 1 $0`  | 4 | `recurse-call 1 $0` |
|  4 | `sub @0 2 stack`      | 5 | `sub @0 2 stack`     |
|  5 | `recurse-call 1 $1`  | 6 | `recurse-call 1 $1` |
|  6 | `add $0 $1 $$`    | 7 | `add $0 $1 $$`   |

## Why not Bud?

It probably doesn't do what you need (yet):

- [x] Don't panic in vm
- [ ] Don't panic in compiler
- [ ] Don't panic in parser
- [x] Support parenthesized expressions as terms
- [x] Add variables
- [ ] Add loops
- [x] Add Real (Float) type
- [ ] Add String type
- [ ] Add List type
- [ ] Add Map type
- [ ] Ability to write Rust functions
- [ ] Implement a REPL
- [ ] Consider static variables for persistent module state.

Bud is compiled to a virtual machine written using only memory-safe abstractions
in Rust. This yields quite good performance for a dynamically typed language,
but will not be the fastest language.

## Goals for Bud

Aside from the goals outlined above, the use cases it's being designed for are:

- A [BonsaiDb][bonsaidb] REPL
- Multi-player server-side scripting where user-submitted scripts are allowed.

[fib]: https://en.wikipedia.org/wiki/Fibonacci_number
[fib-ex]: https://github.com/khonsulabs/budlang/blob/main/examples/fib.rs
[fib-vm]: https://github.com/khonsulabs/budlang/blob/main/examples/fib-vm.rs
[bonsaidb]: https://bonsaidb.io/

## Open-source Licenses

This project, like all projects from [Khonsu Labs](https://khonsulabs.com/), are
open-source. This repository is available under the [MIT License](./LICENSE-MIT)
or the [Apache License 2.0](./LICENSE-APACHE).

To learn more about contributing, please see [CONTRIBUTING.md](./CONTRIBUTING.md).
