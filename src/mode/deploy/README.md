# Lunatic platform CLI

This platform-related subset of lunatic CLI app is a command-line interface tool that allows you authenticate, manage, build and deploy programs to the lunatic platform

## Prerequisites

- Rust (version 1.6.0 or higher)
- lunatic vm and cli installation

## How to use

1. Create a new lunatic project (skip if you have an existing one)
```
lunatic auth -p `PROVIDER_HOSTNAME`
```

2. Initialise project. This will create a `lunatic.toml` file will contain data about remote project and it's apps
```
lunatic init
```

3. Bind your local directory to the remote project

```
lunatic project add `PROJECT_ID` `PROJECT_NAME`
```

4. List the apps in your project to verify the binding worked

```
lunatic app list
```

## Available Commands

- `auth`: Authenticate cli app with lunatic platform
- `app`: Subcommand that allows the user to manage apps on the platform
  - `add`: maps a cargo binary (either `main.rs` or Cargo.toml `[[bin]]` or Cargo.toml `workspace.member`) to a lunatic platform `App` within the defined `Project`
  - `remove`: removes an app on the remote and the local mapping
  - `list`: lists all the apps currently configured in the `Project`
- `deploy <PROJECT_NAME>|--all`: builds the selected wasm files and deploys the responding `Apps` to the platform
- `project`: Allows the user to manage the platform `Project` mapping
  - `set <PROJECT_ID>`: maps the local repository to a platform `Project`

## Example Usage
Once you have authenticated and configured to which project your local repository belongs to you can deploy all mapped apps

```
lunatic deploy --all # this will deploy all the mapped cargo.toml binaries as `Apps` to the platform
```

