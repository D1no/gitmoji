# Website

The pnpm workspace contains the [Gitmoji.com landing page](landingpage/README.md).
From this directory, use Node 24 or later and the pnpm version pinned in the manifest.

```sh
pnpm install --frozen-lockfile
pnpm dev
pnpm check
pnpm build
```

The development server listens on `127.0.0.1:5310`. The production files are in `landingpage/dist/`.
