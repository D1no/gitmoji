# gitmoji.com

The home page consists of a single heading. It uses Fredoka at weight 600, the heading face on the folding sheets.

Install dependencies from the [website workspace](../README.md), then run the commands below from `website/landingpage/`.

| To open            | Command                          | Port | Example for a second worktree |
| ------------------ | -------------------------------- | ---- | ----------------------------- |
| Development server | `pnpm dev`                       | 5310 | `pnpm dev --port 5312`        |
| Production preview | `pnpm build` then `pnpm preview` | 5311 | `pnpm preview --port 5313`    |
| Storybook          | `pnpm storybook`                 | 6310 | `pnpm storybook --port 6311`  |

For a container preview, run `pnpm build` and `docker compose up --build`. The default loopback port is 13004. Give a second checkout its own Compose project name and a free port.

```sh
GITMOJI_LANDINGPAGE_PORT=13005 docker compose -p gitmoji-review up --build
```
