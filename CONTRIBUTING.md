# Contributing to Lunatic

Thanks for contributing to Lunatic!

Before continuing please read our [code of conduct][code-of-conduct] which all
contributors are expected to adhere to.

[code-of-conduct]: https://github.com/lunatic-solutions/lunatic/blob/main/CODE_OF_CONDUCT.md

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
2. Please use `cargo fmt` and `cargo clippy` to check that code is properly
   formatted, and linted for potential problems.
3. Changes, adding and removing host functions require changes to the
   `wat/all_imports.wat` file. Every host function lunatic exposes requires an
   import directive to assert that end developers can import the function.
4. Open a GitHub pull request with your changes and ensure the tests and build
   pass on CI.
5. A Lunatic team member will review the changes and may provide feedback to
   work on. Depending on the change there may be multiple rounds of feedback.
6. Once the changes have been approved the code will be rebased into the
   `main` branch.

## Local development

To build the project run:

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

## Changelog generation

The changelog is updated using the [git-cliff](https://git-cliff.org/) cli,
which generates the changelog file from the [Git](https://git-scm.com/) history by utilizing [conventional commits](https://git-cliff.org/#conventional_commits).

The changelog template is defined in [Cargo.toml](/Cargo.toml) under `[workspace.metadata.git-cliff.*]`.

Updating the CHANGELOG.md file can be achieved with the following command:

```bash
git cliff --config ./Cargo.toml --latest --prepend ./CHANGELOG.md
```

The commit types are as follows:

* **feat**: A new feature
* **fix**: A bug fix
* **docs**: Documentation only changes
* **style**: Changes that do not affect the meaning of the code (white-space, formatting, missing semi-colons, etc)
* **refactor**: A code change that neither fixes a bug nor adds a feature
* **perf**: A code change that improves performance
* **test**: Adding missing or correcting existing tests
* **chore**: Changes to the build process or auxiliary tools and libraries such as documentation generation

For more information, see the [git-cliff usage documentation](https://git-cliff.org/#usage).

## Useful resources
- [Project Loom on virtual threads](https://cr.openjdk.org/~rpressler/loom/loom/sol1_part1.html)
- [Erlang documentation](https://www.erlang.org/docs) - these explain some concepts that Lunatic implements
- [Notes on distributed systems](http://cs-www.cs.yale.edu/homes/aspnes/classes/465/notes.pdf) - explains some distributed algorithms (possibly useful for working on distributed Lunatic)
