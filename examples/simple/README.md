# htmx-axum-template

Adapted from https://github.com/igneous-labs/htmx-axum-template, prod docker build doesnt work due to path dep outside build context.

Template for web apps using:

- [htmx](https://htmx.org)
- [tailwindcss](https://tailwindcss.com/)
- [typescript](https://www.typescriptlang.org/)
- [axum](https://github.com/tokio-rs/axum) for server
- [minijinja](https://github.com/mitsuhiko/minijinja) for templating
- [vite](https://vitejs.dev/) for bundling js
- [bun](https://bun.sh) for js package management and runtime
- [pre-commit](https://pre-commit.com/) for linting both backend and frontend code

## Contents

- [htmx-axum-template](#htmx-axum-template)
  - [Contents](#contents)
  - [How this works](#how-this-works)
  - [Setup](#setup)
    - [Requirements](#requirements)
    - [Install pre-commit](#install-pre-commit)
    - [Install js dependencies](#install-js-dependencies)
  - [Development](#development)
    - [Adding static files](#adding-static-files)
  - [Production](#production)
    - [Build](#build)
    - [Run](#run)

<small><i><a href='http://ecotrust-canada.github.io/markdown-toc/'>Table of contents generated with markdown-toc</a></i></small>

## How this works

Directory structure:

```
├── app/            /* app/ contains the vite project */
├── Cargo.lock      /* top level is the rust package */
├── Cargo.toml
├── src/
```

All frontend files (html, css, ts, static assets) are stored in the `app/` folder.

Apart from running the htmx application, the axum server also statically serves all files in the `app/` dir next to it at path `/`.

At development time, a vite dev server runs at port 5173, serving all the frontend files in `app/`. The axum server runs on port 3000. The web app is accessed via the axum server's port, 3000.

All html templates include resources (ts, css, static assets) from `http://localhost:5173/`. Example:

```html
<script type="module" src="http://localhost:5173/ts/index.ts"></script>
```

They also import and run the vite client to connect to the vite dev server using this snippet:

```html
<script type="module">
  import "http://localhost:5173/@vite/client";
  window.process = { env: { NODE_ENV: "development" } };
</script>
```

This enables hot reloading.

At production build time (`vite build`), a custom [rollup transform plugin](https://rollupjs.org/plugin-development/#transform) configured in `vite.config.js` preprocesses all html files by removing the above vite client snippet. It also deletes all instances of `http://localhost:5173/`, resulting in all includes importing from `/` instead. For example, the above `<script>` snippet is transformed into:

```html
<script type="module" src="/ts/index.ts"></script>
```

This allows vite/rollup to proceed with the asset bundling correctly to create an output `dist/` folder to be statically served in an `app/` folder by our axum server.

This however introduce the following quirks:

- during development, both the vite dev server and the axum server must be running
- during development, `cargo run` must be run at the project root, else the html templates in the `app/` folder will not be served by the axum server
- in order to preserve the same directory structure at dev and prod time, we cannot use vite's [publicDir](https://vitejs.dev/config/shared-options.html#publicdir) feature

## Setup

### Requirements

- [bun](https://bun.sh/docs/installation)
- [rust](https://www.rust-lang.org/tools/install)

### Install pre-commit

`./install-precommit.sh`

### Install js dependencies

`cd app && bun install`

## Development

`dev.sh` runs `vite dev` in the background and `cargo run dev` in the foreground, killing both processes if interrupted (Ctrl+C)

### Adding static files

Add the files to `app/`, treating it as the `/` path, then update the `viteStaticCopy` plugin in `vite.config.js` to include them.

## Production

`Dockerfile` creates a production docker image from a minimal `scratch` image with the following directory structure:

```
├── app/         /* vite production build output (dist/) */
├── my-web-app   /* release-compiled rust binary, image's CMD */
```

- all images used in the multi-stage build are alpine-based
- `x86_64-unknown-linux-musl` is used as the rust server's compile target to create a fully statically-linked binary
- js dependencies in `node_modules/` are cached between builds (dependent on `app/package.json` and `app/bun.lockb` not changing)
- [cargo chef](https://github.com/LukeMathWalker/cargo-chef) is used to cache rust dependencies between builds (dependent on `Cargo.toml` and `Cargo.lock` not changing)

### Build

`docker build -t my-web-app .`

### Run

`docker run -p 3000:3000 --name my-web-app --init --rm my-web-app`

`--init` allows the running container to be killed with `ctrl+c`

`--rm` results in the container being deleted upon exit
