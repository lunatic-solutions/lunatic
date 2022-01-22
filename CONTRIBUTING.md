# Contributing to Lunatic

Thanks for contributing to Lunatic!

Before continuing please read our [code of conduct][code-of-conduct] which all
contributors are expected to adhere to.

[code-of-conduct]: https://github.com/lunatic-lang/lunatic/blob/wasmtime/CODE_OF_CONDUCT.md

## Contributing bug reports

If you have found a bug in Lunatic please check to see if there is an open
ticket for this problem on [our GitHub issue tracker][issues]. If you cannot
find an existing ticket for the bug please open a new one.

[issues]: https://github.com/lunatic-lang/lunatic/issues

A bug may be a technical problem such as a compiler crash or an incorrect
return value from a library function, or a user experience issue such as
unclear or absent documentation. If you are unsure if your problem is a bug
please open a ticket and we will work it out together.

## Contributing code changes

Code changes to Lunatic are welcomed via the process below.

1. Find or open a GitHub issue relevant to the change you wish to make and
   comment saying that you wish to work on this issue. If the change
   introduces new functionality or behaviour this would be a good time to
   discuss the details of the change to ensure we are in agreement as to how
   the new functionality should work.
2. Open a GitHub pull request with your changes and ensure the tests and build
   pass on CI.
3. A Lunatic team member will review the changes and may provide feedback to
   work on. Depending on the change there may be multiple rounds of feedback.
4. Once the changes have been approved the code will be rebased into the
   `main` branch.

## Local development

To build the project you will need to have Rust target wasm32 installed:

```shell
rustup target add wasm32-unknown-unknown
```

then run:

```shell
cargo build
```

or for release builds:

```shell
cargo build --release
```

To run the tests:

```shell
cargo test
```

## Useful resources
- [Project Loom on virtual threads](http://cr.openjdk.java.net/~rpressler/loom/loom/sol1_part1.html)
- [Erlang documentation](https://www.erlang.org/docs) - these explain some concepts that Lunatic implements
- [Notes on distributed systems](http://cs-www.cs.yale.edu/homes/aspnes/classes/465/notes.pdf) - explains some distributed algorithms (possibly useful for working on distributed Lunatic)
