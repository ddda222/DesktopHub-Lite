# next/ 依赖与构建产物

- `node_modules/`、`dist/` 不进入仓库；构建输出已由 `vite.config.ts` 指向 `D:/codex/DesktopHub-work/desktophub-lite-dist`。
- 安装与构建：在 `next/` 下执行 `pnpm install`、`pnpm run build`；开发预览 `pnpm run dev`（默认 `http://localhost:4317/`）。
- 后续 Tauri 落地时，`src-tauri/target` 同样应指向 `D:\codex\DesktopHub-work`，不要留在 C 盘或仓库内。
